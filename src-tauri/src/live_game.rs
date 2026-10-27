//! Live Game: every player in the running game with rank, recent form and
//! champion stats, from the League Client (LCU) and the Live Client Data API.
//! No Riot API key and no backend.
//!
//! Nothing here polls. The champion-select watcher reports gameflow phases
//! ([`observe`]); per-player LCU data is fetched once per game when a page
//! first asks ([`live_game_view`]); Live Client numbers are read only when the
//! visible Live Game page asks ([`live_game_live`]), at most every 2.5 s.
pub mod model;

use model::View;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

/// Each LCU request gets this long; the shared client also caps a response at 4 s.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
/// Match history reads in flight at once; the client serves them one by one.
static HISTORY_FETCHES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
/// Live Client reads closer together than this return the cached view.
const LIVE_MIN_INTERVAL: Duration = Duration::from_millis(2500);

#[derive(Default)]
pub struct Runtime {
    in_game: AtomicBool,
    generation: AtomicU64,
    /// The current game's view; `None` outside a game. Cleared at game end.
    view: Mutex<Option<View>>,
    live: Mutex<LiveState>,
    /// A background load of the current game is running (see `observe`).
    prefetching: AtomicBool,
}

#[derive(Default)]
struct LiveState {
    at: Option<Instant>,
    client: Option<Arc<crate::overlay::live_client::LiveClient>>,
}

impl Runtime {
    fn set_active(&self, active: bool) -> bool {
        if self.in_game.swap(active, Ordering::SeqCst) == active { return false; }
        self.generation.fetch_add(1, Ordering::SeqCst);
        if !active {
            *lock(&self.view) = None;
            *lock(&self.live) = LiveState::default();
        }
        true
    }

    fn is_current(&self, generation: u64) -> bool {
        self.in_game.load(Ordering::SeqCst) && self.generation.load(Ordering::SeqCst) == generation
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn init(app: &tauri::AppHandle) {
    app.manage(Runtime::default());
    #[cfg(any(debug_assertions, feature = "live-fixture"))]
    if fixture::bundle().is_some() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            set_active(&app, true);
        });
    }
}

pub fn is_active(app: &tauri::AppHandle) -> bool {
    app.try_state::<Runtime>()
        .is_some_and(|rt| rt.in_game.load(Ordering::Relaxed))
}

/// Called by the champion-select watcher with every polled phase.
pub fn observe(app: &tauri::AppHandle, phase: &str) {
    #[cfg(any(debug_assertions, feature = "live-fixture"))]
    if fixture::bundle().is_some() {
        return;
    }
    let active = model::in_game(phase);
    set_active(app, active);
    // Load the players from the loading screen on, with or without the window open, so the page is
    // ready when opened. Retried on each poll until the session lists the game's players.
    let Some(rt) = app.try_state::<Runtime>() else { return };
    if active && lock(&rt.view).is_none() && !rt.prefetching.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = live_game_view(app.clone()).await;
            app.state::<Runtime>().prefetching.store(false, Ordering::SeqCst);
        });
    }
}

fn set_active(app: &tauri::AppHandle, active: bool) {
    let Some(rt) = app.try_state::<Runtime>() else {
        return;
    };
    if !rt.set_active(active) {
        return;
    }
    let _ = app.emit("live_game_state", active);
    if !active {
        return;
    }
    if auto_open(app) && crate::full_app::enabled(app) {
        // Created minimized and never activated: the game keeps focus.
        let _ = crate::full_app::open_in_background(app, "live");
    }
}

fn auto_open(app: &tauri::AppHandle) -> bool {
    app.state::<crate::AppState>()
        .config
        .lock()
        .map(|c| c.live_game_auto_open)
        .unwrap_or(false)
}

#[tauri::command]
pub fn live_game_active(app: tauri::AppHandle) -> bool {
    is_active(&app)
}

#[tauri::command]
pub fn live_game_auto_open(app: tauri::AppHandle) -> bool {
    auto_open(&app)
}

#[tauri::command]
pub fn set_live_game_auto_open(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let mut saved = config.clone();
    saved.live_game_auto_open = enabled;
    crate::vault::save(&saved)?;
    *config = saved;
    Ok(())
}

async fn champion_names() -> BTreeMap<i64, String> {
    crate::runes::champion_list()
        .await
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect()
}

/// The current game, built from the gameflow session on first call and then
/// filled in by a once-per-game background fetch (`live_game` events carry
/// each update). `None` when no game is running.
#[tauri::command]
pub async fn live_game_view(app: tauri::AppHandle) -> Result<Option<View>, String> {
    let rt = app.state::<Runtime>();
    let generation = rt.generation.load(Ordering::SeqCst);
    if !rt.is_current(generation) { return Ok(None); }
    let session = source::session().await?;
    if !model::in_game(session["phase"].as_str().unwrap_or("")) {
        return Ok(None);
    }
    let game_id = session["gameData"]["gameId"].as_u64().unwrap_or(0);
    if let Some(view) = lock(&rt.view).as_ref().filter(|v| v.game_id == game_id) {
        return Ok(Some(view.clone()));
    }
    let self_puuid = source::self_puuid().await;
    let Some(view) = model::from_session(&session, &self_puuid, &champion_names().await) else {
        return Ok(None);
    };
    {
        let mut current = lock(&rt.view);
        if !rt.is_current(generation) { return Ok(None); }
        // A concurrent call may have built it while this one awaited.
        if let Some(existing) = current.as_ref().filter(|v| v.game_id == game_id) {
            return Ok(Some(existing.clone()));
        }
        *current = Some(view.clone());
    }
    let task_app = app.clone();
    tauri::async_runtime::spawn(async move { fetch_players(task_app, game_id).await });
    Ok(Some(view))
}

/// One piece of a player, published as soon as it lands.
enum Part {
    Damage(Option<String>),
    Profile(Option<Value>, Option<Value>),
    /// u.gg's recent games: here in about a second, but may miss the latest game.
    Quick(Vec<Value>),
    /// The client's own history: exact, but served one player at a time.
    Rows(Vec<Value>),
}

/// Fetches every player once (all in flight) and publishes each part as it lands.
async fn fetch_players(app: tauri::AppHandle, game_id: u64) {
    let targets: Vec<(String, String, i64, String)> = {
        let rt = app.state::<Runtime>();
        let view = lock(&rt.view);
        let Some(view) = view.as_ref().filter(|v| v.game_id == game_id) else {
            return;
        };
        view.players
            .iter()
            .map(|p| (p.key.clone(), p.puuid.clone(), p.champion_id, p.riot_id.clone()))
            .collect()
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, Part)>();
    for (key, puuid, champion, riot_id) in targets {
        let tx = tx.clone();
        tauri::async_runtime::spawn(async move {
            let send = |part| { let _ = tx.send((key.clone(), part)); };
            if puuid.is_empty() {
                send(Part::Damage(source::damage(champion).await));
                return;
            }
            let (damage, (summoner, ranked)) = tokio::join!(source::damage(champion), source::profile(&puuid));
            let riot_id = summoner.as_ref().and_then(riot_id_of).unwrap_or(riot_id);
            send(Part::Damage(damage));
            send(Part::Profile(summoner, ranked));
            tokio::join!(
                async { if let Some(rows) = source::quick_history(&riot_id).await { send(Part::Quick(rows)) } },
                async { if let Some(rows) = source::history(&puuid).await { send(Part::Rows(rows)) } },
            );
        });
    }
    drop(tx);
    while let Some((key, part)) = rx.recv().await {
        let rt = app.state::<Runtime>();
        let snapshot = {
            let mut view = lock(&rt.view);
            let Some(view) = view.as_mut().filter(|v| v.game_id == game_id) else {
                return; // The game ended or changed; drop the rest.
            };
            if let Some(player) = view.players.iter_mut().find(|p| p.key == key) {
                match part {
                    Part::Damage(damage) => player.damage = damage.unwrap_or_default(),
                    Part::Profile(summoner, ranked) => player.apply(summoner.as_ref(), ranked.as_ref(), None),
                    Part::Quick(rows) => player.apply_quick(&rows),
                    Part::Rows(rows) => player.apply(None, None, Some(&rows)),
                }
            }
            view.summarize_teams();
            view.clone()
        };
        let _ = app.emit("live_game", snapshot);
    }
}

fn riot_id_of(summoner: &Value) -> Option<String> {
    let name = summoner["gameName"].as_str().filter(|s| !s.is_empty())?;
    Some(format!("{name}#{}", summoner["tagLine"].as_str().unwrap_or("")))
}

/// Merges a fresh Live Client read into the current view. Called by the Live
/// Game page every few seconds while it is visible; never by a timer here.
#[tauri::command]
pub async fn live_game_live(app: tauri::AppHandle) -> Result<Option<View>, String> {
    let rt = app.state::<Runtime>();
    let generation = rt.generation.load(Ordering::SeqCst);
    if !rt.is_current(generation) { return Ok(None); }
    if lock(&rt.view).is_none() {
        // Opened during the loading screen, before the session had a game.
        return live_game_view(app.clone()).await;
    }
    let client = {
        let mut live = lock(&rt.live);
        if live.at.is_some_and(|at| at.elapsed() < LIVE_MIN_INTERVAL) {
            return Ok(lock(&rt.view).clone());
        }
        live.at = Some(Instant::now());
        if live.client.is_none() {
            live.client = Some(Arc::new(crate::overlay::live_client::LiveClient::new()?));
        }
        live.client.clone().unwrap()
    };
    let data = source::live(&client).await?;
    let ids: BTreeMap<String, i64> = champion_names()
        .await
        .into_iter()
        .map(|(id, name)| (name.to_lowercase(), id))
        .collect();
    let mut view = lock(&rt.view);
    if !rt.is_current(generation) { return Ok(None); }
    if let Some(view) = view.as_mut() {
        model::merge_live(view, &data, &ids);
    }
    Ok(view.clone())
}

/// Champion damage types never change within a patch; one read per champion.
static DAMAGE: Mutex<Option<HashMap<i64, String>>> = Mutex::new(None);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losing_client_clears_live_state_and_invalidates_pending_reads() {
        let rt = Runtime::default();
        assert!(rt.set_active(model::in_game("InProgress")));
        let generation = rt.generation.load(Ordering::SeqCst);
        let session = serde_json::json!({"phase": "InProgress", "gameData": {"gameId": 42,
            "teamOne": [{"championId": 103, "puuid": "fixture-a"}], "teamTwo": []}});
        *lock(&rt.view) = model::from_session(&session, "fixture-a", &BTreeMap::new());
        assert!(lock(&rt.view).is_some());
        lock(&rt.live).at = Some(Instant::now());
        assert!(rt.set_active(model::in_game("Offline")));
        assert!(lock(&rt.view).is_none());
        assert!(lock(&rt.live).at.is_none());
        assert!(!rt.is_current(generation));
        assert!(!rt.set_active(false));
        assert!(rt.set_active(true));
        assert!(!rt.is_current(generation), "old reads cannot repopulate after reconnect");
    }
}

// Shared with the player-search live rosters.
pub(crate) use source::{history, profile, quick_history};

mod source {
    use super::*;

    async fn get(path: &str) -> Result<Value, String> {
        let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
        tokio::time::timeout(
            REQUEST_TIMEOUT,
            crate::runes::lcu_get::<Value>(&client, path),
        )
        .await
        .map_err(|_| "League Client timed out.".to_string())?
        .map_err(|e| e.to_string())
    }

    pub async fn session() -> Result<Value, String> {
        #[cfg(any(debug_assertions, feature = "live-fixture"))]
        if let Some(bundle) = fixture::bundle() {
            return Ok(bundle["session"].clone());
        }
        get("/lol-gameflow/v1/session").await
    }

    pub async fn self_puuid() -> String {
        #[cfg(any(debug_assertions, feature = "live-fixture"))]
        if let Some(bundle) = fixture::bundle() {
            return bundle["selfPuuid"].as_str().unwrap_or("").to_string();
        }
        get("/lol-summoner/v1/current-summoner")
            .await
            .ok()
            .and_then(|v| v["puuid"].as_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// Summoner (Riot ID, level, icon) and ranked stats; each fails independently.
    pub async fn profile(puuid: &str) -> (Option<Value>, Option<Value>) {
        #[cfg(any(debug_assertions, feature = "live-fixture"))]
        if let Some(bundle) = fixture::bundle() {
            tokio::time::sleep(Duration::from_millis(350)).await;
            let p = &bundle["players"][puuid];
            return (
                Some(p["summoner"].clone()).filter(|v| !v.is_null()),
                Some(p["ranked"].clone()).filter(|v| !v.is_null()),
            );
        }
        tokio::join!(
            async { get(&format!("/lol-summoner/v2/summoners/puuid/{puuid}")).await.ok() },
            async { get(&format!("/lol-ranked/v1/ranked-stats/{puuid}")).await.ok() },
        )
    }

    /// The latest 20 games from the client's own match history.
    pub async fn history(puuid: &str) -> Option<Vec<Value>> {
        #[cfg(any(debug_assertions, feature = "live-fixture"))]
        if let Some(bundle) = fixture::bundle() {
            return bundle["players"][puuid]["history"]["games"]["games"].as_array().cloned();
        }
        let path =
            format!("/lol-match-history/v1/products/lol/{puuid}/matches?begIndex=0&endIndex=19");
        // The client fetches other players' histories remotely one at a time
        // (measured ~1.6 s each), so ten at once queue past the 4 s cap.
        // Asking one at a time keeps each inside it; a retry is served from its cache.
        let _turn = HISTORY_FETCHES.acquire().await.ok()?;
        let history = match get(&path).await {
            Ok(value) => value,
            Err(_) => get(&path).await.ok()?,
        };
        history["games"]["games"].as_array().cloned()
    }

    /// The latest 20 games from u.gg's match summaries: all ten players in
    /// about a second, though u.gg may not have seen the newest game yet.
    /// `None` when u.gg does not know the player or cannot be reached.
    pub async fn quick_history(riot_id: &str) -> Option<Vec<Value>> {
        #[cfg(any(debug_assertions, feature = "live-fixture"))]
        if fixture::bundle().is_some() {
            return None;
        }
        let (name, tag) = riot_id.split_once('#').filter(|(n, t)| !n.is_empty() && !t.is_empty())?;
        let locale = get("/riotclient/region-locale").await.ok()?;
        let region_id = crate::runes::ugg_record::region_id(locale["region"].as_str()?)?;
        let body = serde_json::json!({ "query": UGG_HISTORY, "variables": {
            "page": 1, "queueType": [], "regionId": region_id, "riotUserName": name, "riotTagLine": tag,
            "seasonIds": [crate::runes::ugg_record::SEASON_ID] } });
        let client = UGG_CLIENT.get_or_init(|| {
            reqwest::Client::builder().connect_timeout(REQUEST_TIMEOUT).timeout(REQUEST_TIMEOUT).build().ok()
        }).as_ref()?;
        let response = client.post(crate::runes::probuilds::ENDPOINT).json(&body).send().await.ok()?;
        let value: Value = response.error_for_status().ok()?.json().await.ok()?;
        value.pointer("/data/fetchPlayerMatchSummaries/matchSummaries")?.as_array().cloned()
    }

    static UGG_CLIENT: std::sync::OnceLock<Option<reqwest::Client>> = std::sync::OnceLock::new();
    const UGG_HISTORY: &str = "\
query FetchMatchSummaries($page: Int, $queueType: [Int], $regionId: String!, $riotUserName: String!, $riotTagLine: String!, $seasonIds: [Int]!) {
  fetchPlayerMatchSummaries(page: $page, queueType: $queueType, regionId: $regionId, riotUserName: $riotUserName, riotTagLine: $riotTagLine, seasonIds: $seasonIds) {
    matchSummaries { matchId matchCreationTime matchDuration queueType role championId win kills deaths assists cs jungleCs }
  }
}";

    pub async fn damage(champion: i64) -> Option<String> {
        if champion <= 0 {
            return None;
        }
        if let Some(known) = lock(&DAMAGE)
            .as_ref()
            .and_then(|m| m.get(&champion).cloned())
        {
            return Some(known);
        }
        #[cfg(any(debug_assertions, feature = "live-fixture"))]
        if let Some(bundle) = fixture::bundle() {
            return bundle["damage"][champion.to_string()]
                .as_str()
                .map(str::to_string);
        }
        let value = get(&format!(
            "/lol-game-data/assets/v1/champions/{champion}.json"
        ))
        .await
        .ok()?;
        let kind = value["tacticalInfo"]["damageType"].as_str()?.to_string();
        lock(&DAMAGE)
            .get_or_insert_with(HashMap::new)
            .insert(champion, kind.clone());
        Some(kind)
    }

    pub async fn live(client: &crate::overlay::live_client::LiveClient) -> Result<Value, String> {
        #[cfg(any(debug_assertions, feature = "live-fixture"))]
        if let Some(bundle) = fixture::bundle() {
            let _ = client;
            return Ok(bundle["allgamedata"].clone());
        }
        client.fetch().await
    }
}

/// Debug builds (or `--features live-fixture`) render Live Game from a JSON
/// bundle named by `SWAPPER_LIVE_FIXTURE`, so the page can be built, tested
/// and measured without a running game. Release builds never read it.
#[cfg(any(debug_assertions, feature = "live-fixture"))]
mod fixture {
    use serde_json::Value;
    use std::sync::OnceLock;

    pub fn bundle() -> Option<&'static Value> {
        static BUNDLE: OnceLock<Option<Value>> = OnceLock::new();
        BUNDLE
            .get_or_init(|| {
                let path = std::env::var_os("SWAPPER_LIVE_FIXTURE")?;
                serde_json::from_slice(&std::fs::read(path).ok()?).ok()
            })
            .as_ref()
    }
}
