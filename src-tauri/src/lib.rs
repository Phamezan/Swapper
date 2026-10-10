mod hotkey;
mod identity;
mod korean_font;
mod lifecycle;
mod lcu;
mod remote;
mod notify;
mod doctor;
mod repair;
mod riot;
mod riot_client;
mod runes;
mod updater;
mod shortcuts;
mod vault;
pub mod windows;

use base64::Engine;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{path::BaseDirectory, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_positioner::{Position, WindowExt};
use uuid::Uuid;

const TRAY_ID: &str = "swapper-tray";
const DEFAULT_TRAY_TOOLTIP: &str = "Swapper · Riot account switcher";
const RECONNECT_TOOLTIP_SUFFIX: &str = " · Phone needs to reconnect";
const AUTOSTART_ARG: &str = "--autostart";

#[derive(Clone, Default)]
pub struct SwitchGuard {
    in_progress: Arc<AtomicBool>,
    /// Bumped on every acquired lease. Background watches compare the epoch
    /// they started with and stop when it has moved on, so a stale watch can
    /// never fire for a session another action already replaced.
    epoch: Arc<AtomicU64>,
}

#[derive(Debug)]
pub struct SwitchLease {
    in_progress: Arc<AtomicBool>,
}

impl SwitchGuard {
    pub fn new() -> Self {
        Self {
            in_progress: Arc::new(AtomicBool::new(false)),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn is_switching(&self) -> bool {
        self.in_progress.load(Ordering::SeqCst)
    }

    /// The epoch value handed to a background watch so it can detect that
    /// another account action started after its switch.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub(crate) fn epoch_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.epoch)
    }

    pub fn acquire(&self) -> Result<SwitchLease, String> {
        if self
            .in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.epoch.fetch_add(1, Ordering::SeqCst);
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

pub(crate) struct AppState {
    config: Mutex<vault::Config>,
    bundled_deceive: PathBuf,
    /// Bundled `.fantome` the Korean font overlay is built from. Its patcher is
    /// resolved at use time from the user's LTK Manager install.
    bundled_korean_font: PathBuf,
    switch_guard: SwitchGuard,
    remote: Arc<remote::RemoteCore>,
    /// Whether the saved global hotkey is actually registered with the OS.
    /// A saved hotkey can be inactive when another application owns it.
    hotkey_active: AtomicBool,
}

impl AppState {
    /// Whether an account action is running. The updater stays out of the way
    /// while it is.
    pub(crate) fn is_switching(&self) -> bool {
        self.switch_guard.is_switching()
    }

    fn auto_apply_top_preset(&self) -> bool {
        self.config
            .lock()
            .map(|config| config.auto_apply_top_preset)
            .unwrap_or(false)
    }

    pub(crate) fn set_auto_apply_top_preset(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        config.auto_apply_top_preset = enabled;
        vault::save(&config)
    }

    /// The op.gg rank bracket, defaulting to Emerald+ when unset or unknown.
    pub(crate) fn rune_tier(&self) -> String {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.rune_tier.clone())
            .map(|tier| runes::normalize_tier(&tier).to_string())
            .unwrap_or_else(|| runes::DEFAULT_TIER.to_string())
    }

    pub(crate) fn set_rune_tier(&self, tier: &str) -> Result<(), String> {
        let Some(slug) = runes::tier_slug(tier) else {
            return Err("That rank filter is not available.".into());
        };
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.rune_tier.as_deref() == Some(slug) {
            return Ok(());
        }
        config.rune_tier = Some(slug.to_string());
        vault::save(&config)
    }

    /// The League page id Swapper owns, if it has created one yet.
    pub(crate) fn rune_page_id(&self) -> Option<i64> {
        self.config
            .lock()
            .map(|config| config.rune_page_id)
            .unwrap_or(None)
    }

    pub(crate) fn set_rune_page_id(&self, id: i64) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.rune_page_id == Some(id) {
            return Ok(());
        }
        config.rune_page_id = Some(id);
        vault::save(&config)
    }

    pub(crate) fn publish_runes(&self, applied: runes::AppliedView) {
        self.remote.publish_runes(applied);
    }

    /// Whether applying a page should also set its summoner spells. Defaults to
    /// on until the user changes it.
    pub(crate) fn apply_spells_with_runes(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.apply_spells_with_runes)
            .unwrap_or(true)
    }

    pub(crate) fn set_apply_spells_with_runes(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.apply_spells_with_runes == Some(enabled) {
            return Ok(());
        }
        config.apply_spells_with_runes = Some(enabled);
        vault::save(&config)
    }

    /// Whether applying a recommended preset also imports its item build.
    /// Defaults to on until the user changes it.
    pub(crate) fn import_items_with_runes(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.import_items_with_runes)
            .unwrap_or(true)
    }

    pub(crate) fn set_import_items_with_runes(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.import_items_with_runes == Some(enabled) {
            return Ok(());
        }
        config.import_items_with_runes = Some(enabled);
        vault::save(&config)
    }

    /// Whether gameflow notifications (ready check, champion select) are on.
    /// Defaults to on until the user changes it.
    pub(crate) fn notifications_enabled(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.notifications_enabled)
            .unwrap_or(true)
    }

    pub(crate) fn set_notifications_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.notifications_enabled == Some(enabled) {
            return Ok(());
        }
        config.notifications_enabled = Some(enabled);
        vault::save(&config)
    }

    /// Whether ready-check notifications are on, independently of champion
    /// select. Defaults to on until the user changes it.
    pub(crate) fn ready_check_notifications(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.ready_check_notifications)
            .unwrap_or(true)
    }

    pub(crate) fn set_ready_check_notifications(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.ready_check_notifications == Some(enabled) {
            return Ok(());
        }
        config.ready_check_notifications = Some(enabled);
        vault::save(&config)
    }

    /// Whether the optional Korean font is on. Off until the user turns it on.
    pub(crate) fn korean_font_enabled(&self) -> bool {
        self.config
            .lock()
            .map(|config| config.korean_font)
            .unwrap_or(false)
    }

    pub(crate) fn set_korean_font_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.korean_font == enabled {
            return Ok(());
        }
        config.korean_font = enabled;
        vault::save(&config)
    }
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
    auto_apply_top_preset: bool,
    rune_tier: String,
    apply_spells_with_runes: bool,
    import_items_with_runes: bool,
    hotkey: Option<String>,
    hotkey_active: bool,
    notifications_enabled: bool,
    ready_check_notifications: bool,
    korean_font: bool,
    /// Whether the user's LTK Manager install has the patcher host the Korean
    /// font needs. The UI shows the toggle only when it is.
    ltk_installed: bool,
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
    /// Whether the failure left a dead saved session the user can fix through
    /// the Repair Account flow (see `repair::is_repairable`).
    repairable: bool,
}

fn view(
    config: &vault::Config,
    bundled_deceive: &std::path::Path,
    remote: &remote::RemoteStatus,
    is_switching: bool,
    hotkey_active: bool,
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
        auto_apply_top_preset: config.auto_apply_top_preset,
        rune_tier: config
            .rune_tier
            .clone()
            .map(|tier| runes::normalize_tier(&tier).to_string())
            .unwrap_or_else(|| runes::DEFAULT_TIER.to_string()),
        apply_spells_with_runes: config.apply_spells_with_runes.unwrap_or(true),
        import_items_with_runes: config.import_items_with_runes.unwrap_or(true),
        hotkey: config.hotkey.clone(),
        hotkey_active,
        notifications_enabled: config.notifications_enabled.unwrap_or(true),
        ready_check_notifications: config.ready_check_notifications.unwrap_or(true),
        korean_font: config.korean_font,
        ltk_installed: korean_font::patcher_path().is_some(),
        remote: remote.clone(),
    }
}

fn update_tray_tooltip(app: &tauri::AppHandle, tooltip: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

/// The idle tray tooltip, plus the reconnect hint while paired phones must
/// scan a new QR code.
fn default_tray_tooltip(app: &tauri::AppHandle) -> String {
    let needed = app
        .try_state::<AppState>()
        .is_some_and(|state| state.remote.status().lan_reconnect_needed);
    if needed {
        format!("{DEFAULT_TRAY_TOOLTIP}{RECONNECT_TOOLTIP_SUFFIX}")
    } else {
        DEFAULT_TRAY_TOOLTIP.to_string()
    }
}

/// Restores the tray tooltip from the current reconnect state, unless a switch
/// owns the tooltip; the switch restores it when it finishes.
pub(crate) fn refresh_tray_tooltip(app: &tauri::AppHandle) {
    let switching = app
        .try_state::<AppState>()
        .is_some_and(|state| state.switch_guard.is_switching());
    if switching {
        return;
    }
    update_tray_tooltip(app, &default_tray_tooltip(app));
}

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> Result<AppView, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        state.switch_guard.is_switching(),
        state.hotkey_active.load(Ordering::SeqCst),
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
        state.hotkey_active.load(Ordering::SeqCst),
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
        state.hotkey_active.load(Ordering::SeqCst),
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

/// Opens Riot Client so the user can sign in to the account whose saved
/// session could not be restored. Nothing is saved until `complete_repair`
/// verifies the signed-in identity belongs to that account.
#[tauri::command]
fn begin_repair(state: State<'_, AppState>, id: Uuid) -> Result<(), String> {
    let _lease = state.switch_guard.acquire()?;
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let account = config
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| "Account no longer exists.".to_string())?;
    repair::ensure_repairable(account)?;
    riot::open_riot(&config)
}

/// Replaces a repaired account's saved session with the live one. The live
/// identity is re-read here so a login change between detection and this
/// call can never save the wrong session (same rule as complete_add).
#[tauri::command]
async fn complete_repair(state: State<'_, AppState>, id: Uuid) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    let live = match identity::detect().await {
        identity::Detection::Identified(found) => found,
        other => return Err(detection_error(&other)),
    };
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    repair::complete(&mut config, id, &live)?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
        state.hotkey_active.load(Ordering::SeqCst),
    ))
}

/// Starts switching to one saved account: guard lease, notifications, tray
/// tooltip, then the switch on a background thread. Shared by the UI command
/// and deep-link shortcuts so both get identical safety checks.
fn start_switch(app: &tauri::AppHandle, state: &AppState, id: Uuid) -> Result<(), String> {
    // Acquire guard lease first
    let lease = state.switch_guard.acquire()?;

    let (name, use_deceive, expected_puuid, watch_epoch) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let target = config
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| "Account no longer exists.".to_string())?;
        (
            target.display_name(),
            config.use_deceive,
            target.puuid.clone(),
            state.switch_guard.epoch(),
        )
    };

    notify::switch_started(app, &name);
    update_tray_tooltip(app, &format!("Swapper · Switching to {name}…"));
    let _ = app.emit("switch_started", SwitchStartedPayload { id, name: name.clone() });

    let app_handle = app.clone();
    let app = app.clone();
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
                app_state.hotkey_active.load(Ordering::SeqCst),
            );
            Ok::<_, (riot::SwitchFailure, String)>(updated_view)
        })
        .await;

        update_tray_tooltip(&app, &default_tray_tooltip(&app));

        match result {
            Ok(Ok(updated_view)) => {
                notify::switch_succeeded(&app, &name, use_deceive);
                let _ = app.emit("switch_done", SwitchDonePayload { id, view: updated_view });
                // Watch in the background for a session that was dead on
                // arrival; the watch cancels itself when any other account
                // action starts. Accounts saved without a PUUID cannot be
                // verified, so they are never watched.
                if let Some(expected_puuid) = expected_puuid {
                    if let Some(app_state) = app.try_state::<AppState>() {
                        repair::spawn_session_watch(
                            app.clone(),
                            id,
                            name,
                            expected_puuid,
                            watch_epoch,
                            app_state.switch_guard.epoch_handle(),
                        );
                    }
                }
            }
            Ok(Err((failure, friendly_error))) => {
                notify::switch_failed(&app, &name, use_deceive, &failure);
                let _ = app.emit("switch_failed", SwitchFailedPayload { id, error: friendly_error, repairable: repair::is_repairable(failure.kind) });
            }
            Err(join_err) => {
                let err_msg = format!("Background switch error: {join_err}");
                let failure = riot::SwitchFailure::from(err_msg.clone());
                notify::switch_failed(&app, &name, use_deceive, &failure);
                let _ = app.emit("switch_failed", SwitchFailedPayload { id, error: err_msg, repairable: repair::is_repairable(failure.kind) });
            }
        }
    });

    Ok(())
}

#[tauri::command]
fn switch_account(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<(), String> {
    start_switch(&app, &state, id)
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
        state.hotkey_active.load(Ordering::SeqCst),
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
        state.hotkey_active.load(Ordering::SeqCst),
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
        state.hotkey_active.load(Ordering::SeqCst),
    ))
}

/// Sets or clears the global hotkey. `None` disables it. Validation and
/// registration happen before the change is persisted, and the previous
/// combination is only released once the new one registered, so a rejected
/// combination leaves the previous shortcut in effect.
#[tauri::command]
fn set_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    shortcut: Option<String>,
) -> Result<AppView, String> {
    let raw = shortcut
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let incoming = raw
        .as_deref()
        .map(|value| hotkey::parse(value).map(|parsed| (parsed, hotkey::canonical(parsed))))
        .transpose()?;
    // An inactive saved hotkey (another app held it at startup) is retried
    // even when the same combination is recorded again.
    let unchanged = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.hotkey.as_deref() == incoming.as_ref().map(|(_, text)| text.as_str())
            && (incoming.is_none() || state.hotkey_active.load(Ordering::SeqCst))
    };
    if unchanged {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        return Ok(view(
            &config,
            &state.bundled_deceive,
            &state.remote.status(),
            false,
            state.hotkey_active.load(Ordering::SeqCst),
        ));
    }
    let previous = state.config.lock().map_err(|e| e.to_string())?.hotkey.clone();
    let registered = previous.as_deref().and_then(|value| hotkey::parse(value).ok());
    let wanted = incoming.as_ref().map(|(parsed, _)| *parsed);
    hotkey::swap(&app, registered, wanted)?;
    let saved = {
        let mut config = state.config.lock().map_err(|e| e.to_string())?;
        config.hotkey = incoming.map(|(_, text)| text);
        match vault::save(&config) {
            Ok(()) => Ok(()),
            Err(error) => {
                // Put the in-memory setting back so it matches disk and the
                // registration that swap is about to restore.
                config.hotkey = previous.clone();
                Err(error)
            }
        }
    };
    if let Err(error) = saved {
        // Best effort: the previous combination was registered moments ago,
        // but re-registering it can race with another application.
        let _ = hotkey::swap(&app, wanted, registered);
        state
            .hotkey_active
            .store(registered.is_some(), Ordering::SeqCst);
        return Err(error);
    }
    state
        .hotkey_active
        .store(wanted.is_some(), Ordering::SeqCst);
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
        state.hotkey_active.load(Ordering::SeqCst),
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
fn create_lan_pairing_url(state: State<'_, AppState>) -> Result<String, String> {
    state.remote.create_lan_pairing_url()
}

#[tauri::command]
fn reset_lan_access(state: State<'_, AppState>) -> Result<String, String> {
    state.remote.reset_lan_access()?;
    state.remote.create_lan_pairing_url()
}

#[tauri::command]
fn list_paired_lan_devices(state: State<'_, AppState>) -> Vec<remote::PairedDeviceView> {
    state.remote.paired_devices()
}

#[tauri::command]
fn revoke_lan_device(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.remote.revoke_paired_device(&id)
}

#[tauri::command]
fn rename_lan_device(state: State<'_, AppState>, id: String, name: String) -> Result<String, String> {
    state.remote.rename_paired_device(&id, &name)
}

#[tauri::command]
fn dismiss_lan_reconnect(state: State<'_, AppState>) {
    state.remote.dismiss_lan_reconnect();
}

#[tauri::command]
fn remove_stale_lan_devices(state: State<'_, AppState>) -> Result<usize, String> {
    state.remote.remove_stale_lan_devices()
}

#[tauri::command]
fn set_auto_apply_top_preset(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    state.set_auto_apply_top_preset(enabled)?;
    // Turning the setting on with the champion already locked applies now,
    // instead of waiting for the watcher's next poll.
    if enabled {
        tauri::async_runtime::spawn(async move {
            let _ = runes::auto_apply_current(&app).await;
        });
    }
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
        state.hotkey_active.load(Ordering::SeqCst),
    ))
}

#[tauri::command]
async fn get_runes(
    state: State<'_, AppState>,
    position: Option<String>,
) -> Result<runes::RunesView, String> {
    let mut view = runes::view(
        state.auto_apply_top_preset(),
        state.apply_spells_with_runes(),
        &state.rune_tier(),
        position.as_deref(),
    )
    .await;
    view.import_items = state.import_items_with_runes();
    Ok(view)
}

#[tauri::command]
fn set_apply_spells_with_runes(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    state.set_apply_spells_with_runes(enabled)?;
    let view = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        view(
            &config,
            &state.bundled_deceive,
            &state.remote.status(),
            false,
            state.hotkey_active.load(Ordering::SeqCst),
        )
    };
    notify_runes_changed(&app, &state);
    Ok(view)
}

#[tauri::command]
fn set_import_items_with_runes(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    state.set_import_items_with_runes(enabled)?;
    let view = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        view(
            &config,
            &state.bundled_deceive,
            &state.remote.status(),
            false,
            state.hotkey_active.load(Ordering::SeqCst),
        )
    };
    notify_runes_changed(&app, &state);
    Ok(view)
}

/// Applies one summoner spell to a slot, swapping when it is already in the
/// other slot. Published so both surfaces reload the new spells.
#[tauri::command]
async fn apply_spell(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    slot: String,
    spell_id: i64,
) -> Result<(), String> {
    let slot = if slot.eq_ignore_ascii_case("d") || slot == "1" { 0 } else { 1 };
    runes::spells::apply_pick(slot, spell_id)
        .await
        .map_err(|e| e.message().to_string())?;
    notify_runes_changed(&app, &state);
    Ok(())
}

/// Allows Swapper through Windows Firewall on Private networks. Windows shows
/// one elevation prompt for Swapper; nothing is asked without this click.
#[tauri::command]
async fn allow_lan_firewall(state: State<'_, AppState>) -> Result<AppView, String> {
    let remote = state.remote.clone();
    tauri::async_runtime::spawn_blocking(move || remote.allow_lan_firewall())
        .await
        .map_err(|error| error.to_string())??;
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
        state.hotkey_active.load(Ordering::SeqCst),
    ))
}

#[tauri::command]
fn set_notifications_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    state.set_notifications_enabled(enabled)?;
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
        state.hotkey_active.load(Ordering::SeqCst),
    ))
}

#[tauri::command]
fn set_ready_check_notifications(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    state.set_ready_check_notifications(enabled)?;
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
        state.hotkey_active.load(Ordering::SeqCst),
    ))
}

/// Persists the Korean font toggle and, when turned on mid-game, starts the
/// patcher now instead of waiting for the watcher's next poll. Turning it off
/// stops the patcher immediately.
#[tauri::command]
fn set_korean_font(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    state.set_korean_font_enabled(enabled)?;
    if enabled {
        korean_font::on_enabled(&app);
    } else {
        korean_font::stop();
    }
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
        state.hotkey_active.load(Ordering::SeqCst),
    ))
}

#[tauri::command]
fn set_rune_tier(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    tier: String,
) -> Result<AppView, String> {
    let _lease = state.switch_guard.acquire()?;
    state.set_rune_tier(&tier)?;
    let view = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        view(
            &config,
            &state.bundled_deceive,
            &state.remote.status(),
            false,
            state.hotkey_active.load(Ordering::SeqCst),
        )
    };
    notify_runes_changed(&app, &state);
    Ok(view)
}

/// Nudges the desktop flyout and the phone to reload runes with the new setting.
fn notify_runes_changed(app: &tauri::AppHandle, state: &AppState) {
    state.remote.notify_runes_changed();
    let _ = app.emit("runes_changed", serde_json::Value::Null);
}

#[tauri::command]
async fn champ_select_status() -> runes::ChampSelectEvent {
    runes::current_status().await
}

#[tauri::command]
async fn rune_icon(id: i64) -> Result<String, String> {
    let bytes = runes::icon(id).await.map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
async fn role_icon(role: String) -> Result<String, String> {
    let bytes = runes::roles::icon(&role)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
async fn spell_icon(id: i64) -> Result<String, String> {
    let bytes = runes::spells::icon(id)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
async fn item_icon(id: i64) -> Result<String, String> {
    let bytes = runes::items::icon(id)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
async fn rank_icon(tier: String) -> Result<String, String> {
    let bytes = runes::ranks::icon(&tier)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
async fn get_pro_builds(champion_id: i64, position: String, page: Option<u32>) -> runes::ProBuildsView {
    runes::pro_builds_view(champion_id, &position, page.unwrap_or(1)).await
}

/// The 6-item build people build with one preset's keystone, from lolalytics.
/// `None` means there is no build to show for that card.
#[tauri::command]
async fn get_keystone_build(
    champion_id: i64,
    position: String,
    tier: String,
    keystone: i64,
) -> Option<runes::KeystoneBuildView> {
    runes::preset_build_view(champion_id, &position, &tier, keystone).await
}

/// The lane matchup build against the enemy laner, from lolalytics. `None`
/// means there is nothing to show; a low sample comes back flagged `fallback`.
#[tauri::command]
async fn get_matchup(
    champion_id: i64,
    enemy_champion_id: i64,
    position: String,
    tier: String,
) -> Option<runes::MatchupView> {
    runes::matchup_view(champion_id, enemy_champion_id, &position, &tier).await
}

/// A champion's matchup table (who beats it, who it beats), from lolalytics.
#[tauri::command]
async fn get_champion_counters(
    champion_id: i64,
    position: Option<String>,
    tier: String,
) -> runes::ChampionCountersView {
    runes::champion_counters_view(champion_id, position.as_deref(), &tier).await
}

/// A champion's tier, rates, lane rank and most common build, from lolalytics.
#[tauri::command]
async fn get_champion_overview(
    champion_id: i64,
    position: Option<String>,
    tier: String,
) -> runes::ChampionOverviewView {
    runes::champion_overview_view(champion_id, position.as_deref(), &tier).await
}

/// The strongest champions for a role and rank bracket, from lolalytics.
#[tauri::command]
async fn get_tier_list(position: String, tier: String) -> runes::TierListView {
    runes::tier_list_view(&position, &tier).await
}

/// Every champion, for the Champion tab search box.
#[tauri::command]
async fn get_champion_list() -> Vec<runes::ChampionOption> {
    runes::champion_list().await
}

#[tauri::command]
async fn champion_icon(id: i64) -> Result<String, String> {
    let bytes = runes::champion_icon::icon(id)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
async fn import_item_build(
    champion_id: i64,
    champion_name: String,
    source: String,
    items: Vec<i64>,
) -> Result<String, String> {
    runes::item_sets::import_build(champion_id, &champion_name, &source, &items)
        .await
        .map_err(|error| error.message().to_string())
}

#[tauri::command]
async fn apply_rune_page(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    selection: runes::RuneSelection,
    preset_index: Option<usize>,
    spells: Option<Vec<i64>>,
    position: Option<String>,
    enemy_champion_id: Option<i64>,
) -> Result<runes::AppliedView, String> {
    let owned = state.rune_page_id();
    let keystone = selection.keystone;
    let spells = spells.as_deref().and_then(runes::spells::pair_from_ids);
    let applied = runes::apply_selection(
        selection,
        preset_index,
        owned,
        spells,
        state.apply_spells_with_runes(),
    )
    .await
    .map_err(|e| e.message().to_string())?;
    // Presets carry a recommended item build; exact pro pages import items
    // through their own button.
    if (preset_index.is_some() || enemy_champion_id.is_some()) && state.import_items_with_runes() {
        runes::spawn_preset_items_import(position, state.rune_tier(), keystone, enemy_champion_id);
    }
    if let Some(id) = applied.page_id {
        let handle = app.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Some(state) = handle.try_state::<AppState>() {
                let _ = state.set_rune_page_id(id);
            }
        })
        .await;
    }
    state.remote.clone().publish_runes(applied.clone());
    let _ = app.emit("runes_changed", applied.clone());
    Ok(applied)
}

#[tauri::command]
fn hide_flyout(app: tauri::AppHandle) -> Result<(), String> {
    hide_main(&app.get_webview_window("main").ok_or("Flyout is unavailable")?)
}

/// Tells WebView2 how much memory the flyout may keep. While it sits hidden in
/// the tray, Low lets WebView2 release most of its caches; Normal comes back
/// before it is shown again.
fn set_flyout_memory(window: &tauri::WebviewWindow, low: bool) {
    let _ = window.with_webview(move |webview| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        };
        use windows_core::Interface;
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        if let Ok(core) = core.cast::<ICoreWebView2_19>() {
            let _ = core.SetMemoryUsageTargetLevel(if low {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
            } else {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
            });
        }
    });
}

/// Every way the flyout hides goes through here, so it always drops to the
/// low memory target.
fn hide_main(window: &tauri::WebviewWindow) -> Result<(), String> {
    let result = window.hide().map_err(|e| e.to_string());
    set_flyout_memory(window, true);
    result
}

#[tauri::command]
fn open_windows_network_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd.exe")
            .args(["/C", "start", "", remote::network_settings_page()])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open Windows network settings: {error}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Windows network settings are only available on Windows.".into())
    }
}

fn show_flyout(app: &tauri::AppHandle, destination: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.as_ref().window().move_window(Position::TrayCenter);
        let _ = app.emit("navigate", destination);
        set_flyout_memory(&window, false);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// The flyout stays open until the tray icon or the close button hides it, so a
// tray left-click toggles by visibility rather than focus (clicking the tray
// moves focus away before the event arrives). Only the Up event toggles once.
fn toggle_flyout(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = hide_main(&window);
    } else {
        show_flyout(app, "accounts");
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
    // The elevated copy Swapper starts to allow itself in Windows Firewall:
    // add the rule and exit before any window, tray icon or single-instance
    // handoff exists.
    if std::env::args().any(|arg| arg == remote::network::ALLOW_FIREWALL_ARG) {
        std::process::exit(match remote::network::add_private_app_rule() {
            Ok(()) => 0,
            Err(_) => 1,
        });
    }
    let config = vault::load().expect("Swapper settings could not be loaded");
    let launched_at_startup = std::env::args().any(|arg| arg == AUTOSTART_ARG);
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // With the deep-link feature the second instance's arguments were
            // already forwarded to the deep-link plugin, whose handler (see
            // shortcuts::init) performs the switch. A second plain launch
            // behaves like a tray click instead of starting a second Swapper.
            let has_link = args.iter().any(|arg| arg.starts_with("swapper://"));
            let is_autostart = args.iter().any(|arg| arg == AUTOSTART_ARG);
            if !has_link && !is_autostart {
                show_flyout(app, "accounts");
            }
        }))
        .setup(move |app| {
            let saved_hotkey = config.hotkey.clone();
            let bundled_deceive = app.path().resolve("Deceive.exe", BaseDirectory::Resource)?;
            let bundled_korean_font =
                app.path().resolve("korean-font.fantome", BaseDirectory::Resource)?;
            let remote = remote::RemoteCore::new(app.handle().clone(), config.remote_enabled);
            app.manage(AppState {
                config: Mutex::new(config),
                bundled_deceive,
                bundled_korean_font,
                switch_guard: SwitchGuard::new(),
                remote: remote.clone(),
                hotkey_active: AtomicBool::new(false),
            });
            let auto_enable = remote.clone();
            std::thread::spawn(move || {
                if auto_enable.status().enabled {
                    auto_enable.set_enabled(true);
                }
            });
            runes::spawn_watch(app.handle().clone());
            app.handle().plugin(tauri_plugin_positioner::init())?;
            app.handle().plugin(tauri_plugin_notification::init())?;
            app.handle().plugin(tauri_plugin_dialog::init())?;
            app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
            app.manage(updater::UpdateState::new(app.handle().clone()));
            updater::spawn(app.handle().clone());
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, _shortcut, event| {
                        // Fire once per press; the Released half is ignored.
                        if event.state == ShortcutState::Pressed {
                            toggle_flyout(app);
                        }
                    })
                    .build(),
            )?;
            let hotkey_registered = hotkey::register_saved(app.handle(), saved_hotkey.as_deref());
            if let Some(state) = app.try_state::<AppState>() {
                state.hotkey_active.store(hotkey_registered, Ordering::SeqCst);
            }
            app.handle().plugin(tauri_plugin_deep_link::init())?;
            // After the notification plugin so shortcut links can always report
            // their outcome, and after AppState exists so links can switch.
            shortcuts::init(app.handle().clone());
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
                        toggle_flyout(tray.app_handle());
                    }
                })
                .build(app)?;
            // Windows starts Swapper through the autostart entry with --autostart.
            // A login launch must never open the flyout, so keep the window hidden
            // and leave the tray icon as the only entry point. The window already
            // starts hidden (tauri.conf.json `visible: false`); this guard keeps
            // that intent explicit if a future change adds a startup show.
            if let Some(window) = app.get_webview_window("main") {
                if launched_at_startup {
                    let _ = hide_main(&window);
                } else if !window.is_visible().unwrap_or(false) {
                    // The window starts hidden; start it on the low target too.
                    set_flyout_memory(&window, true);
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                match window.app_handle().get_webview_window(window.label()) {
                    Some(webview) => {
                        let _ = hide_main(&webview);
                    }
                    None => {
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            open_riot,
            begin_add,
            detect_account_identity,
            complete_add,
            begin_repair,
            complete_repair,
            switch_account,
            shortcuts::create_account_shortcut,
            set_nickname,
            remove_account,
            save_settings,
            hide_flyout,
            set_hotkey,
            open_windows_network_settings,
            set_remote_enabled,
            probe_remote,
            create_lan_pairing_url,
            reset_lan_access,
            list_paired_lan_devices,
            revoke_lan_device,
            rename_lan_device,
            dismiss_lan_reconnect,
            remove_stale_lan_devices,
            set_auto_apply_top_preset,
            set_rune_tier,
            set_apply_spells_with_runes,
            set_import_items_with_runes,
            set_notifications_enabled,
            allow_lan_firewall,
            set_ready_check_notifications,
            set_korean_font,
            korean_font::install_ltk_manager,
            korean_font::open_ltk_website,
            get_runes,
            champ_select_status,
            rune_icon,
            role_icon,
            spell_icon,
            item_icon,
            rank_icon,
            apply_spell,
            get_pro_builds,
            get_keystone_build,
            get_matchup,
            get_champion_counters,
            get_champion_overview,
            get_tier_list,
            get_champion_list,
            champion_icon,
            import_item_build,
            apply_rune_page,
            updater::update_status,
            updater::update_check_now,
            updater::update_install,
            doctor::run_doctor,
            doctor::run_doctor_check,
            doctor::doctor_report
        ])
        .build(tauri::generate_context!())
        .expect("Could not start Swapper")
        .run(|_app, event| {
            // Never leave the Korean font patcher running once Swapper exits.
            if let tauri::RunEvent::Exit = event {
                korean_font::stop();
            }
        });
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
