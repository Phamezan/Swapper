use crate::state::{view, AppView};
use crate::{hotkey, korean_font, riot, vault, AppState};
use std::sync::atomic::Ordering;
use tauri::State;

#[tauri::command]
pub(crate) fn get_state(state: State<'_, AppState>) -> Result<AppView, String> {
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
pub(crate) fn save_settings(
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
pub(crate) fn set_hotkey(
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
    let previous = state
        .config
        .lock()
        .map_err(|e| e.to_string())?
        .hotkey
        .clone();
    let registered = previous
        .as_deref()
        .and_then(|value| hotkey::parse(value).ok());
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
pub(crate) fn set_notifications_enabled(
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
pub(crate) fn set_ready_check_notifications(
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

/// Whether the first-run setup should open: true only for a new install.
#[tauri::command]
pub(crate) fn onboarding_pending(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state
        .config
        .lock()
        .map_err(|e| e.to_string())?
        .onboarding_pending())
}

/// Marks the first-run setup finished or skipped, so it never opens by itself again.
#[tauri::command]
pub(crate) fn set_onboarded(state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let mut saved = config.clone();
    saved.onboarded = Some(true);
    vault::save(&saved)?;
    *config = saved;
    Ok(())
}

/// Persists the Korean font toggle and, when turned on mid-game, starts the
/// patcher now instead of waiting for the watcher's next poll. Turning it off
/// stops the patcher immediately.
#[tauri::command]
pub(crate) fn set_korean_font(
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
