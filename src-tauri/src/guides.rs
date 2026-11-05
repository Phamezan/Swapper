mod model;
use model::*;
use serde::{Deserialize, Serialize};
use futures_util::{stream, StreamExt};
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
    #[serde(default)] verified_index: Option<IndexVerification>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexVerification { at: u64, sha256: String }

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GuideLookup { Exact, Absent, Unknown }

#[derive(Clone, Serialize, Deserialize)]
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
        let bytes = root.as_ref().and_then(|p| read_limited(&p.join("index.json"), MAX_INDEX_BYTES));
        let index = bytes.as_deref().and_then(parse_index);
        let mut refresh: RefreshState = root.as_ref().and_then(|p| read_limited(&p.join("refresh.json"), MAX_INDEX_BYTES))
            .and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if !refresh.verified_index.as_ref().is_some_and(|v| bytes.as_ref().is_some_and(|b| digest(b) == v.sha256)) {
            refresh.verified_index = None;
        }
        Self { root, index, refresh, champions: BTreeMap::new(), base, offline }
    }

    fn install_index(&mut self, bytes: &[u8], index: GuideIndex, now: u64) {
        if let Some(root) = &self.root { write_atomic(&root.join("index.json"), bytes); }
        self.index = Some(index);
        self.refresh.verified_index = Some(IndexVerification { at: now, sha256: digest(bytes) });
        self.persist_refresh();
    }

    fn lookup(&self, champion: i64, enemy: i64, position: Lane, bytes: &[u8], now: u64) -> GuideLookup {
        let guides = parse_champion(bytes, champion).unwrap_or_default();
        if guides.iter().any(|g| g.enemy == Some(enemy) && g.lane == position) { return GuideLookup::Exact; }
        let fresh = self.refresh.verified_index.as_ref().is_some_and(|v| now >= v.at && now - v.at < REFRESH_SECONDS);
        let Some(index) = self.index.as_ref().filter(|_| fresh) else { return GuideLookup::Unknown };
        let Some(entry) = index.champions.get(&champion.to_string()) else { return GuideLookup::Absent };
        let raw_count = serde_json::from_slice::<serde_json::Value>(bytes).ok().and_then(|file| {
            Some(file["guides"].as_array()?.len() + match file.get("general") { Some(v) => v.as_array()?.len(), None => 0 })
        });
        // Dropped invalid/duplicate guide rows cannot prove absence, even with a valid file hash.
        if verified_champion(bytes, &entry.sha256, champion).is_some()
            && raw_count == Some(guides.len())
            && guides.iter().filter(|g| g.enemy.is_some()).count() == entry.guides
            && guides.iter().filter(|g| g.enemy.is_none()).count() == entry.general.unwrap_or(0) {
            GuideLookup::Absent
        } else { GuideLookup::Unknown }
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

    async fn refresh_index(&mut self, client: &reqwest::Client, now: u64) {
        if due(self.refresh.index, now) {
            self.refresh.index = Some(now);
            // Persist failures as attempts too: a 404/offline install does not hammer the host.
            self.persist_refresh();
            if let Some((bytes, index)) = fetch(client, &format!("{}/index.json", self.base), MAX_INDEX_BYTES).await
                .and_then(|b| parse_index(&b).map(|i| (b, i))) {
                self.install_index(&bytes, index, now);
            }
        }
    }

    fn store_champion(&mut self, champion: i64, cached: CachedChampion) {
        if let (Some(root), Ok(bytes)) = (&self.root, serde_json::to_vec(&cached)) {
            write_atomic(&root.join(format!("champions/{champion}.json")), &bytes);
        }
        self.champions.insert(champion, cached);
    }

    async fn content(&mut self, client: &reqwest::Client, champion: i64, now: u64) -> Vec<u8> {
        self.refresh_index(client, now).await;
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
                    self.store_champion(champion, cached);
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

struct SharedGuideCache {
    data: tokio::sync::Mutex<GuideCache>,
    index_load: tokio::sync::Mutex<()>,
    champion_loads: tokio::sync::Mutex<BTreeMap<i64, std::sync::Arc<tokio::sync::Mutex<()>>>>,
}
impl SharedGuideCache {
    fn new(data: GuideCache) -> Self {
        Self { data: tokio::sync::Mutex::new(data), index_load: tokio::sync::Mutex::new(()),
            champion_loads: tokio::sync::Mutex::new(BTreeMap::new()) }
    }

    async fn refresh_index(&self, client: &reqwest::Client, now: u64) {
        let _load = self.index_load.lock().await;
        let base = {
            let mut data = self.data.lock().await;
            if !due(data.refresh.index, now) { return; }
            data.refresh.index = Some(now);
            data.persist_refresh();
            data.base.clone()
        };
        if let Some((bytes, index)) = fetch(client, &format!("{base}/index.json"), MAX_INDEX_BYTES).await
            .and_then(|b| parse_index(&b).map(|i| (b, i))) {
            let mut data = self.data.lock().await;
            data.install_index(&bytes, index, now);
        }
    }

    async fn content(&self, client: &reqwest::Client, champion: i64, now: u64) -> Vec<u8> {
        self.refresh_index(client, now).await;
        let load = self.champion_loads.lock().await.entry(champion).or_default().clone();
        let _load = load.lock().await;
        let mut pending = {
            let mut data = self.data.lock().await;
            data.cached(champion);
            let pending = GuideCache {
                root: None, index: data.index.clone(), base: data.base.clone(), offline: data.offline,
                refresh: RefreshState { index: Some(now), champions: data.refresh.champions.get(&champion)
                    .map(|at| (champion, *at)).into_iter().collect(), verified_index: data.refresh.verified_index.clone() },
                champions: data.champions.get(&champion).cloned().map(|c| (champion, c)).into_iter().collect(),
            };
            // Record attempts before I/O so cancellation retains retry throttling.
            if data.index.as_ref().is_some_and(|index| index.champions.get(&champion.to_string())
                .is_some_and(|entry| pending.champions.get(&champion).is_none_or(|held|
                    !held.sha256.eq_ignore_ascii_case(&entry.sha256) || verified_champion(held.content.as_bytes(), &held.sha256, champion).is_none())))
                && due(data.refresh.champions.get(&champion).copied(), now) {
                data.refresh.champions.insert(champion, now);
                data.persist_refresh();
            }
            pending
        };
        let bytes = pending.content(client, champion, now).await;
        let mut data = self.data.lock().await;
        if let Some(cached) = pending.champions.remove(&champion) {
            if data.champions.get(&champion).is_none_or(|held| held.sha256 != cached.sha256 || held.content != cached.content) {
                data.store_champion(champion, cached);
            }
        }
        bytes
    }

    async fn all_vods(&self, client: &reqwest::Client, now: u64) -> Vec<LibraryVod> {
        self.refresh_index(client, now).await;
        let index = {
            let data = self.data.lock().await;
            data.index.clone().or_else(|| data.bundled("index.json").and_then(parse_index))
        };
        let Some(index) = index else { return vec![] };
        let loads = index.champions.iter().filter(|(_, entry)| entry.vods.unwrap_or(0) > 0).map(|(id, _)| {
            let champion = id.parse::<i64>().expect("validated index id");
            async move { (champion, self.content(client, champion, now).await) }
        }).collect::<Vec<_>>();
        let mut loads = stream::iter(loads).buffered(4);
        let mut vods = vec![];
        while let Some((champion, bytes)) = loads.next().await {
            vods.extend(parse_champion_vods(&bytes, champion).into_iter().map(|vod| LibraryVod { champion, vod }));
        }
        vods
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

fn guide_cache() -> &'static SharedGuideCache {
    static CACHE: OnceLock<SharedGuideCache> = OnceLock::new();
    // Dev builds only: SWAPPER_GUIDES_LOCAL=1 skips the published index and cache.
    let local = cfg!(debug_assertions) && std::env::var_os("SWAPPER_GUIDES_LOCAL").is_some();
    CACHE.get_or_init(|| SharedGuideCache::new(if local { GuideCache::open(None, String::new(), BUNDLED_GUIDES) } else { GuideCache::open(
        crate::vault::data_root().ok().map(|p| p.join("guides-cache")), BASE_URL.into(), BUNDLED_GUIDES) }))
}

fn guide_client() -> Option<reqwest::Client> {
    reqwest::Client::builder().connect_timeout(Duration::from_secs(3)).timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none()).user_agent("Swapper/1.0 (matchup guides)").build().ok()
}

async fn champion_content(champion: i64) -> Option<Vec<u8>> {
    let client = guide_client()?;
    let now = crate::runes::provider::now_ms().max(0) as u64 / 1000;
    Some(guide_cache().content(&client, champion, now).await)
}

async fn guide(champion: i64, enemy: i64, position: Lane) -> (Option<Guide>, GuideLookup, Option<u64>) {
    let bytes = champion_content(champion).await.unwrap_or_default();
    let now = crate::runes::provider::now_ms().max(0) as u64 / 1000;
    let (lookup, expires) = {
        let data = guide_cache().data.lock().await;
        let lookup = data.lookup(champion, enemy, position.clone(), &bytes, now);
        let expires = (lookup == GuideLookup::Absent).then(|| data.refresh.verified_index.as_ref().map(|v| (v.at + REFRESH_SECONDS) * 1000)).flatten();
        (lookup, expires)
    };
    let mut guide = select_guide(parse_champion(&bytes, champion).unwrap_or_default(), enemy, position);
    if let Some(guide) = guide.as_mut() { mark_outdated(guide).await; }
    (guide, lookup, expires)
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
#[serde(rename_all = "camelCase")]
pub(crate) struct MatchupGuideResult { guide: Option<Guide>, record: Option<crate::matchup_records::Record>, lookup: GuideLookup, lookup_expires_at: Option<u64> }

#[tauri::command]
pub(crate) async fn get_matchup_guide(app: tauri::AppHandle, champion_id: i64, enemy_champion_id: i64, position: String) -> Result<MatchupGuideResult, String> {
    if champion_id <= 0 || enemy_champion_id <= 0 || champion_id > i32::MAX as i64 || enemy_champion_id > i32::MAX as i64 { return Err("Unknown champion.".into()); }
    let lane = lane(&position).ok_or("Unknown position.")?;
    let ((guide, lookup, lookup_expires_at), record) = tokio::join!(guide(champion_id, enemy_champion_id, lane),
        tokio::task::spawn_blocking(move || crate::matchup_records::record(&app, champion_id, enemy_champion_id, &position)));
    Ok(MatchupGuideResult { guide, record: record.unwrap_or(None), lookup, lookup_expires_at })
}

#[tauri::command]
pub(crate) async fn get_matchup_vods(champion_id: i64, enemy_champion_id: i64, position: String) -> Result<Vec<MatchupVod>, String> {
    if champion_id <= 0 || enemy_champion_id <= 0 || champion_id > i32::MAX as i64 || enemy_champion_id > i32::MAX as i64 { return Err("Unknown champion.".into()); }
    let lane = lane(&position).ok_or("Unknown position.")?;
    Ok(parse_vods(&champion_content(champion_id).await.unwrap_or_default(), champion_id, enemy_champion_id, lane))
}

#[tauri::command]
pub(crate) async fn get_champion_vods(champion_id: i64) -> Result<Vec<ChampionVod>, String> {
    if champion_id <= 0 || champion_id > i32::MAX as i64 { return Err("Unknown champion.".into()); }
    Ok(parse_champion_vods(&champion_content(champion_id).await.unwrap_or_default(), champion_id))
}

#[derive(Serialize)]
pub(crate) struct LibraryVod { champion: i64, #[serde(flatten)] vod: ChampionVod }

#[tauri::command]
pub(crate) async fn get_all_vods() -> Result<Vec<LibraryVod>, String> {
    let client = guide_client().ok_or("VOD library unavailable.")?;
    let now = crate::runes::provider::now_ms().max(0) as u64 / 1000;
    Ok(guide_cache().all_vods(&client, now).await)
}

#[cfg(test)]
mod tests;
