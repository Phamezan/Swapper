//! Optional Fullsized App. When on, it replaces the tray flyout: the flyout
//! webview is never created and every entry point opens this window instead.
//! No window or data task exists until it is opened; closing destroys it.
pub mod art;
pub mod ui_data;
pub(crate) mod model;
pub mod routing;

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

fn read_cache() -> Option<Snapshot> {
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

async fn fresh(previous: Option<&Snapshot>) -> Result<Snapshot, String> {
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
                    g.metrics[1].is_some() && g.metrics[4].is_some() && !g.items.is_empty()
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
    // Never publish a mixed snapshot if the account changed during the requests.
    let verify: Value = crate::runes::lcu_get(&client, "/lol-summoner/v1/current-summoner")
        .await
        .map_err(|e| e.to_string())?;
    if verify["puuid"].as_str() != Some(&summoner.puuid) {
        return Err("League account changed; refresh the dashboard.".into());
    }
    let champion_names = crate::runes::champion_list()
        .await
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    Ok(Snapshot {
        champion_names,
        summoner,
        rank: model::rank(&ranked),
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
            match fresh(previous.as_ref()).await {
                Ok(snapshot) => {
                    let bytes = serde_json::to_vec(&snapshot).ok();
                    let _ = tokio::task::spawn_blocking(move || {
                        if let (Some(path), Some(bytes)) = (cache_path(), bytes) {
                            if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
                            let pending = path.with_extension("tmp");
                            if std::fs::write(&pending, bytes).is_ok() { let _ = std::fs::rename(pending, path); }
                        }
                    }).await;
                    Ok(View { snapshot: Some(snapshot), stale: false, message: None })
                },
                Err(error) => Ok(View { snapshot: previous, stale: true, message: Some(error) }),
            }
        } => result
    }
}

#[cfg(test)]
mod tests {
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
