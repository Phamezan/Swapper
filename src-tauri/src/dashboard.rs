//! Optional desktop dashboard. No window or data task exists until explicitly opened.
mod model;

use base64::Engine;
use futures_util::{stream, StreamExt};
use model::Snapshot;
use serde::Serialize;
use serde_json::Value;
use tauri::{Emitter, Manager};

pub struct Runtime {
    menu: tauri::menu::Menu<tauri::Wry>,
    cancel: tokio::sync::watch::Sender<u64>,
    gate: tokio::sync::Mutex<()>,
}

impl Runtime {
    pub fn new(menu: tauri::menu::Menu<tauri::Wry>) -> Self {
        Self {
            menu,
            cancel: tokio::sync::watch::channel(0).0,
            gate: tokio::sync::Mutex::new(()),
        }
    }
}

pub fn cancel(app: &tauri::AppHandle) {
    app.state::<Runtime>()
        .cancel
        .send_modify(|epoch| *epoch += 1);
}

pub fn enabled(app: &tauri::AppHandle) -> bool {
    app.state::<crate::AppState>()
        .config
        .lock()
        .map(|c| c.dashboard_window)
        .unwrap_or(false)
}

/// The dashboard is read-only; account switching and settings commands stay in the flyout.
pub fn allowed_command(command: &str) -> bool {
    matches!(
        command,
        "dashboard_snapshot"
            | "get_champion_list"
            | "get_tier_list"
            | "get_champion_overview"
            | "get_champion_counters"
            | "get_matchup"
            | "champion_icon"
            | "rune_icon"
            | "spell_icon"
            | "item_icon"
            | "rank_icon"
            | "role_icon"
    )
}

#[tauri::command]
pub fn dashboard_enabled(app: tauri::AppHandle) -> bool {
    enabled(&app)
}

#[tauri::command]
pub fn set_dashboard_enabled(
    app: tauri::AppHandle,
    enabled: bool,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Change dashboard settings from the tray flyout.".into());
    }
    {
        let state = app.state::<crate::AppState>();
        let mut config = state.config.lock().map_err(|e| e.to_string())?;
        let previous = config.dashboard_window;
        config.dashboard_window = enabled;
        if let Err(error) = crate::vault::save(&config) {
            config.dashboard_window = previous;
            return Err(error);
        }
    }
    if !enabled {
        cancel(&app);
        if let Some(window) = app.get_webview_window("dashboard") {
            window.destroy().map_err(|e| e.to_string())?;
        }
    }
    sync_menu(&app)?;
    let _ = app.emit_to("main", "dashboard_enabled", enabled);
    Ok(())
}

pub fn sync_menu(app: &tauri::AppHandle) -> Result<(), String> {
    let runtime = app.state::<Runtime>();
    let menu = &runtime.menu;
    if let Some(item) = menu.get("dashboard") {
        menu.remove(&item).map_err(|e| e.to_string())?;
    }
    if enabled(app) {
        let item =
            tauri::menu::MenuItem::with_id(app, "dashboard", "Open dashboard", true, None::<&str>)
                .map_err(|e| e.to_string())?;
        menu.insert(&item, 0).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn open(app: &tauri::AppHandle) -> Result<(), String> {
    if !enabled(app) {
        return Err("Enable Dashboard window in Settings first.".into());
    }
    if let Some(window) = app.get_webview_window("dashboard") {
        window.unminimize().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    // A separate WebView2 environment lets its renderer/browser processes exit on close.
    let data = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("dashboard-webview");
    tauri::WebviewWindowBuilder::new(
        app,
        "dashboard",
        tauri::WebviewUrl::App("dashboard.html".into()),
    )
    .title("Swapper · Dashboard")
    .inner_size(1280.0, 820.0)
    .min_inner_size(1100.0, 700.0)
    .resizable(true)
    .decorations(true)
    .center()
    .data_directory(data)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn open_dashboard(app: tauri::AppHandle) -> Result<(), String> {
    open(&app)
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
                .filter(|g| g.metrics[1].is_some() && g.metrics[4].is_some())
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
    if window.label() != "dashboard" || !enabled(&app) {
        return Err("The dashboard is closed or disabled.".into());
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

    #[test]
    fn dashboard_only_allows_read_only_commands() {
        for command in [
            "dashboard_snapshot",
            "get_tier_list",
            "get_matchup",
            "champion_icon",
        ] {
            assert!(super::allowed_command(command));
        }
        for command in [
            "switch_account",
            "get_state",
            "set_dashboard_enabled",
            "apply_rune_page",
            "open_riot",
        ] {
            assert!(!super::allowed_command(command));
        }
    }

    #[test]
    fn dashboard_defaults_off_for_old_settings() {
        let config: crate::vault::Config = serde_json::from_value(serde_json::json!({
            "accounts": [], "active_id": null, "use_deceive": false,
            "riot_exe": null, "deceive_exe": null
        }))
        .unwrap();
        assert!(!config.dashboard_window);
    }
}
