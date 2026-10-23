mod client_health;
mod client_restart;
mod commands;
mod doctor;
mod flyout;
mod full_app;
mod hotkey;
mod identity;
mod korean_font;
mod lcu;
mod lifecycle;
mod live_game;
#[path = "full_app/match_detail.rs"]
mod match_detail;
mod notify;
mod overlay;
mod remote;
mod repair;
mod riot;
mod riot_client;
mod riot_icon;
mod runes;
mod shortcuts;
mod state;
mod updater;
mod vault;
pub mod windows;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{path::BaseDirectory, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

pub(crate) use commands::accounts::start_switch;
use flyout::{create_flyout, hide_main, toggle_flyout};
pub(crate) use flyout::{set_flyout_memory, show_flyout};
pub(crate) use state::AppState;
pub use state::{SwitchGuard, SwitchLease};

const TRAY_ID: &str = "swapper-tray";
const DEFAULT_TRAY_TOOLTIP: &str = "Swapper · Riot account switcher";
const RECONNECT_TOOLTIP_SUFFIX: &str = " · Phone needs to reconnect";
const AUTOSTART_ARG: &str = "--autostart";

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
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // With the deep-link feature the second instance's arguments were
            // already forwarded to the deep-link plugin, whose handler (see
            // shortcuts::init) performs the switch. A second plain launch
            // behaves like a tray click instead of starting a second Swapper.
            let has_link = args.iter().any(|arg| arg.starts_with("swapper://"));
            let is_autostart = args.iter().any(|arg| arg == AUTOSTART_ARG);
            if !has_link && !is_autostart {
                if full_app::enabled(app) {
                    let _ = full_app::open(app, None);
                } else {
                    show_flyout(app, "accounts");
                }
            }
        }))
        .setup(move |app| {
            let saved_hotkey = config.hotkey.clone();
            let bundled_deceive = app.path().resolve("Deceive.exe", BaseDirectory::Resource)?;
            let bundled_korean_font = app
                .path()
                .resolve("korean-font.fantome", BaseDirectory::Resource)?;
            let remote = remote::RemoteCore::new(app.handle().clone(), config.remote_enabled);
            let items_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut imported = runes::subscribe_items_imported();
                loop {
                    match imported.recv().await {
                        Ok(result) => {
                            let _ = items_app.emit("items_imported", result);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            app.manage(AppState {
                config: Mutex::new(config),
                bundled_deceive,
                bundled_korean_font,
                switch_guard: SwitchGuard::new(),
                remote: remote.clone(),
                hotkey_active: AtomicBool::new(false),
            });
            overlay::init(app.handle())?;
            let auto_enable = remote.clone();
            std::thread::spawn(move || {
                if auto_enable.status().enabled {
                    auto_enable.set_enabled(true);
                }
            });
            live_game::init(app.handle());
            runes::spawn_watch(app.handle().clone());
            client_health::spawn(app.handle().clone());
            app.handle().plugin(tauri_plugin_positioner::init())?;
            app.handle().plugin(tauri_plugin_notification::init())?;
            app.handle().plugin(tauri_plugin_dialog::init())?;
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
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
                state
                    .hotkey_active
                    .store(hotkey_registered, Ordering::SeqCst);
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
            let restart = MenuItem::with_id(app, "restart", "Restart client", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&add, &edit, &remove, &settings, &restart, &exit])?;
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
                    "restart" => client_restart::tray_restart(app),
                    full_app::LABEL => {
                        let _ = full_app::open(app, None);
                    }
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
            app.manage(full_app::Runtime::new(menu.clone()));
            full_app::sync_menu(app.handle())?;
            // Windows starts Swapper through the autostart entry with --autostart.
            // Startup never shows a window, so a login launch leaves the tray
            // icon as the only entry point. The flyout is created hidden on the
            // low memory target, and only in plain flyout mode: Fullsized App
            // and low memory mode create their window on first open.
            if full_app::window_settings(app.handle().clone()).create_flyout_at_startup() {
                create_flyout(app.handle(), None)?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == full_app::LABEL && matches!(event, WindowEvent::Destroyed) {
                full_app::cancel(window.app_handle());
            }
            // A minimized full app reports `document.hidden` (Live Game stops
            // reading the Live Client) and drops to WebView2's low memory target.
            if window.label() == full_app::LABEL && matches!(event, WindowEvent::Resized(_)) {
                if let Some(webview) = window.app_handle().get_webview_window(full_app::LABEL) {
                    set_flyout_memory(&webview, webview.is_minimized().unwrap_or(false));
                }
            }
            // Closing the full app hides it, so the tray reopens it instantly; low memory
            // mode still releases the webview. Hidden, it sits on the low memory target.
            if window.label() == full_app::LABEL && !full_app::low_memory(window.app_handle()) {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                    if let Some(webview) = window.app_handle().get_webview_window(full_app::LABEL) {
                        set_flyout_memory(&webview, true);
                    }
                }
                return;
            }
            if window.label() != full_app::FLYOUT {
                return;
            }
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
        .invoke_handler(|invoke| {
            if !full_app::routing::window_may_invoke(invoke.message.webview_ref().label()) {
                invoke
                    .resolver
                    .reject("This window cannot call Swapper commands.");
                return true;
            }
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
                full_app::window_settings,
                full_app::set_full_app_enabled,
                full_app::set_low_memory_mode,
                live_game::live_game_view,
                live_game::live_game_live,
                live_game::live_game_active,
                live_game::live_game_auto_open,
                live_game::set_live_game_auto_open,
                full_app::open_full_app,
                full_app::window_ready,
                full_app::dashboard_snapshot,
                match_detail::full_app_match_detail,
                full_app::search::full_app_search_players,
                full_app::search::full_app_player_profile,
                full_app::bench::dashboard_benchmarks,
                full_app::art::champion_art,
                full_app::art::ambient_background,
                full_app::ui_data::full_app_rune_catalog,
                full_app::ui_data::full_app_draft,
                full_app::champ_select::full_app_hover_champion,
                full_app::champ_select::full_app_lock_in,
                full_app::ui_data::full_app_abilities,
                full_app::abilities::champion_abilities,
                full_app::abilities::ability_icon,
                full_app::ui_data::full_app_matchups,
                full_app::ui_data::full_app_champion_pool,
                overlay::get_overlay_settings,
                overlay::set_overlay_settings,
                commands::settings::get_state,
                commands::accounts::open_riot,
                commands::accounts::riot_client_icon,
                commands::accounts::begin_add,
                commands::accounts::detect_account_identity,
                commands::accounts::complete_add,
                commands::accounts::begin_repair,
                commands::accounts::complete_repair,
                commands::accounts::switch_account,
                shortcuts::create_account_shortcut,
                commands::accounts::set_nickname,
                commands::accounts::remove_account,
                commands::settings::save_settings,
                flyout::hide_flyout,
                commands::settings::set_hotkey,
                commands::remote::open_windows_network_settings,
                commands::remote::set_remote_enabled,
                commands::remote::probe_remote,
                commands::remote::create_lan_pairing_url,
                commands::remote::reset_lan_access,
                commands::remote::list_paired_lan_devices,
                commands::remote::revoke_lan_device,
                commands::remote::rename_lan_device,
                commands::remote::dismiss_lan_reconnect,
                commands::remote::remove_stale_lan_devices,
                commands::runes::set_auto_apply_top_preset,
                commands::runes::set_rune_tier,
                commands::runes::set_apply_spells_with_runes,
                commands::runes::set_import_items_with_runes,
                commands::settings::set_notifications_enabled,
                commands::remote::allow_lan_firewall,
                commands::settings::set_ready_check_notifications,
                client_restart::restart_plan,
                client_health::client_health,
                commands::settings::set_korean_font,
                commands::settings::onboarding_pending,
                commands::settings::set_onboarded,
                korean_font::install_ltk_manager,
                korean_font::open_ltk_website,
                commands::runes::get_runes,
                commands::runes::champ_select_status,
                commands::runes::rune_icon,
                commands::runes::role_icon,
                commands::runes::spell_icon,
                commands::runes::item_icon,
                commands::runes::rank_icon,
                commands::runes::objective_icon,
                commands::runes::team_icon,
                commands::runes::apply_spell,
                commands::runes::get_pro_builds,
                commands::runes::get_keystone_build,
                commands::runes::get_matchup,
                commands::runes::get_champion_counters,
                commands::runes::get_champion_overview,
                commands::runes::get_current_patch,
                commands::runes::get_tier_list,
                commands::runes::get_duo_list,
                runes::blind::recommended_picks,
                commands::runes::get_champion_list,
                commands::runes::champion_icon,
                commands::runes::import_item_build,
                commands::runes::apply_rune_page,
                updater::update_status,
                updater::update_check_now,
                updater::update_install,
                doctor::run_doctor,
                doctor::run_doctor_check,
                doctor::doctor_report
            ];
            handler(invoke)
        })
        .build(tauri::generate_context!())
        .expect("Could not start Swapper")
        .run(|_app, event| match event {
            // Closing the last window (Fullsized App, or the flyout in low
            // memory mode) leaves Swapper in the tray; only exit_app quits.
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            // Never leave the Korean font patcher running once Swapper exits.
            tauri::RunEvent::Exit => korean_font::stop(),
            _ => {}
        });
}
