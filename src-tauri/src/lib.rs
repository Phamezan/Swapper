mod identity;
mod lcu;
mod remote;
mod notify;
mod riot;
mod riot_client;
mod vault;
pub mod windows;

use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{path::BaseDirectory, Emitter, Manager, State, WindowEvent};
use tauri_plugin_positioner::{Position, WindowExt};
use uuid::Uuid;

struct AppState {
    config: Mutex<vault::Config>,
    bundled_deceive: PathBuf,
    keep_flyout_open: AtomicBool,
    remote: Arc<remote::RemoteCore>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountView {
    id: Uuid,
    name: String,
    riot_id: Option<String>,
    nickname: Option<String>,
    platform: Option<String>,
    region: Option<String>,
    profile_icon_id: Option<i64>,
    icon_data_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppView {
    accounts: Vec<AccountView>,
    active_id: Option<Uuid>,
    use_deceive: bool,
    riot_exe: Option<String>,
    riot_detected: bool,
    deceive_detected: bool,
    remote: remote::RemoteStatus,
}

fn view(
    config: &vault::Config,
    bundled_deceive: &std::path::Path,
    remote: &remote::RemoteStatus,
) -> AppView {
    AppView {
        accounts: config
            .accounts
            .iter()
            .map(|a| AccountView {
                id: a.id,
                name: a.display_name(),
                riot_id: a.riot_id(),
                nickname: a.nickname.clone(),
                platform: a.platform.clone(),
                region: a.region.clone(),
                profile_icon_id: a.profile_icon_id,
                icon_data_url: vault::profile_icon_data_url(a.id),
            })
            .collect(),
        active_id: config.active_id,
        use_deceive: config.use_deceive,
        riot_exe: config.riot_exe.clone(),
        riot_detected: riot::riot_path(config).is_some(),
        deceive_detected: riot::deceive_path(config, bundled_deceive).is_some(),
        remote: remote.clone(),
    }
}

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> Result<AppView, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
    ))
}

#[tauri::command]
fn open_riot(state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    riot::open_riot(&config)
}

#[tauri::command]
async fn detect_account_identity(
    state: State<'_, AppState>,
) -> Result<identity::DetectOutcome, String> {
    let detection = identity::detect().await;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    identity::outcome_for(&detection, &mut config)
}

#[tauri::command]
async fn begin_add(state: State<'_, AppState>) -> Result<AppView, String> {
    // Detect identity first, while Riot Client is still alive; the
    // backend verifies it before overwriting any saved session.
    let detection = identity::detect().await;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::begin_add(&mut config, &detection)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
    ))
}

#[tauri::command]
async fn complete_add(state: State<'_, AppState>, puuid: String) -> Result<AppView, String> {
    // Re-read the live identity at save time: if the Riot login changed
    // between detection and this click, refuse instead of saving the wrong
    // session under the account the user saw.
    let live = match identity::detect().await {
        identity::Detection::Identified(found) => found,
        other => return Err(detection_error(&other)),
    };
    if live.puuid != puuid.trim() {
        return Err(mismatch_error());
    }
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::complete_add(&mut config, &live)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
    ))
}

fn detection_error(detection: &identity::Detection) -> String {
    match detection {
        identity::Detection::Identified(_) => mismatch_error(),
        identity::Detection::NotRunning => {
            "Open Riot Client and sign in with Stay signed in so Swapper can detect your account.".into()
        }
        identity::Detection::NotReady => {
            "Waiting for Riot Client sign-in. Keep Riot Client open; Swapper will detect the account automatically.".into()
        }
        identity::Detection::Unavailable => {
            "Could not read account identity. Make sure Riot Client is open and signed in.".into()
        }
    }
}

fn mismatch_error() -> String {
    "Different Riot account detected. The current login does not match the account Swapper expected. Save this account separately or sign back into the expected account.".into()
}

#[tauri::command]
async fn switch_account(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<AppView, String> {
    // Detect identity first, while Riot Client is still alive; a
    // mismatch aborts the switch before anything is overwritten.
    let detection = identity::detect().await;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let name = config
        .accounts
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.display_name());
    let use_deceive = config.use_deceive;
    if let Some(name) = name.as_deref() {
        notify::switch_started(&app, name);
    }
    let label = name.as_deref().unwrap_or("the selected account");
    if let Err(failure) = riot::switch_account(&mut config, id, &state.bundled_deceive, &detection) {
        notify::switch_failed(&app, label, use_deceive, &failure);
        return Err(failure.detail);
    }
    notify::switch_succeeded(&app, label, use_deceive);
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
    ))
}

#[tauri::command]
fn set_nickname(state: State<'_, AppState>, id: Uuid, name: String) -> Result<AppView, String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::set_nickname(&mut config, id, name)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
    ))
}

#[tauri::command]
fn remove_account(state: State<'_, AppState>, id: Uuid) -> Result<AppView, String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::remove_account(&mut config, id)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
    ))
}

#[tauri::command]
fn save_settings(
    state: State<'_, AppState>,
    use_deceive: bool,
    riot_exe: Option<String>,
) -> Result<AppView, String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::save_settings(&mut config, use_deceive, riot_exe, &state.bundled_deceive)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
    ))
}

#[tauri::command]
fn set_remote_enabled(state: State<'_, AppState>, enabled: bool) -> Result<remote::RemoteStatus, String> {
    {
        let mut config = state.config.lock().map_err(|e| e.to_string())?;
        config.remote_enabled = enabled;
        vault::save(&config)?;
    }
    state.remote.clone().set_enabled(enabled);
    Ok(state.remote.status())
}

#[tauri::command]
fn probe_remote(state: State<'_, AppState>) -> remote::RemoteStatus {
    state.remote.clone().probe()
}

#[tauri::command]
fn set_add_mode(state: State<'_, AppState>, enabled: bool) {
    state.keep_flyout_open.store(enabled, Ordering::SeqCst);
}

#[tauri::command]
fn hide_flyout(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.keep_flyout_open.store(false, Ordering::SeqCst);
    app.get_webview_window("main")
        .ok_or("Flyout is unavailable")?
        .hide()
        .map_err(|e| e.to_string())
}

fn show_flyout(app: &tauri::AppHandle, destination: &str) {
    if let Some(state) = app.try_state::<AppState>() {
        state
            .keep_flyout_open
            .store(destination == "add", Ordering::SeqCst);
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.as_ref().window().move_window(Position::TrayCenter);
        let _ = app.emit("navigate", destination);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = vault::load().expect("Swapper settings could not be loaded");
    tauri::Builder::default()
        .setup(move |app| {
            let bundled_deceive = app.path().resolve("Deceive.exe", BaseDirectory::Resource)?;
            let remote = remote::RemoteCore::new(app.handle().clone(), config.remote_enabled);
            app.manage(AppState {
                config: Mutex::new(config),
                bundled_deceive,
                keep_flyout_open: AtomicBool::new(false),
                remote: remote.clone(),
            });
            let auto_enable = remote.clone();
            std::thread::spawn(move || {
                if auto_enable.status().enabled {
                    auto_enable.set_enabled(true);
                }
            });
            app.handle().plugin(tauri_plugin_positioner::init())?;
            app.handle().plugin(tauri_plugin_notification::init())?;
            app.handle().plugin(tauri_plugin_dialog::init())?;
            let add = MenuItem::with_id(app, "add", "Add Account", true, None::<&str>)?;
            let edit = MenuItem::with_id(app, "edit", "Edit Account", true, None::<&str>)?;
            let remove = MenuItem::with_id(app, "remove", "Remove Account", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&add, &edit, &remove, &settings, &exit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("Missing app icon").clone())
                .tooltip("Swapper · Riot account switcher")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "add" => show_flyout(app, "add"),
                    "edit" => show_flyout(app, "edit"),
                    "remove" => show_flyout(app, "remove"),
                    "settings" => show_flyout(app, "settings"),
                    "exit" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            state.remote.clone().set_enabled(false);
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_flyout(tray.app_handle(), "accounts");
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    state.keep_flyout_open.store(false, Ordering::SeqCst);
                }
                let _ = window.hide();
            }
            WindowEvent::Focused(false) => {
                let keep_open = window
                    .app_handle()
                    .try_state::<AppState>()
                    .is_some_and(|state| state.keep_flyout_open.load(Ordering::SeqCst));
                if !keep_open {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            open_riot,
            begin_add,
            detect_account_identity,
            complete_add,
            switch_account,
            set_nickname,
            remove_account,
            save_settings,
            set_add_mode,
            hide_flyout,
            set_remote_enabled,
            probe_remote
        ])
        .run(tauri::generate_context!())
        .expect("Could not start Swapper");
}
