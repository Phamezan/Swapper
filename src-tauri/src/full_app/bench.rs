//! Per-game averages for a champion in a role and rank tier (lolalytics), the
//! Dashboard's "vs <champion> <tier>" benchmark. One small JSON request per
//! champion, cached on disk for a day.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const TTL_MS: i64 = 24 * 60 * 60 * 1000;
const MAX_ENTRIES: usize = 300;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_PICKS: usize = 20;
/// Below this many games the averages are too noisy to compare against.
const MIN_GAMES: u64 = 500;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bench {
    pub kills: f64,
    pub deaths: f64,
    pub assists: f64,
    pub damage: f64,
    pub gold: f64,
    /// Lane minions plus jungle camps, like the dashboard's CS.
    pub cs: f64,
    pub games: u64,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    at: i64,
    bench: Bench,
}

/// The lolalytics bracket for a Solo/Duo tier: exact tiers up to Diamond,
/// Master+ above, every rank when unranked.
pub fn bracket(tier: &str) -> &'static str {
    match tier.to_ascii_uppercase().as_str() {
        "IRON" => "iron",
        "BRONZE" => "bronze",
        "SILVER" => "silver",
        "GOLD" => "gold",
        "PLATINUM" => "platinum",
        "EMERALD" => "emerald",
        "DIAMOND" => "diamond",
        "MASTER" | "GRANDMASTER" | "CHALLENGER" => "master_plus",
        _ => "all",
    }
}

pub fn parse(root: &Value) -> Option<Bench> {
    let games = root["header"]["n"].as_u64().filter(|n| *n >= MIN_GAMES)?;
    let stats: BTreeMap<&str, f64> = root["sidebar"]["stats"]["stats"]
        .as_array()?
        .iter()
        .filter_map(|row| Some((row.get(0)?.as_str()?, row.get(2)?.as_f64()?)))
        .filter(|(_, value)| value.is_finite() && *value >= 0.0)
        .collect();
    Some(Bench {
        kills: *stats.get("kills")?,
        deaths: *stats.get("deaths")?,
        assists: *stats.get("assists")?,
        damage: *stats.get("damage")?,
        gold: *stats.get("gold")?,
        cs: stats.get("minions")? + stats.get("jungleCS").copied().unwrap_or(0.0),
        games,
    })
}

fn path() -> Option<PathBuf> {
    Some(
        crate::vault::data_root()
            .ok()?
            .join("dashboard-benchmarks.json"),
    )
}

fn load(path: &Path) -> BTreeMap<String, Entry> {
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
        return BTreeMap::new();
    }
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(path: &Path, mut entries: BTreeMap<String, Entry>, now: i64) {
    entries.retain(|_, e| now - e.at < TTL_MS);
    while entries.len() > MAX_ENTRIES {
        let oldest = entries
            .iter()
            .min_by_key(|(_, e)| e.at)
            .map(|(k, _)| k.clone());
        entries.remove(&oldest.unwrap_or_default());
    }
    if let Ok(bytes) = serde_json::to_vec(&entries) {
        let pending = path.with_extension("tmp");
        if std::fs::write(&pending, bytes).is_ok() {
            let _ = std::fs::rename(&pending, path);
        }
    }
}

/// Benchmarks for `(champion id, role)` pairs in the user's `tier`, keyed
/// `"<id>|<role>"`. Pairs without enough data are left out.
#[tauri::command]
pub async fn dashboard_benchmarks(
    picks: Vec<(i64, String)>,
    tier: String,
) -> Result<BTreeMap<String, Bench>, String> {
    let bracket = bracket(&tier);
    let now = crate::runes::provider::now_ms();
    let file = path();
    let cached = match file.clone() {
        Some(file) => tokio::task::spawn_blocking(move || load(&file))
            .await
            .unwrap_or_default(),
        None => BTreeMap::new(),
    };
    let mut picks = picks;
    picks.sort();
    picks.dedup();
    picks.truncate(MAX_PICKS);
    let mut found = BTreeMap::new();
    let mut missing = Vec::new();
    for (id, role) in picks {
        let Some(lane) = crate::runes::lolalytics::lane(&role) else {
            continue;
        };
        let key = format!("{id}|{role}");
        match cached
            .get(&format!("{key}|{bracket}"))
            .filter(|e| now - e.at < TTL_MS)
        {
            Some(entry) => {
                found.insert(key, entry.bench.clone());
            }
            None => missing.push((id, lane, key)),
        }
    }
    if missing.is_empty() {
        return Ok(found);
    }
    let names = crate::runes::data::champion_names()
        .await
        .map_err(|e| e.to_string())?;
    let client = crate::runes::lolalytics::LolalyticsClient::new().map_err(|e| format!("{e:?}"))?;
    // ponytail: one request at a time; at most 20 champions, then cached for a day.
    let mut fetched: Vec<(String, Bench)> = Vec::new();
    for (id, lane, key) in missing {
        let Some(slug) = names
            .get(&id)
            .and_then(|n| crate::runes::lolalytics::champion_slug(n))
        else {
            continue;
        };
        let url = format!("https://a1.lolalytics.com/mega/?ep=build-full&v=1&c={slug}&lane={lane}&tier={bracket}&queue=ranked&region=all");
        let Ok(body) = client.page(url).await else {
            continue;
        };
        if let Some(bench) = serde_json::from_str(&body).ok().as_ref().and_then(parse) {
            fetched.push((key, bench));
        }
    }
    if let (Some(file), false) = (file, fetched.is_empty()) {
        let mut entries = cached;
        for (key, bench) in &fetched {
            entries.insert(
                format!("{key}|{bracket}"),
                Entry {
                    at: now,
                    bench: bench.clone(),
                },
            );
        }
        let _ = tokio::task::spawn_blocking(move || save(&file, entries, now)).await;
    }
    found.extend(fetched);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_sidebar_averages_and_rejects_thin_samples() {
        let root = json!({
            "header": { "n": 66516 },
            "sidebar": { "stats": { "stats": [
                ["damage", 1, 24797, 43.7, 55], ["kills", 0, 7.03, 50.5, 50], ["deaths", 0, 5.46, 35.4, 78],
                ["assists", 0, 7.39, 43.0, 29], ["gold", 1, 11955, 52.5, 64], ["minions", 0, 204.31, 72.9, 37],
                ["jungleCS", 0, 1.18, 5.0, 91]
            ]}}
        });
        let bench = parse(&root).unwrap();
        assert_eq!(
            (bench.kills, bench.damage, bench.games),
            (7.03, 24797.0, 66516)
        );
        assert!((bench.cs - 205.49).abs() < 1e-9);
        let mut thin = root.clone();
        thin["header"]["n"] = json!(120);
        assert!(parse(&thin).is_none());
        let mut missing = root;
        missing["sidebar"]["stats"]["stats"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        assert!(parse(&missing).is_none());
    }

    #[test]
    fn brackets_follow_the_rank() {
        assert_eq!(bracket("DIAMOND"), "diamond");
        assert_eq!(bracket("grandmaster"), "master_plus");
        assert_eq!(bracket(""), "all");
    }

    #[test]
    fn cache_drops_expired_entries_and_stays_bounded() {
        let path = std::env::temp_dir().join(format!("bench-test-{}.json", uuid::Uuid::new_v4()));
        let bench = parse(&json!({"header":{"n":1000},"sidebar":{"stats":{"stats":[
            ["damage",1,1],["kills",0,1],["deaths",0,1],["assists",0,1],["gold",1,1],["minions",0,1]]}}}))
        .unwrap();
        let mut entries = BTreeMap::new();
        for i in 0..(MAX_ENTRIES as i64 + 10) {
            entries.insert(
                format!("{i}"),
                Entry {
                    at: TTL_MS + i,
                    bench: bench.clone(),
                },
            );
        }
        entries.insert("old".into(), Entry { at: 0, bench });
        save(&path, entries, TTL_MS + 400);
        let saved = load(&path);
        assert_eq!(saved.len(), MAX_ENTRIES);
        assert!(!saved.contains_key("old") && !saved.contains_key("0"));
        let _ = std::fs::remove_file(path);
    }
}
