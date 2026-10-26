//! Optional Fullsized App. When on, it replaces the tray flyout: the flyout
//! webview is never created and every entry point opens this window instead.
//! No window or data task exists until it is opened; closing hides it (low
//! memory mode destroys it instead).
pub mod abilities;
pub mod art;
pub mod champ_select;
pub(crate) mod bench;
pub(crate) mod cards;
pub(crate) mod lp;
pub(crate) mod lp_seed;
pub(crate) mod mmr;
pub mod ui_data;
pub(crate) mod model;
pub mod routing;
pub mod search;

use base64::Engine;
use futures_util::{stream, StreamExt};
use model::Snapshot;
use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

pub const LABEL: &str = "full_app";
pub const FLYOUT: &str = "main";

pub struct Runtime {
    menu: tauri::menu::Menu<tauri::Wry>,
    cancel: tokio::sync::watch::Sender<u64>,
    gate: tokio::sync::Mutex<()>,
    /// Destination each window shows once its page reports ready. `Some`
    /// also means the window was created and has not reported ready yet.
    pending_flyout: Mutex<Option<String>>,
    pending_full: Mutex<Option<String>>,
    /// The full app being created should appear minimized without taking
    /// focus (Live Game auto-open while a game has the foreground).
    background: std::sync::atomic::AtomicBool,
}

impl Runtime {
    pub fn new(menu: tauri::menu::Menu<tauri::Wry>) -> Self {
        Self {
            menu,
            cancel: tokio::sync::watch::channel(0).0,
            gate: tokio::sync::Mutex::new(()),
            pending_flyout: Mutex::new(None),
            pending_full: Mutex::new(None),
            background: Default::default(),
        }
    }
}

pub fn cancel(app: &tauri::AppHandle) {
    app.state::<Runtime>()
        .cancel
        .send_modify(|epoch| *epoch += 1);
}

fn settings(app: &tauri::AppHandle) -> routing::WindowSettings {
    app.state::<crate::AppState>()
        .config
        .lock()
        .map(|c| routing::WindowSettings::from_config(&c))
        .unwrap_or_default()
}

pub fn enabled(app: &tauri::AppHandle) -> bool {
    settings(app).full_app
}

pub fn low_memory(app: &tauri::AppHandle) -> bool {
    settings(app).low_memory
}

#[tauri::command]
pub fn window_settings(app: tauri::AppHandle) -> routing::WindowSettings {
    settings(&app)
}

fn save_setting(
    app: &tauri::AppHandle,
    change: impl Fn(&mut crate::vault::Config),
) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let previous = config.clone();
    change(&mut config);
    if let Err(error) = crate::vault::save(&config) {
        *config = previous;
        return Err(error);
    }
    Ok(())
}

fn clear(pending: &Mutex<Option<String>>) {
    if let Ok(mut pending) = pending.lock() {
        *pending = None;
    }
}

/// Async so window creation and destruction never run inside a synchronous
/// IPC handler (WebView2 deadlocks there).
#[tauri::command]
pub async fn set_full_app_enabled(
    app: tauri::AppHandle,
    enabled: bool,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if !routing::window_may_invoke(window.label()) {
        return Err("Change this setting from Swapper's Settings.".into());
    }
    save_setting(&app, |c| c.full_app = enabled)?;
    sync_menu(&app)?;
    let _ = app.emit("window_settings", settings(&app));
    let runtime = app.state::<Runtime>();
    if enabled {
        if let Some(flyout) = app.get_webview_window(FLYOUT) {
            flyout.destroy().map_err(|e| e.to_string())?;
        }
        clear(&runtime.pending_flyout);
        open(&app, None)
    } else {
        cancel(&app);
        if let Some(full) = app.get_webview_window(LABEL) {
            full.destroy().map_err(|e| e.to_string())?;
        }
        clear(&runtime.pending_full);
        crate::show_flyout(&app, "settings");
        Ok(())
    }
}

#[tauri::command]
pub fn set_low_memory_mode(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    save_setting(&app, |c| c.low_memory_mode = Some(enabled))?;
    let _ = app.emit("window_settings", settings(&app));
    Ok(())
}

pub fn sync_menu(app: &tauri::AppHandle) -> Result<(), String> {
    let runtime = app.state::<Runtime>();
    let menu = &runtime.menu;
    if let Some(item) = menu.get(LABEL) {
        menu.remove(&item).map_err(|e| e.to_string())?;
    }
    if enabled(app) {
        let item = tauri::menu::MenuItem::with_id(app, LABEL, "Open Swapper", true, None::<&str>)
            .map_err(|e| e.to_string())?;
        menu.insert(&item, 0).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Opens or focuses the Fullsized App. `destination` is a flyout view name
/// ("accounts", "add", "settings", ...) the app navigates to.
pub fn open(app: &tauri::AppHandle, destination: Option<&str>) -> Result<(), String> {
    open_window(app, destination, false)
}

/// Creates the Fullsized App minimized and never activated, so it waits in the
/// taskbar and Alt+Tab while a game keeps the foreground. An existing window
/// is left alone.
pub fn open_in_background(app: &tauri::AppHandle, destination: &str) -> Result<(), String> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    open_window(app, Some(destination), true)
}

fn open_window(
    app: &tauri::AppHandle,
    destination: Option<&str>,
    background: bool,
) -> Result<(), String> {
    if !enabled(app) {
        return Err("Turn on Fullsized App in Settings first.".into());
    }
    let runtime = app.state::<Runtime>();
    // A user's open while a background window is still loading shows it normally.
    runtime
        .background
        .store(background, std::sync::atomic::Ordering::Relaxed);
    if let Some(window) = app.get_webview_window(LABEL) {
        {
            let mut pending = runtime.pending_full.lock().map_err(|e| e.to_string())?;
            if pending.is_some() {
                // Still loading; `window_ready` shows it at the latest destination.
                if let Some(destination) = destination {
                    *pending = Some(destination.to_string());
                }
                return Ok(());
            }
        }
        if let Some(destination) = destination {
            let _ = app.emit_to(LABEL, "navigate", destination);
        }
        window.unminimize().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        crate::set_flyout_memory(&window, false);
        return window.set_focus().map_err(|e| e.to_string());
    }
    *runtime.pending_full.lock().map_err(|e| e.to_string())? =
        Some(destination.unwrap_or_default().to_string());
    // Shares the flyout's WebView2 environment (default data directory), which
    // requires identical browser arguments; they come from the flyout config.
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        LABEL,
        tauri::WebviewUrl::App("full-app.html".into()),
    )
    .title("Swapper")
    .inner_size(1280.0, 820.0)
    .min_inner_size(1100.0, 700.0)
    .resizable(true)
    // Swapper draws its own title bar (drag region, history, search, window
    // buttons); Windows keeps the resize border and shadow.
    .decorations(false)
    .shadow(true)
    .center()
    .focused(!background)
    // Shown by `window_ready` once painted: no blank frame, and no close
    // request lands while WebView2 is still initializing (it is ignored then).
    .visible(false);
    if let Some(args) = flyout_config(app).and_then(|c| c.additional_browser_args) {
        builder = builder.additional_browser_args(&args);
    }
    if let Err(error) = builder.build() {
        clear(&runtime.pending_full);
        return Err(error.to_string());
    }
    Ok(())
}

pub fn flyout_config(app: &tauri::AppHandle) -> Option<tauri::utils::config::WindowConfig> {
    app.config()
        .app
        .windows
        .iter()
        .find(|w| w.label == FLYOUT)
        .cloned()
}

/// Records that a flyout was just created and is waiting for its page; it is
/// shown at `destination` (if any) once the page reports ready.
pub fn expect_flyout(app: &tauri::AppHandle, destination: Option<&str>) {
    if let Ok(mut pending) = app.state::<Runtime>().pending_flyout.lock() {
        *pending = Some(destination.unwrap_or_default().to_string());
    }
}

/// While a created flyout has not reported ready, a show request only
/// updates where it lands. Returns whether the request was deferred.
pub fn defer_flyout(app: &tauri::AppHandle, destination: &str) -> bool {
    let runtime = app.state::<Runtime>();
    let Ok(mut pending) = runtime.pending_flyout.lock() else {
        return false;
    };
    if pending.is_none() {
        return false;
    }
    *pending = Some(destination.to_string());
    true
}

#[tauri::command]
pub async fn open_full_app(app: tauri::AppHandle) -> Result<(), String> {
    open(&app, None)
}

/// Called by a page once its event listeners exist, so a destination sent now
/// is never lost. A newly created flyout is shown only at this point.
#[tauri::command]
pub async fn window_ready(app: tauri::AppHandle, window: tauri::WebviewWindow) {
    let runtime = app.state::<Runtime>();
    if window.label() == FLYOUT {
        let destination = runtime.pending_flyout.lock().ok().and_then(|mut p| p.take());
        if let Some(destination) = destination.filter(|d| !d.is_empty()) {
            crate::show_flyout(&app, &destination);
        }
    } else if window.label() == LABEL {
        let destination = runtime.pending_full.lock().ok().and_then(|mut p| p.take());
        if let Some(destination) = destination.filter(|d| !d.is_empty()) {
            let _ = app.emit_to(LABEL, "navigate", destination);
        }
        if runtime
            .background
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            show_minimized_without_focus(&window);
        } else {
            // A new window opens maximized; after that it keeps whatever size the user chose.
            let _ = window.maximize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        // Opening during champion select lands on the champion select page.
        let status = crate::runes::current_status().await;
        let _ = app.emit_to(LABEL, "champ_select", status);
    }
}

/// `SW_SHOWMINNOACTIVE`: in the taskbar and Alt+Tab, not on screen, and the
/// foreground window (the game) keeps focus. Never topmost.
fn show_minimized_without_focus(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWMINNOACTIVE};
    if let Ok(hwnd) = window.hwnd() {
        unsafe { ShowWindow(hwnd.0 as _, SW_SHOWMINNOACTIVE) };
    }
    crate::set_flyout_memory(window, true);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    snapshot: Option<Snapshot>,
    /// Rank and recent record per saved account id.
    cards: cards::Cards,
    stale: bool,
    message: Option<String>,
}

fn cache_path() -> Option<std::path::PathBuf> {
    Some(
        crate::vault::data_root()
            .ok()?
            .join("dashboard-snapshot.json"),
    )
}

pub(crate) fn read_cache() -> Option<Snapshot> {
    let path = cache_path()?;
    read_cache_at(&path)
}

fn read_cache_at(path: &std::path::Path) -> Option<Snapshot> {
    if std::fs::metadata(path).ok()?.len() > 512 * 1024 {
        return None;
    }
    let snapshot: Snapshot = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (snapshot.games.len() <= 20 && !snapshot.summoner.puuid.is_empty()).then_some(snapshot)
}

/// Keeps a saved account's avatar in step with its League profile icon. Icons
/// were otherwise only saved when an account was identified with League open,
/// so accounts added or switched to without it showed a letter instead.
fn save_account_icon(app: &tauri::AppHandle, puuid: &str, icon: &str) {
    use tauri::Manager;
    let Some(state) = app.try_state::<crate::AppState>() else { return };
    let Some(id) = state.config.lock().ok().and_then(|config| {
        config.accounts.iter().find(|a| a.puuid.as_deref() == Some(puuid)).map(|a| a.id)
    }) else {
        return;
    };
    // Presentation only: a failed write keeps the letter avatar.
    if crate::vault::profile_icon_data_url(id).as_deref() != Some(icon) && crate::vault::save_profile_icon(id, icon).is_ok() {
        use tauri::Emitter;
        let _ = app.emit("accounts_changed", ());
    }
}

/// How long, and how often, to wait for League to file a finished game in match history.
const FILED_CHECKS: u32 = 36;
const FILED_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Waits until the game that just ended shows up in match history (League takes
/// up to a few minutes), then emits `game_finished` with its id so the app can
/// refresh the dashboard and open the match.
pub async fn announce_finished_game(app: tauri::AppHandle) {
    let Some((id, queue)) = finished_game().await else { return };
    // The end-of-game screen's stats exist right away, minutes before match history files the game:
    // the dashboard shows them as a normal row with match details meanwhile. They can take a few seconds to appear.
    for _ in 0..ENDED_CHECKS {
        let eog = match crate::runes::lcu().await {
            Ok(client) => crate::runes::lcu_get::<Value>(&client, "/lol-end-of-game/v1/eog-stats-block").await.ok(),
            Err(_) => None,
        };
        if let Some(eog) = eog.filter(|eog| eog["gameId"].as_u64() == Some(id)) {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
            if let Some((history, owner)) = eog_as_history(&eog, queue, now) {
                if let Some(game) = model::game(&history, &owner) {
                    crate::match_detail::remember_ended(&history, &owner);
                    let _ = app.emit("game_ended", game);
                }
            }
            break;
        }
        tokio::time::sleep(ENDED_INTERVAL).await;
    }
    for _ in 0..FILED_CHECKS {
        tokio::time::sleep(FILED_INTERVAL).await;
        let Ok(client) = crate::runes::lcu().await else { continue };
        let Ok(history) = crate::runes::lcu_get::<Value>(&client, "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex=4").await else {
            continue;
        };
        if history_has_game(&history, id) {
            let _ = app.emit("game_finished", id);
            return;
        }
    }
}

const ENDED_CHECKS: u32 = 6;
const ENDED_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// End-of-game stat names whose match-history name is not just their camelCase form.
const EOG_STATS: [(&str, &str); 6] = [
    ("kills", "CHAMPIONS_KILLED"),
    ("deaths", "NUM_DEATHS"),
    ("totalMinionsKilled", "MINIONS_KILLED"),
    ("champLevel", "LEVEL"),
    ("wardsPlaced", "WARD_PLACED"),
    ("wardsKilled", "WARD_KILLED"),
];

/// `TOTAL_DAMAGE_TAKEN` to `totalDamageTaken`.
fn camel(key: &str) -> String {
    let mut out = String::new();
    for (i, word) in key.split('_').filter(|w| !w.is_empty()).enumerate() {
        let lower = word.to_ascii_lowercase();
        if i == 0 {
            out.push_str(&lower);
        } else {
            out.push_str(&lower[..1].to_ascii_uppercase());
            out.push_str(&lower[1..]);
        }
    }
    out
}

/// The end-of-game stats in the match-history game shape, plus the local player,
/// so the just-ended game gets the same row and match details as filed games.
fn eog_as_history(eog: &Value, queue: Option<u64>, now_ms: u64) -> Option<(Value, model::Summoner)> {
    let local = &eog["localPlayer"];
    let duration = eog["gameLength"].as_u64()?;
    let queue = queue.or_else(|| eog["queueId"].as_u64()).or_else(|| match eog["queueType"].as_str()? {
        "RANKED_SOLO_5x5" => Some(420),
        "RANKED_FLEX_SR" => Some(440),
        _ => None,
    })?;
    let teams = eog["teams"].as_array()?;
    let mut players: Vec<(&Value, &Value)> = teams.iter()
        .flat_map(|team| team["players"].as_array().into_iter().flatten().map(move |p| (team, p)))
        .collect();
    if !players.iter().any(|(_, p)| p["puuid"] == local["puuid"]) {
        let team = teams.iter().find(|t| t["teamId"] == local["teamId"])?;
        players.push((team, local));
    }
    let (mut participants, mut identities) = (Vec::new(), Vec::new());
    for (index, (team, p)) in players.into_iter().enumerate() {
        let raw = &p["stats"];
        let mut stats: serde_json::Map<String, Value> = raw.as_object().into_iter().flatten()
            .map(|(key, value)| (camel(key), value.clone())).collect();
        for (history, eog_key) in EOG_STATS {
            if let Some(value) = raw.get(eog_key) {
                stats.insert(history.into(), value.clone());
            }
        }
        for (i, item) in p["items"].as_array().into_iter().flatten().take(7).enumerate() {
            stats.insert(format!("item{i}"), item.clone());
        }
        let win = team["isWinningTeam"].as_bool().or_else(|| raw["WIN"].as_u64().map(|win| win == 1))?;
        stats.insert("win".into(), win.into());
        stats.insert("gameEndedInEarlySurrender".into(), eog["gameEndedInEarlySurrender"].as_bool().unwrap_or(false).into());
        let position = p["detectedTeamPosition"].as_str().filter(|s| !s.is_empty()).or_else(|| p["selectedPosition"].as_str());
        let (lane, role) = match position {
            Some("TOP") => ("TOP", "SOLO"),
            Some("JUNGLE") => ("JUNGLE", "NONE"),
            Some("MIDDLE") => ("MIDDLE", "SOLO"),
            Some("BOTTOM") => ("BOTTOM", "DUO_CARRY"),
            Some("UTILITY") => ("BOTTOM", "DUO_SUPPORT"),
            _ => ("NONE", "NONE"),
        };
        participants.push(serde_json::json!({
            "participantId": index + 1, "teamId": p["teamId"].as_u64().or_else(|| team["teamId"].as_u64()),
            "championId": p["championId"], "spell1Id": p["spell1Id"], "spell2Id": p["spell2Id"],
            "stats": stats, "timeline": { "lane": lane, "role": role },
        }));
        identities.push(serde_json::json!({ "participantId": index + 1, "player": {
            "puuid": p["puuid"], "summonerId": p["summonerId"], "gameName": p["riotIdGameName"],
            "tagLine": p["riotIdTagLine"], "summonerName": p["summonerName"],
        } }));
    }
    let history = serde_json::json!({
        "gameId": eog["gameId"].as_u64().filter(|id| *id > 0)?, "gameDuration": duration,
        "gameCreation": now_ms.saturating_sub(duration * 1000), "mapId": 11, "queueId": queue,
        "participants": participants, "participantIdentities": identities,
        "teams": teams.iter().map(|t| serde_json::json!({ "teamId": t["teamId"], "win": t["isWinningTeam"] })).collect::<Vec<_>>(),
    });
    let name = local["riotIdGameName"].as_str().unwrap_or_default();
    let owner = model::Summoner {
        puuid: local["puuid"].as_str().filter(|id| !id.is_empty())?.into(),
        summoner_id: local["summonerId"].as_u64().unwrap_or(0),
        riot_id: match local["riotIdTagLine"].as_str().filter(|tag| !tag.is_empty()) {
            Some(tag) => format!("{name}#{tag}"),
            None => name.into(),
        },
        level: 0,
        icon: None,
    };
    Some((history, owner))
}

/// The id and queue of the game that just ended: the gameflow session still
/// holds them at end of game; the end-of-game stats are the fallback for the id.
async fn finished_game() -> Option<(u64, Option<u64>)> {
    let client = crate::runes::lcu().await.ok()?;
    let id = |value: &Value, pointer: &str| value.pointer(pointer).and_then(Value::as_u64).filter(|id| *id > 0);
    if let Ok(session) = crate::runes::lcu_get::<Value>(&client, "/lol-gameflow/v1/session").await {
        if let Some(found) = id(&session, "/gameData/gameId") {
            return Some((found, id(&session, "/gameData/queue/id")));
        }
    }
    let stats = crate::runes::lcu_get::<Value>(&client, "/lol-end-of-game/v1/eog-stats-block").await.ok()?;
    id(&stats, "/gameId").map(|found| (found, None))
}

fn history_has_game(history: &Value, id: u64) -> bool {
    history["games"]["games"].as_array().is_some_and(|games| games.iter().any(|g| g["gameId"].as_u64() == Some(id)))
}

async fn fresh(app: &tauri::AppHandle, previous: Option<&Snapshot>) -> Result<Snapshot, String> {
    let account = lp::AccountRead::begin(app)?;
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let current: Value = crate::runes::lcu_get(&client, "/lol-summoner/v1/current-summoner")
        .await
        .map_err(|e| e.to_string())?;
    let mut summoner =
        model::summoner(&current).ok_or("League Client has no signed-in summoner.")?;
    let (history, ranked) = tokio::try_join!(
        crate::runes::lcu_get::<Value>(
            &client,
            "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex=19"
        ),
        crate::runes::lcu_get::<Value>(&client, "/lol-ranked/v1/current-ranked-stats")
    )
    .map_err(|e| e.to_string())?;
    let rows = history["games"]["games"]
        .as_array()
        .ok_or("League Client sent an unexpected match history.")?;
    let cached = previous.filter(|s| s.summoner.puuid == summoner.puuid);
    let eligible: Vec<Value> = rows
        .iter()
        .take(20)
        .filter(|g| model::eligible(g))
        .cloned()
        .collect();
    let mut games = stream::iter(eligible.into_iter().map(|row| {
        let client = &client;
        let summoner = &summoner;
        async move {
            let id = row["gameId"].as_u64()?;
            if let Some(game) = cached
                .and_then(|s| s.games.iter().find(|g| g.id == id))
                .filter(|g| {
                    g.metrics[1].is_some()
                        && g.metrics[4].is_some()
                        && !g.items.is_empty()
                        && !g.players.is_empty()
                        && g.players.iter().any(|p| !p.puuid.is_empty())
                })
            {
                return Some(game.clone());
            }
            let detail = crate::runes::lcu_get::<Value>(
                client,
                &format!("/lol-match-history/v1/games/{id}"),
            )
            .await
            .ok();
            model::game(detail.as_ref().unwrap_or(&row), summoner)
        }
    }))
    .buffer_unordered(3)
    .filter_map(|g| async move { g })
    .collect::<Vec<_>>()
    .await;
    games.sort_by_key(|g| std::cmp::Reverse(g.created_at));
    let rank = model::rank(&ranked);
    if let Some(id) = current["profileIconId"].as_u64() {
        if let Ok(bytes) = crate::runes::lcu_get_bytes(
            &client,
            &format!("/lol-game-data/assets/v1/profile-icons/{id}.jpg"),
        )
        .await
        {
            summoner.icon = Some(format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ));
        }
    }
    if let Some(icon) = summoner.icon.as_deref() {
        save_account_icon(app, &summoner.puuid, icon);
    }
    let champion_names = crate::runes::champion_list()
        .await
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    // Verify both identity and switch generation before LP can be persisted.
    let points = lp::record_current(&client, account, summoner.puuid.clone(), rank.clone()).await?;
    lp::annotate(&mut games, &points);
    lp_seed::spawn(summoner.puuid.clone(), summoner.riot_id.clone(), &points);
    let mmr = mmr::current(&games).await;
    Ok(Snapshot {
        lp_history: points,
        mmr,
        champion_names,
        summoner,
        rank,
        fetched_at: crate::runes::provider::now_ms(),
        summaries: model::summaries(&games),
        games,
    })
}

#[tauri::command]
pub async fn dashboard_snapshot(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<View, String> {
    if window.label() != LABEL || !enabled(&app) {
        return Err("Fullsized App is closed or disabled.".into());
    }
    let runtime = app.state::<Runtime>();
    let mut cancelled = runtime.cancel.subscribe();
    tokio::select! {
        _ = cancelled.changed() => Err("Dashboard closed.".into()),
        result = async {
            let _guard = runtime.gate.lock().await;
            let previous = tokio::task::spawn_blocking(read_cache).await.unwrap_or(None);
            match fresh(&app, previous.as_ref()).await {
                Ok(snapshot) => {
                    crate::runes::blind::preload(&app, &snapshot);
                    let bytes = serde_json::to_vec(&snapshot).ok();
                    let saved = tokio::task::spawn_blocking(move || {
                        if let (Some(path), Some(bytes)) = (cache_path(), bytes) {
                            if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
                            let pending = path.with_extension("tmp");
                            if std::fs::write(&pending, bytes).is_ok() { let _ = std::fs::rename(pending, path); }
                        }
                        let cards = cards::update(Some(&snapshot));
                        (cards, snapshot)
                    }).await;
                    let (cards, snapshot) = saved.map_err(|e| e.to_string())?;
                    Ok(View { snapshot: Some(snapshot), cards, stale: false, message: None })
                },
                Err(error) => {
                    let cards = tokio::task::spawn_blocking(|| cards::update(None)).await.unwrap_or_default();
                    Ok(View { snapshot: previous, cards, stale: true, message: Some(error) })
                },
            }
        } => result
    }
}

#[cfg(test)]
mod tests {
    use super::{eog_as_history, history_has_game, model};

    #[test]
    fn end_of_game_stats_become_a_full_match() {
        let player = |n: u64, team: u64, position: &str, kills: u64| serde_json::json!({
            "puuid": format!("p{n}"), "summonerId": n, "riotIdGameName": format!("Player{n}"), "riotIdTagLine": "EUW",
            "championId": 100 + n, "teamId": team, "detectedTeamPosition": position,
            "spell1Id": 4, "spell2Id": if position == "JUNGLE" { 11 } else { 14 },
            "items": [3089, 0, 3020, 0, 0, 0, 3364],
            "stats": { "CHAMPIONS_KILLED": kills, "NUM_DEATHS": 2, "ASSISTS": 9, "MINIONS_KILLED": 180, "NEUTRAL_MINIONS_KILLED": 12,
                "LEVEL": 16, "PERK0": 8112, "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS": 20000 + kills, "GOLD_EARNED": 12000,
                "VISION_SCORE": 30, "TOTAL_DAMAGE_TAKEN": 15000, "WARD_PLACED": 10, "WARD_KILLED": 3 }
        });
        let positions = ["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY"];
        let team = |id: u64, win: bool| serde_json::json!({ "teamId": id, "isWinningTeam": win, "players":
            positions.iter().enumerate().map(|(i, pos)| player(id / 100 * 10 + i as u64, id, pos, i as u64)).collect::<Vec<_>>() });
        let eog = serde_json::json!({
            "gameId": 42, "gameLength": 1830, "localPlayer": player(12, 100, "MIDDLE", 2),
            "teams": [team(100, false), team(200, true)]
        });
        let (history, owner) = eog_as_history(&eog, Some(420), 2_000_000).unwrap();
        assert_eq!(owner.riot_id, "Player12#EUW");
        let game = model::game(&history, &owner).unwrap();
        assert_eq!((game.id, game.queue.as_str(), game.win, game.champion_id, game.kills, game.deaths, game.assists, game.cs, game.duration),
            (42, "solo", false, 112, 2, 2, 9, 192, 1830));
        assert_eq!((game.items.clone(), game.spells.clone(), game.keystone, game.level, game.role.as_str()), (vec![3089, 0, 3020, 0, 0, 0, 3364], vec![4, 14], 8112, 16, "mid"));
        assert_eq!(game.created_at, 2_000_000 - 1_830_000);
        assert!(game.placement > 0 && game.opponent.is_some(), "placement and lane opponent come from all ten players");
        assert!(crate::match_detail::remember_ended(&history, &owner));
        // Without a queue the game is not a dashboard row; without a game id there is nothing.
        assert!(eog_as_history(&eog, None, 0).is_none());
        let mut nameless = eog.clone();
        nameless["gameId"] = 0.into();
        assert!(eog_as_history(&nameless, Some(420), 0).is_none());
    }

    #[test]
    fn finished_game_is_found_in_the_history_list() {
        let history = serde_json::json!({ "games": { "games": [{ "gameId": 7 }, { "gameId": 9 }] } });
        assert!(history_has_game(&history, 9));
        assert!(!history_has_game(&history, 8));
        assert!(!history_has_game(&serde_json::json!({}), 9));
    }

    #[test]
    fn cached_snapshot_survives_restart_and_rejects_corrupt_or_oversized_files() {
        use super::*;
        let path =
            std::env::temp_dir().join(format!("dashboard-test-{}.json", uuid::Uuid::new_v4()));
        let snapshot = Snapshot {
            summoner: model::summoner(
                &serde_json::json!({ "puuid": "test", "gameName": "Cached", "tagLine": "EUW" }),
            )
            .unwrap(),
            rank: None,
            fetched_at: 1234,
            games: vec![],
            summaries: model::summaries(&[]),
            champion_names: Default::default(),
            lp_history: vec![],
            mmr: None,
        };
        std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let cached = read_cache_at(&path).unwrap();
        assert_eq!(cached.fetched_at, 1234);
        assert_eq!(cached.summoner.riot_id, "Cached#EUW");
        assert_eq!(cached.summaries["all"].metrics[0].average, None);
        std::fs::write(&path, b"broken json").unwrap();
        assert!(read_cache_at(&path).is_none());
        std::fs::write(&path, vec![b' '; 512 * 1024 + 1]).unwrap();
        assert!(read_cache_at(&path).is_none());
        std::fs::remove_file(path).unwrap();
    }
}
