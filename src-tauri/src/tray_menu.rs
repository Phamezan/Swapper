//! The tray icon's right-click menu: a small borderless Swapper window instead
//! of the native menu, which Windows 11 opens partly behind the taskbar.
use crate::{client_restart, exit_app, full_app, set_flyout_memory, show_flyout};
use tauri::{Emitter, Manager};

pub const LABEL: &str = "tray_menu";
/// Logical size of the visible menu; the window adds `SHADOW` on every side
/// so the CSS drop shadow is not clipped.
const MENU_WIDTH: i32 = 208;
const ROW: i32 = 32;
const SEPARATOR: i32 = 9;
const PADDING: i32 = 10; // 4px padding x2 plus the 1px border x2
const SHADOW: i32 = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where a `w` x `h` menu goes for a click at `(cx, cy)`, all physical pixels.
/// It opens upward when it would not fit below the cursor (taskbar at the
/// bottom), leftward when it would not fit to the right, and is finally
/// clamped inside `work` (the monitor minus the taskbar).
pub fn place(cx: i32, cy: i32, w: i32, h: i32, work: Rect) -> (i32, i32) {
    let x = if cx + w > work.x + work.w { cx - w } else { cx };
    let y = if cy + h > work.y + work.h { cy - h } else { cy };
    (
        x.min(work.x + work.w - w).max(work.x),
        y.min(work.y + work.h - h).max(work.y),
    )
}

fn rows(full_app_enabled: bool) -> i32 {
    if full_app_enabled { 7 } else { 6 }
}

/// Logical window height for the menu with or without "Open Swapper".
fn window_height(full_app_enabled: bool) -> i32 {
    rows(full_app_enabled) * ROW + SEPARATOR + PADDING + SHADOW * 2
}

fn create(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    let mut builder = tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("tray-menu.html".into()))
        .title("Swapper menu")
        .inner_size((MENU_WIDTH + SHADOW * 2) as f64, window_height(false) as f64)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .focused(true);
    // WebView2 refuses a second environment with different browser args (0x8007139F); match the flyout's.
    if let Some(args) = full_app::flyout_config(app).and_then(|c| c.additional_browser_args) {
        builder = builder.additional_browser_args(&args);
    }
    let window = builder
        .build()
        .map_err(|e| { eprintln!("Tray menu create failed: {e}"); "Swapper couldn't open its menu.".to_string() })?;
    set_flyout_memory(&window, true);
    Ok(window)
}

/// Opens the menu at a physical screen position (the tray click).
pub fn show(app: &tauri::AppHandle, cursor: (f64, f64)) {
    let Some(window) = app.get_webview_window(LABEL).or_else(|| create(app).ok()) else {
        return;
    };
    let (cx, cy) = (cursor.0 as i32, cursor.1 as i32);
    let monitor = app
        .monitor_from_point(cursor.0, cursor.1)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let full = full_app::enabled(app);
    let phys = |logical: i32| (logical as f64 * scale).round() as i32;
    let (w, h) = (phys(MENU_WIDTH + SHADOW * 2), phys(window_height(full)));
    let area = monitor.work_area();
    let work = Rect { x: area.position.x, y: area.position.y, w: area.size.width as i32, h: area.size.height as i32 };
    let (x, y) = place(cx, cy, w, h, work);
    // Move first so the window is on the target monitor before it is sized in
    // that monitor's pixels.
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    let _ = window.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    set_flyout_memory(&window, false);
    let _ = app.emit_to(LABEL, "tray-menu-show", full);
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
        set_flyout_memory(&window, true);
    }
}

#[tauri::command]
pub fn tray_menu_state(app: tauri::AppHandle) -> bool {
    full_app::enabled(&app)
}

#[tauri::command]
pub fn tray_menu_hide(app: tauri::AppHandle) {
    hide(&app);
}

#[tauri::command]
pub fn tray_menu_action(app: tauri::AppHandle, id: String) {
    hide(&app);
    match id.as_str() {
        "add" | "edit" | "remove" | "settings" => show_flyout(&app, &id),
        "restart" => client_restart::tray_restart(&app),
        "open" => {
            let _ = full_app::open(&app, None);
        }
        "exit" => exit_app(&app),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 }; // 1080p, 40px bottom taskbar

    #[test]
    fn opens_upward_from_a_click_on_a_bottom_taskbar() {
        // Tray icon at the right end of the taskbar (cursor y inside the taskbar).
        assert_eq!(place(1880, 1060, 232, 300, WORK), (1648, 740));
    }

    #[test]
    fn opens_downward_and_right_when_there_is_room() {
        assert_eq!(place(100, 100, 232, 300, WORK), (100, 100));
    }

    #[test]
    fn a_top_taskbar_keeps_the_menu_below_the_work_area_top() {
        let work = Rect { x: 0, y: 40, w: 1920, h: 1040 };
        assert_eq!(place(1880, 10, 232, 300, work), (1648, 40));
    }

    #[test]
    fn a_scaled_secondary_monitor_with_negative_origin_is_clamped_inside() {
        // 150% monitor left of the primary: physical work area, taskbar 60px.
        let work = Rect { x: -2560, y: 0, w: 2560, h: 1380 };
        let (w, h) = (348, 450);
        assert_eq!(place(-5, 1400, w, h, work), (-5 - w, 930));
        assert_eq!(place(-2600, 20, w, h, work), (-2560, 20));
    }

    #[test]
    fn a_menu_larger_than_the_work_area_pins_to_its_top_left() {
        let small = Rect { x: 0, y: 0, w: 100, h: 100 };
        assert_eq!(place(50, 50, 232, 300, small), (0, 0));
    }

    #[test]
    fn window_height_adds_a_row_for_open_swapper() {
        assert_eq!(window_height(true) - window_height(false), ROW);
    }
}
