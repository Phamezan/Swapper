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

const TRAY_ID: &str = "swapper-tray";
const DEFAULT_TRAY_TOOLTIP: &str = "Swapper · Riot account switcher";
const AUTOSTART_ARG: &str = "--autostart";

#[derive(Clone, Default)]
pub struct SwitchGuard {
    in_progress: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct SwitchLease {
    in_progress: Arc<AtomicBool>,
}

impl SwitchGuard {
    pub fn new() -> Self {
        Self {
            in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_switching(&self) -> bool {
        self.in_progress.load(Ordering::SeqCst)
    }

    pub fn acquire(&self) -> Result<SwitchLease, String> {
        if self
            .in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Ok(SwitchLease {
                in_progress: Arc::clone(&self.in_progress),
            })
        } else {
            Err("Another account action is in progress. Try again when it finishes.".into())
        }
    }
}

impl Drop for SwitchLease {
    fn drop(&mut self) {
        self.in_progress.store(false, Ordering::SeqCst);
    }
}

struct AppState {
    config: Mutex<vault::Config>,
    bundled_deceive: PathBuf,
    keep_flyout_open: AtomicBool,
    switch_guard: SwitchGuard,
    remote: Arc<remote::RemoteCore>,
}

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppView {
    accounts: Vec<AccountView>,
    active_id: Option<Uuid>,
    is_switching: bool,
    use_deceive: bool,
    riot_exe: Option<String>,
    riot_detected: bool,
    deceive_detected: bool,
    remote: remote::RemoteStatus,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchStartedPayload {
    id: Uuid,
    name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchDonePayload {
    id: Uuid,
    view: AppView,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchFailedPayload {
    id: Uuid,
    error: String,
}

fn view(
    config: &vault::Config,
    bundled_deceive: &std::path::Path,
    remote: &remote::RemoteStatus,
    is_switching: bool,
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
        is_switching,
        use_deceive: config.use_deceive,
        riot_exe: config.riot_exe.clone(),
        riot_detected: riot::riot_path(config).is_some(),
        deceive_detected: riot::deceive_path(config, bundled_deceive).is_some(),
        remote: remote.clone(),
    }
}

fn update_tray_tooltip(app: &tauri::AppHandle, tooltip: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> Result<AppView, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        state.switch_guard.is_switching(),
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
    let _lease = state.switch_guard.acquire()?;
    // Detect identity first, while Riot Client is still alive; the
    // backend verifies it before overwriting any saved session.
    let detection = identity::detect().await;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::begin_add(&mut config, &detection)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
    ))
}

#[tauri::command]
async fn complete_add(state: State<'_, AppState>, puuid: String) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
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
        false,
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
fn switch_account(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<(), String> {
    // Acquire guard lease first
    let lease = state.switch_guard.acquire()?;

    let (name, use_deceive) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let target = config
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| "Account no longer exists.".to_string())?;
        (target.display_name(), config.use_deceive)
    };

    notify::switch_started(&app, &name);
    update_tray_tooltip(&app, &format!("Swapper · Switching to {name}…"));
    let _ = app.emit("switch_started", SwitchStartedPayload { id, name: name.clone() });

    let app_handle = app.clone();
    let bundled_deceive = state.bundled_deceive.clone();

    tauri::async_runtime::spawn(async move {
        let _lease = lease;
        let detection = identity::detect_account().await;

        let name_for_switch = name.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let app_state = match app_handle.try_state::<AppState>() {
                Some(state) => state,
                None => {
                    let err = "App state unavailable".to_string();
                    return Err((riot::SwitchFailure::from(err.clone()), err));
                }
            };
            let mut config = match app_state.config.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    let err = format!("Lock error: {e}");
                    return Err((riot::SwitchFailure::from(err.clone()), err));
                }
            };

            riot::switch_account(&mut config, id, &bundled_deceive, &detection)
                .map_err(|failure| {
                    let friendly = notify::human_reason(failure.kind, &name_for_switch, use_deceive)
                        .unwrap_or_else(|| failure.detail.clone());
                    (failure, friendly)
                })?;

            let updated_view = view(
                &config,
                &app_state.bundled_deceive,
                &app_state.remote.status(),
                false,
            );
            Ok::<_, (riot::SwitchFailure, String)>(updated_view)
        })
        .await;

        update_tray_tooltip(&app, DEFAULT_TRAY_TOOLTIP);

        match result {
            Ok(Ok(updated_view)) => {
                notify::switch_succeeded(&app, &name, use_deceive);
                let _ = app.emit("switch_done", SwitchDonePayload { id, view: updated_view });
            }
            Ok(Err((failure, friendly_error))) => {
                notify::switch_failed(&app, &name, use_deceive, &failure);
                let _ = app.emit("switch_failed", SwitchFailedPayload { id, error: friendly_error });
            }
            Err(join_err) => {
                let err_msg = format!("Background switch error: {join_err}");
                let failure = riot::SwitchFailure::from(err_msg.clone());
                notify::switch_failed(&app, &name, use_deceive, &failure);
                let _ = app.emit("switch_failed", SwitchFailedPayload { id, error: err_msg });
            }
        }
    });

    Ok(())
}

#[tauri::command]
fn set_nickname(state: State<'_, AppState>, id: Uuid, name: String) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::set_nickname(&mut config, id, name)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
    ))
}

#[tauri::command]
fn remove_account(state: State<'_, AppState>, id: Uuid) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::remove_account(&mut config, id)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
    ))
}

#[tauri::command]
fn save_settings(
    state: State<'_, AppState>,
    use_deceive: bool,
    riot_exe: Option<String>,
) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    riot::save_settings(&mut config, use_deceive, riot_exe, &state.bundled_deceive)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
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

// Exits without blocking the event loop thread. The tray icon is removed first
// so Windows does not keep a ghost icon while the Tailscale Serve route is torn
// down; that teardown is bounded (see RemoteCore::shutdown) and runs on a
// background thread, which then asks the event loop to exit.
fn exit_app(app: &tauri::AppHandle) {
    let _ = app.remove_tray_by_id(TRAY_ID);
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(state) = app.try_state::<AppState>() {
            state.remote.clone().shutdown();
        }
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = vault::load().expect("Swapper settings could not be loaded");
    let launched_at_startup = std::env::args().any(|arg| arg == AUTOSTART_ARG);
    tauri::Builder::default()
        .setup(move |app| {
            let bundled_deceive = app.path().resolve("Deceive.exe", BaseDirectory::Resource)?;
            let remote = remote::RemoteCore::new(app.handle().clone(), config.remote_enabled);
            app.manage(AppState {
                config: Mutex::new(config),
                bundled_deceive,
                keep_flyout_open: AtomicBool::new(false),
                switch_guard: SwitchGuard::new(),
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
            app.handle().plugin(
                tauri_plugin_autostart::Builder::new()
                    .args([AUTOSTART_ARG])
                    .app_name("Swapper")
                    .build(),
            )?;
            let add = MenuItem::with_id(app, "add", "Add Account", true, None::<&str>)?;
            let edit = MenuItem::with_id(app, "edit", "Edit Account", true, None::<&str>)?;
            let remove = MenuItem::with_id(app, "remove", "Remove Account", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&add, &edit, &remove, &settings, &exit])?;
            TrayIconBuilder::with_id(TRAY_ID)
                .icon(app.default_window_icon().expect("Missing app icon").clone())
                .tooltip(DEFAULT_TRAY_TOOLTIP)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "add" => show_flyout(app, "add"),
                    "edit" => show_flyout(app, "edit"),
                    "remove" => show_flyout(app, "remove"),
                    "settings" => show_flyout(app, "settings"),
                    "exit" => exit_app(app),
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
            // Windows starts Swapper through the autostart entry with --autostart.
            // A login launch must never open the flyout, so keep the window hidden
            // and leave the tray icon as the only entry point. The window already
            // starts hidden (tauri.conf.json `visible: false`); this guard keeps
            // that intent explicit if a future change adds a startup show.
            if launched_at_startup {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_initially_not_switching() {
        let guard = SwitchGuard::new();
        assert!(!guard.is_switching());
    }

    #[test]
    fn guard_acquire_sets_switching_and_blocks_second_acquire() {
        let guard = SwitchGuard::new();
        let lease = guard.acquire().expect("First acquire should succeed");
        assert!(guard.is_switching());

        let second = guard.acquire();
        assert!(second.is_err(), "Second acquire while leased should fail");
        assert_eq!(
            second.unwrap_err(),
            "Another account action is in progress. Try again when it finishes."
        );

        drop(lease);
        assert!(!guard.is_switching());
    }

    #[test]
    fn guard_can_reacquire_after_lease_drop() {
        let guard = SwitchGuard::new();
        {
            let _lease = guard.acquire().unwrap();
            assert!(guard.is_switching());
        }
        assert!(!guard.is_switching());

        let lease2 = guard.acquire().expect("Should be able to acquire after drop");
        assert!(guard.is_switching());
        drop(lease2);
        assert!(!guard.is_switching());
    }

    #[test]
    fn guard_resets_on_panic() {
        let guard = SwitchGuard::new();
        let guard_clone = guard.clone();

        let _ = std::panic::catch_unwind(move || {
            let _lease = guard_clone.acquire().unwrap();
            assert!(guard_clone.is_switching());
            panic!("Simulated panic inside switch task");
        });

        assert!(
            !guard.is_switching(),
            "Guard lease must be released even if task panics"
        );
        let next_lease = guard.acquire();
        assert!(next_lease.is_ok(), "Guard can be acquired after panic unwinds");
    }
}
