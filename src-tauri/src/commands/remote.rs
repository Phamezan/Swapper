use crate::state::{view, AppView};
use crate::{remote, vault, AppState};
use std::sync::atomic::Ordering;
use tauri::State;

#[tauri::command]
pub(crate) fn set_remote_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<remote::RemoteStatus, String> {
    {
        let mut config = state.config.lock().map_err(|e| e.to_string())?;
        config.remote_enabled = enabled;
        vault::save(&config)?;
    }
    state.remote.clone().set_enabled(enabled);
    Ok(state.remote.status())
}

#[tauri::command]
pub(crate) fn probe_remote(state: State<'_, AppState>) -> remote::RemoteStatus {
    state.remote.clone().probe()
}

#[tauri::command]
pub(crate) fn create_lan_pairing_url(state: State<'_, AppState>) -> Result<String, String> {
    state.remote.create_lan_pairing_url()
}

#[tauri::command]
pub(crate) fn reset_lan_access(state: State<'_, AppState>) -> Result<String, String> {
    state.remote.reset_lan_access()?;
    state.remote.create_lan_pairing_url()
}

#[tauri::command]
pub(crate) fn list_paired_lan_devices(state: State<'_, AppState>) -> Vec<remote::PairedDeviceView> {
    state.remote.paired_devices()
}

#[tauri::command]
pub(crate) fn revoke_lan_device(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.remote.revoke_paired_device(&id)
}

#[tauri::command]
pub(crate) fn rename_lan_device(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<String, String> {
    state.remote.rename_paired_device(&id, &name)
}

#[tauri::command]
pub(crate) fn dismiss_lan_reconnect(state: State<'_, AppState>) {
    state.remote.dismiss_lan_reconnect();
}

#[tauri::command]
pub(crate) fn remove_stale_lan_devices(state: State<'_, AppState>) -> Result<usize, String> {
    state.remote.remove_stale_lan_devices()
}

/// Allows Swapper through Windows Firewall on Private networks. Windows shows
/// one elevation prompt for Swapper; nothing is asked without this click.
#[tauri::command]
pub(crate) async fn allow_lan_firewall(state: State<'_, AppState>) -> Result<AppView, String> {
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
pub(crate) fn open_windows_network_settings() -> Result<(), String> {
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
