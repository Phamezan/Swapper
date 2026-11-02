mod model;
use model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

const BASE_URL: &str = "https://raw.githubusercontent.com/Phamezan/swapper-guides/dist";
const REFRESH_SECONDS: u64 = 6 * 60 * 60;
include!(concat!(env!("OUT_DIR"), "/bundled_guides.rs"));

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefreshState {
    index: Option<u64>,
    champions: BTreeMap<i64, u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedChampion { sha256: String, content: String }

struct GuideCache {
    root: Option<PathBuf>,
    index: Option<GuideIndex>,
    refresh: RefreshState,
    // Keep last-good bytes in memory too when the data directory is not writable.
    champions: BTreeMap<i64, CachedChampion>,
    base: String,
    offline: &'static [(&'static str, &'static [u8])],
}

fn read_limited(path: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> bool {
    let Some(parent) = path.parent() else { return false };
    if std::fs::create_dir_all(parent).is_err() { return false; }
    let pending = path.with_extension("tmp");
    let written = std::fs::write(&pending, bytes).is_ok() && std::fs::rename(&pending, path).is_ok();
    let _ = std::fs::remove_file(pending);
    written
}

fn due(previous: Option<u64>, now: u64) -> bool {
    previous.is_none_or(|at| now < at || now - at >= REFRESH_SECONDS)
}

impl GuideCache {
    fn open(root: Option<PathBuf>, base: String, offline: &'static [(&'static str, &'static [u8])]) -> Self {
        let index = root.as_ref().and_then(|p| read_limited(&p.join("index.json"), MAX_INDEX_BYTES)).and_then(|b| parse_index(&b));
        let refresh = root.as_ref().and_then(|p| read_limited(&p.join("refresh.json"), MAX_INDEX_BYTES))
            .and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { root, index, refresh, champions: BTreeMap::new(), base, offline }
    }

    fn bundled(&self, name: &str) -> Option<&'static [u8]> {
        self.offline.iter().find(|(key, _)| *key == name).map(|(_, bytes)| *bytes)
    }

    fn persist_refresh(&self) {
        if let (Some(root), Ok(bytes)) = (&self.root, serde_json::to_vec(&self.refresh)) {
            write_atomic(&root.join("refresh.json"), &bytes);
        }
    }

    fn cached(&mut self, champion: i64) -> Option<(String, Vec<u8>)> {
        if !self.champions.contains_key(&champion) {
            let bytes = read_limited(&self.root.as_ref()?.join(format!("champions/{champion}.json")), MAX_CHAMPION_BYTES * 6 + 1024)?;
            let cached: CachedChampion = serde_json::from_slice(&bytes).ok()?;
            self.champions.insert(champion, cached);
        }
        let cached = self.champions.get(&champion)?;
        verified_champion(cached.content.as_bytes(), &cached.sha256, champion).map(|_| (cached.sha256.clone(), cached.content.as_bytes().to_vec()))
    }

    async fn content(&mut self, client: &reqwest::Client, champion: i64, now: u64) -> Vec<u8> {
        if due(self.refresh.index, now) {
            self.refresh.index = Some(now);
            // Persist failures as attempts too: a 404/offline install does not hammer the host.
            self.persist_refresh();
            if let Some((bytes, index)) = fetch(client, &format!("{}/index.json", self.base), MAX_INDEX_BYTES).await
                .and_then(|b| parse_index(&b).map(|i| (b, i))) {
                if let Some(root) = &self.root { write_atomic(&root.join("index.json"), &bytes); }
                self.index = Some(index);
            }
        }
        let cached = self.cached(champion);
        let offline_index = self.bundled("index.json").and_then(parse_index);
        let index = self.index.as_ref().or(offline_index.as_ref());
        // A published index is authoritative when an author removes a champion.
        if index.is_some_and(|i| !i.champions.contains_key(&champion.to_string())) { return vec![]; }
        let expected = index.and_then(|i| i.champions.get(&champion.to_string())).map(|e| e.sha256.clone());
        if let (Some(hash), Some((held, bytes))) = (&expected, &cached) {
            if held.eq_ignore_ascii_case(hash) { return bytes.clone(); }
        }
        if self.index.is_some() && due(self.refresh.champions.get(&champion).copied(), now) {
            self.refresh.champions.insert(champion, now);
            self.persist_refresh();
            if let Some(hash) = expected {
                if let Some(bytes) = fetch(client, &format!("{}/champions/{champion}.json", self.base), MAX_CHAMPION_BYTES).await
                    .filter(|b| verified_champion(b, &hash, champion).is_some()) {
                    let cached = CachedChampion { sha256: hash, content: String::from_utf8(bytes.clone()).expect("validated JSON is UTF-8") };
                    if let (Some(root), Ok(bytes)) = (&self.root, serde_json::to_vec(&cached)) {
                        write_atomic(&root.join(format!("champions/{champion}.json")), &bytes);
                    }
                    self.champions.insert(champion, cached);
                    return bytes;
                }
            }
        }
        if let Some((_, bytes)) = cached { return bytes; }
        offline_index.as_ref().and_then(|i| i.champions.get(&champion.to_string()))
            .and_then(|e| self.bundled(&format!("champions/{champion}.json")).and_then(|b| verified_champion(b, &e.sha256, champion).map(|_| b.to_vec())))
            .unwrap_or_default()
    }

    #[cfg(test)]
    async fn champion(&mut self, client: &reqwest::Client, champion: i64, now: u64) -> Vec<Guide> {
        parse_champion(&self.content(client, champion, now).await, champion).unwrap_or_default()
    }
}

async fn fetch(client: &reqwest::Client, url: &str, limit: usize) -> Option<Vec<u8>> {
    let mut response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    if response.content_length().is_some_and(|n| n > limit as u64) { return None; }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > limit { return None; }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

async fn champion_content(champion: i64) -> Option<Vec<u8>> {
    // ponytail: one fetch lock coalesces IPC bursts; per-champion locks if parallel guide loads matter.
    static CACHE: OnceLock<tokio::sync::Mutex<GuideCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| tokio::sync::Mutex::new(GuideCache::open(
        crate::vault::data_root().ok().map(|p| p.join("guides-cache")), BASE_URL.into(), BUNDLED_GUIDES)));
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(3)).timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none()).user_agent("Swapper/1.0 (matchup guides)").build().ok()?;
    let now = crate::runes::provider::now_ms().max(0) as u64 / 1000;
    Some(cache.lock().await.content(&client, champion, now).await)
}

async fn guide(champion: i64, enemy: i64, position: Lane) -> Option<Guide> {
    let mut guide = select_guide(parse_champion(&champion_content(champion).await?, champion)?, enemy, position)?;
    mark_outdated(&mut guide).await;
    Some(guide)
}

fn select_guide(guides: Vec<Guide>, enemy: i64, position: Lane) -> Option<Guide> {
    guides.iter().find(|g| g.enemy == Some(enemy) && g.lane == position)
        .or_else(|| guides.iter().find(|g| g.enemy.is_none() && g.lane == position)).cloned()
}

fn flag_tips(guide: &mut Guide, changes: &[(String, Vec<i64>)]) {
    let (champion, enemy, patch) = (guide.champion, guide.enemy.unwrap_or(guide.champion), guide.patch.clone());
    for tip in guide.tips_mut() {
        // Cooldowns resolve from live game data; never attach a stale-data warning to them.
        if tip.text.contains("{cd:") { continue; }
        let Some(authored) = patch_number(tip.patch.as_deref().unwrap_or(&patch)) else { continue };
        if changes.iter().any(|(p, ids)| patch_number(p).is_some_and(|p| p > authored) && (ids.contains(&champion) || ids.contains(&enemy))) {
            tip.outdated = Some(Outdated::Flag(true));
        }
    }
}

async fn mark_outdated(guide: &mut Guide) {
    let Ok(current) = crate::runes::patch_notes::latest_patch_notes(false, None).await else { return };
    let previous = current.available_patches.get(1).map(|p| p.version.clone());
    let mut notes = vec![current];
    if let Some(version) = previous {
        if let Ok(previous) = crate::runes::patch_notes::latest_patch_notes(false, Some(version)).await { notes.push(previous); }
    }
    let champions = crate::runes::champion_list().await;
    let changes = notes.into_iter().map(|n| {
        let ids = n.sections.iter().filter(|s| s.title.eq_ignore_ascii_case("champions"))
            .flat_map(|s| &s.entries).filter_map(|e| champions.iter().find(|c| c.name.eq_ignore_ascii_case(&e.name)).map(|c| c.id)).collect();
        (n.patch, ids)
    }).collect::<Vec<_>>();
    // Only positive matches in available notes are evidence. Missing older patches prove nothing.
    flag_tips(guide, &changes);
}

#[derive(Serialize)]
pub(crate) struct MatchupGuideResult { guide: Option<Guide>, record: Option<crate::matchup_records::Record> }

#[tauri::command]
pub(crate) async fn get_matchup_guide(app: tauri::AppHandle, champion_id: i64, enemy_champion_id: i64, position: String) -> Result<MatchupGuideResult, String> {
    if champion_id <= 0 || enemy_champion_id <= 0 || champion_id > i32::MAX as i64 || enemy_champion_id > i32::MAX as i64 { return Err("Unknown champion.".into()); }
    let lane = lane(&position).ok_or("Unknown position.")?;
    let (guide, record) = tokio::join!(guide(champion_id, enemy_champion_id, lane),
        tokio::task::spawn_blocking(move || crate::matchup_records::record(&app, champion_id, enemy_champion_id, &position)));
    Ok(MatchupGuideResult { guide, record: record.unwrap_or(None) })
}

#[tauri::command]
pub(crate) async fn get_matchup_vods(champion_id: i64, enemy_champion_id: i64, position: String) -> Result<Vec<MatchupVod>, String> {
    if champion_id <= 0 || enemy_champion_id <= 0 || champion_id > i32::MAX as i64 || enemy_champion_id > i32::MAX as i64 { return Err("Unknown champion.".into()); }
    let lane = lane(&position).ok_or("Unknown position.")?;
    Ok(parse_vods(&champion_content(champion_id).await.unwrap_or_default(), champion_id, enemy_champion_id, lane))
}

#[cfg(test)]
mod tests;
