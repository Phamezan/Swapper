//! Bot lane duos (ADC + Support) from u.gg's duo tier list.
//!
//! `stats2.u.gg/lol/1.5/duos/world/{16_20}/ranked_solo_5x5/{rank}/1.5.0.json`
//! is `[{"adc_supp": [row, ...], ...}, updated_at, ...]`, each row
//! `[adc_id, role, adc_wins, adc_games, support_id, role, support_wins,
//! support_games, duo_wins, duo_games, ...]`. u.gg already drops small pairs
//! (a few thousand games at Emerald+). Pairs are blended with the previous
//! patch and scored like the lane tier list (shrunk win rate + popularity).

use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::cache::{self, LastGoodCache};
use super::patch::{self, blend_rate};
use super::provider::{DataKind, Provider, ProviderError, Sourced};
use super::tierlist::{scores, tier_index};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
const TTL: Duration = Duration::from_secs(60 * 60);
const LAST_GOOD_MAX_ENTRIES: usize = 16;
const LAST_GOOD_MAX_BYTES: usize = 512 * 1024;
/// Pick-rate weight in a duo's score. Lower than the lane tier list's: a duo
/// list is read for the pairs that win, and u.gg already drops rare ones.
const POP_WEIGHT: f64 = 0.25;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Duo {
    pub adc_id: i64,
    pub support_id: i64,
    pub win_pct: f64,
    /// The current patch's games together (0 for a previous-patch-only pair).
    pub games: u64,
    /// Our tier index (0 = S+, 5 = D).
    pub tier: u8,
    pub score: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DuoList {
    /// Best first.
    pub duos: Vec<Duo>,
}

/// u.gg's bracket for one of ours. u.gg has no Gold+ duo list, so it reads
/// Platinum+, the closest one it has.
fn rank_slug(tier: &str) -> &'static str {
    match super::lolalytics::tier_slug(tier) {
        "all" => "overall",
        "gold_plus" | "platinum_plus" => "platinum_plus",
        "diamond_plus" => "diamond_plus",
        "master_plus" => "master_plus",
        "challenger" => "challenger",
        _ => "emerald_plus",
    }
}

pub fn url(patch: &str, tier: &str) -> String {
    format!(
        "https://stats2.u.gg/lol/1.5/duos/world/{}/ranked_solo_5x5/{}/1.5.0.json",
        patch.replace('.', "_"),
        rank_slug(tier)
    )
}

/// The unscored pairs of one duo file, `None` when it has no `adc_supp` table.
fn parse(body: &str) -> Option<DuoList> {
    let root: Value = serde_json::from_str(body).ok()?;
    let rows = root.get(0)?.get("adc_supp")?.as_array()?;
    let duos = rows
        .iter()
        .filter_map(|row| {
            let at = |index: usize| row.get(index).and_then(Value::as_f64);
            let games = at(9).filter(|games| *games > 0.0)?;
            Some(Duo {
                adc_id: at(0)? as i64,
                support_id: at(4)? as i64,
                win_pct: 100.0 * at(8)? / games,
                games: games as u64,
                ..Duo::default()
            })
        })
        .collect();
    Some(DuoList { duos })
}

/// Blends each pair's win rate with the previous patch's (games stay the
/// current patch's), adds pairs only the previous patch has, then scores and
/// sorts best-first. Pick rate is each pair's share of the listed games.
fn blend(now: DuoList, prev: Option<&DuoList>) -> DuoList {
    let same = |a: &Duo, b: &Duo| a.adc_id == b.adc_id && a.support_id == b.support_id;
    let mut duos: Vec<Duo> = now
        .duos
        .iter()
        .map(|duo| match prev.and_then(|prev| prev.duos.iter().find(|old| same(old, duo))) {
            Some(old) => Duo { win_pct: blend_rate(duo.win_pct, duo.games, old.win_pct), ..duo.clone() },
            None => duo.clone(),
        })
        .collect();
    let prev_only: Vec<Duo> = prev
        .map(|prev| prev.duos.iter().filter(|old| !now.duos.iter().any(|duo| same(duo, old))).cloned().collect())
        .unwrap_or_default();
    // Previous-patch pairs keep their old games for popularity only, scaled
    // to the current list's size so they neither dominate nor vanish.
    let now_total: u64 = now.duos.iter().map(|duo| duo.games).sum();
    let prev_total: u64 = prev.map(|prev| prev.duos.iter().map(|duo| duo.games).sum()).unwrap_or(0);
    let scale = if now_total == 0 || prev_total == 0 { 1.0 } else { now_total as f64 / prev_total as f64 };
    let mut weights: Vec<f64> = duos.iter().map(|duo| duo.games as f64).collect();
    weights.extend(prev_only.iter().map(|old| old.games as f64 * scale));
    duos.extend(prev_only.into_iter().map(|old| Duo { games: 0, ..old }));

    let total: f64 = weights.iter().sum::<f64>().max(1.0);
    let stats: Vec<(f64, f64, u64)> = duos
        .iter()
        .zip(&weights)
        .map(|(duo, weight)| (duo.win_pct, 100.0 * weight / total, *weight as u64))
        .collect();
    for (duo, score) in duos.iter_mut().zip(scores(&stats, POP_WEIGHT)) {
        duo.score = score;
        duo.tier = tier_index(score);
    }
    duos.sort_by(|left, right| right.score.total_cmp(&left.score));
    DuoList { duos }
}

async fn fetch(patch: &str, tier: &str) -> Result<Option<DuoList>, ProviderError> {
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| ProviderError::Unavailable)?;
    let response = client.get(url(patch, tier)).send().await.map_err(|e| ProviderError::from_reqwest(&e))?;
    if !response.status().is_success() {
        return Err(ProviderError::from_status(response.status()));
    }
    let body = response.text().await.map_err(|_| ProviderError::Unavailable)?;
    parse(&body).map(Some).ok_or(ProviderError::Format)
}

static LAST_GOOD: OnceLock<LastGoodCache<DuoList>> = OnceLock::new();

fn last_good() -> &'static LastGoodCache<DuoList> {
    LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(cache::cache_path(DataKind::Duos), TTL, LAST_GOOD_MAX_ENTRIES, LAST_GOOD_MAX_BYTES)
    })
}

/// The bot lane duo list for a rank bracket. A fresh cached copy is served as
/// is; a failed fetch falls back to the last good copy, marked stale. On the
/// first day of a patch u.gg may not have the new file yet, so the previous
/// patch alone is shown.
pub async fn load(tier: &str) -> Option<Sourced<DuoList>> {
    let key = format!("duos|{}", rank_slug(tier));
    let cached = last_good().lookup(&key);
    if let Some(hit) = cached.as_ref().filter(|hit| hit.fresh) {
        return Some(Sourced { value: hit.value.clone(), provider: Provider::Ugg, stale: false, fetched_at: Some(hit.fetched_at) });
    }
    let stale = || cached.clone().map(|hit| Sourced::stale(hit.value, Provider::Ugg, hit.fetched_at));
    let Some(patches) = patch::patches().await else {
        return stale();
    };
    let now = fetch(&patches.current, tier).await.ok().flatten();
    let prev = match &patches.previous {
        Some(previous) => patch::prior(last_good(), &patch::prior_key(&key, previous), fetch(previous, tier)).await,
        None => None,
    };
    if now.is_none() && prev.is_none() {
        return stale();
    }
    let found = blend(now.unwrap_or_default(), prev.as_ref());
    last_good().store(key, found.clone());
    Some(Sourced::fresh(found, Provider::Ugg))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[{"adc_supp":[
        [222,3,102124,195868,201,2,34536,66177,4226,7539,146968,null,null],
        [51,3,1000,2000,412,2,1000,2000,6855,13314,1,null,null],
        [18,3,1000,2000,555,2,1000,2000,0,0,1,null,null]
    ],"jungle_supp":[]},"2026-10-13T17:47:47Z",1.0,1]"#;

    fn duo(adc_id: i64, support_id: i64, win_pct: f64, games: u64) -> Duo {
        Duo { adc_id, support_id, win_pct, games, ..Duo::default() }
    }

    #[test]
    fn parses_the_adc_support_pairs_and_skips_empty_ones() {
        let list = parse(SAMPLE).unwrap();
        assert_eq!(list.duos.len(), 2);
        assert_eq!((list.duos[0].adc_id, list.duos[0].support_id, list.duos[0].games), (222, 201, 7539));
        assert!((list.duos[0].win_pct - 56.055).abs() < 0.01);
        assert!(parse("<html>").is_none());
    }

    #[test]
    fn a_strong_popular_pair_tops_the_list_and_a_previous_only_pair_has_zero_games() {
        let now = DuoList { duos: vec![duo(1, 2, 54.0, 20_000), duo(3, 4, 49.0, 5_000), duo(5, 6, 50.0, 8_000)] };
        let prev = DuoList { duos: vec![duo(3, 4, 47.0, 9_000), duo(7, 8, 53.0, 9_000)] };
        let list = blend(now, Some(&prev));
        assert_eq!((list.duos[0].adc_id, list.duos[0].support_id), (1, 2));
        assert!(list.duos.windows(2).all(|pair| pair[0].score >= pair[1].score));
        let old = list.duos.iter().find(|duo| duo.adc_id == 7).unwrap();
        assert_eq!(old.games, 0);
        // 5,000 current games dominate the 400-game prior.
        let blended = list.duos.iter().find(|duo| duo.adc_id == 3).unwrap();
        assert!(blended.win_pct < 49.0 && blended.win_pct > 48.5);
    }

    #[test]
    fn urls_use_u_gg_patch_and_rank_names() {
        assert_eq!(
            url("16.20", "emerald_plus"),
            "https://stats2.u.gg/lol/1.5/duos/world/16_20/ranked_solo_5x5/emerald_plus/1.5.0.json"
        );
        assert!(url("16.20", "all").contains("/overall/"));
        assert!(url("16.20", "gold_plus").contains("/platinum_plus/"));
    }

    /// Run with `cargo test --lib -- --ignored --nocapture prints_the_live_duos`.
    #[tokio::test]
    #[ignore = "network: fetches the live emerald_plus duo list"]
    async fn prints_the_live_duos() {
        let patches = patch::patches().await.expect("patch names");
        let now = fetch(&patches.current, "emerald_plus").await.expect("a fetch").expect("a list");
        let list = blend(now, None);
        let mut counts = std::collections::BTreeMap::new();
        for duo in &list.duos {
            *counts.entry(crate::runes::tierlist::tier_label(duo.tier)).or_insert(0usize) += 1;
        }
        for duo in list.duos.iter().take(15) {
            println!("{:>4}+{:<4} {:<2} wr={:.2} games={} score={:.2}", duo.adc_id, duo.support_id, crate::runes::tierlist::tier_label(duo.tier), duo.win_pct, duo.games, duo.score);
        }
        println!("{} pairs, tiers {counts:?}", list.duos.len());
    }
}
