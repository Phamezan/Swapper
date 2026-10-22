use crate::{full_app, live_game};
use tauri::{Emitter, Manager};
use tauri_plugin_positioner::{Position, WindowExt};

#[tauri::command]
pub(crate) fn hide_flyout(app: tauri::AppHandle) -> Result<(), String> {
    hide_main(
        &app.get_webview_window(full_app::FLYOUT)
            .ok_or("Flyout is unavailable")?,
    )
}

/// Windows fades popup windows in and out; the flyout should appear and vanish
/// at once.
fn disable_window_transitions(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED,
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let disabled: i32 = 1;
    // SAFETY: a live window handle and a BOOL-sized attribute value.
    unsafe {
        DwmSetWindowAttribute(
            hwnd.0 as _,
            DWMWA_TRANSITIONS_FORCEDISABLED as _,
            (&disabled as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
    }
}

/// Tells WebView2 how much memory the flyout may keep. While it sits hidden in
/// the tray, Low lets WebView2 release most of its caches; Normal comes back
/// before it is shown again. Hiding the window alone leaves the page
/// "visible", so the controller is hidden too: the page sees
/// `document.hidden` and its timers and frames are throttled.
pub(crate) fn set_flyout_memory(window: &tauri::WebviewWindow, low: bool) {
    let _ = window.with_webview(move |webview| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        };
        use windows_core::Interface;
        let _ = webview.controller().SetIsVisible(!low);
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
/// low memory target, or in low memory mode releases its webview entirely.
pub(crate) fn hide_main(window: &tauri::WebviewWindow) -> Result<(), String> {
    if full_app::low_memory(window.app_handle()) {
        return window.destroy().map_err(|e| e.to_string());
    }
    let result = window.hide().map_err(|e| e.to_string());
    set_flyout_memory(window, true);
    result
}

/// Creates the hidden flyout from its `tauri.conf.json` entry (`create:
/// false`). It is shown at `destination` once its page reports ready.
pub(crate) fn create_flyout(
    app: &tauri::AppHandle,
    destination: Option<&str>,
) -> Result<(), String> {
    let config = full_app::flyout_config(app).ok_or("Flyout window config is missing")?;
    full_app::expect_flyout(app, destination);
    let window = tauri::WebviewWindowBuilder::from_config(app, &config)
        .and_then(|builder| builder.build())
        .map_err(|e| e.to_string())?;
    disable_window_transitions(&window);
    set_flyout_memory(&window, true);
    Ok(())
}

/// Every entry point (tray, hotkey, menu, second launch) comes through here.
/// Fullsized App mode opens that window instead; a missing flyout (low memory
/// mode) is created and shown once its page is ready.
pub(crate) fn show_flyout(app: &tauri::AppHandle, destination: &str) {
    if full_app::enabled(app) {
        let _ = full_app::open(app, Some(destination));
        return;
    }
    if full_app::defer_flyout(app, destination) {
        return;
    }
    let Some(window) = app.get_webview_window(full_app::FLYOUT) else {
        let _ = create_flyout(app, Some(destination));
        return;
    };
    let _ = window.as_ref().window().move_window(Position::TrayCenter);
    let _ = app.emit_to(full_app::FLYOUT, "navigate", destination);
    set_flyout_memory(&window, false);
    let _ = window.show();
    let _ = window.set_focus();
}

// The flyout stays open until the tray icon or the close button hides it, so a
// tray left-click toggles by visibility rather than focus (clicking the tray
// moves focus away before the event arrives). Only the Up event toggles once.
// The Fullsized App is only focused, keeping whatever page it shows.
pub(crate) fn toggle_flyout(app: &tauri::AppHandle) {
    if full_app::enabled(app) {
        // During a game the tray opens the full app on Live Game.
        let _ = full_app::open(app, live_game::is_active(app).then_some("live"));
        return;
    }
    let visible = app
        .get_webview_window(full_app::FLYOUT)
        .filter(|window| window.is_visible().unwrap_or(false));
    if let Some(window) = visible {
        let _ = hide_main(&window);
    } else {
        show_flyout(app, "accounts");
    }
}
