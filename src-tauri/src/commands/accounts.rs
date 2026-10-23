use crate::state::{view, AppView};
use crate::{
    default_tray_tooltip, identity, notify, repair, riot, riot_icon, update_tray_tooltip, AppState,
};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

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

/// The installed Riot Client's icon for the "Open Riot Client" button, or none.
#[tauri::command]
pub(crate) async fn riot_client_icon(
    state: State<'_, AppState>,
) -> Result<Option<riot_icon::IconPixels>, String> {
    let path = riot::riot_path(&*state.config.lock().map_err(|e| e.to_string())?);
    Ok(path.and_then(|path| riot_icon::icon(&path, 64)))
}

#[tauri::command]
pub(crate) fn open_riot(state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    riot::open_riot(&config)
}

#[tauri::command]
pub(crate) async fn detect_account_identity(
    state: State<'_, AppState>,
) -> Result<identity::DetectOutcome, String> {
    let detection = identity::detect().await;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    identity::outcome_for(&detection, &mut config)
}

#[tauri::command]
pub(crate) async fn begin_add(state: State<'_, AppState>) -> Result<AppView, String> {
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
pub(crate) async fn complete_add(
    state: State<'_, AppState>,
    puuid: String,
) -> Result<AppView, String> {
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
pub(crate) fn begin_repair(state: State<'_, AppState>, id: Uuid) -> Result<(), String> {
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
pub(crate) async fn complete_repair(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<AppView, String> {
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
pub(crate) fn start_switch(
    app: &tauri::AppHandle,
    state: &AppState,
    id: Uuid,
) -> Result<(), String> {
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
    let _ = app.emit(
        "switch_started",
        SwitchStartedPayload {
            id,
            name: name.clone(),
        },
    );

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

            riot::switch_account(&mut config, id, &bundled_deceive, &detection).map_err(
                |failure| {
                    let friendly =
                        notify::human_reason(failure.kind, &name_for_switch, use_deceive)
                            .unwrap_or_else(|| failure.detail.clone());
                    (failure, friendly)
                },
            )?;

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
                let _ = app.emit(
                    "switch_done",
                    SwitchDonePayload {
                        id,
                        view: updated_view,
                    },
                );
                if let Some(app_state) = app.try_state::<AppState>() {
                    let epoch = app_state.switch_guard.epoch_handle();
                    tauri::async_runtime::spawn(crate::riot_client::launch_league_when_signed_in(
                        move || epoch.load(Ordering::SeqCst) != watch_epoch,
                    ));
                }
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
                let _ = app.emit(
                    "switch_failed",
                    SwitchFailedPayload {
                        id,
                        error: friendly_error,
                        repairable: repair::is_repairable(failure.kind),
                    },
                );
            }
            Err(join_err) => {
                let err_msg = format!("Background switch error: {join_err}");
                let failure = riot::SwitchFailure::from(err_msg.clone());
                notify::switch_failed(&app, &name, use_deceive, &failure);
                let _ = app.emit(
                    "switch_failed",
                    SwitchFailedPayload {
                        id,
                        error: err_msg,
                        repairable: repair::is_repairable(failure.kind),
                    },
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub(crate) fn switch_account(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<(), String> {
    start_switch(&app, &state, id)
}

#[tauri::command]
pub(crate) fn set_nickname(
    state: State<'_, AppState>,
    id: Uuid,
    name: String,
) -> Result<AppView, String> {
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
pub(crate) fn remove_account(state: State<'_, AppState>, id: Uuid) -> Result<AppView, String> {
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
