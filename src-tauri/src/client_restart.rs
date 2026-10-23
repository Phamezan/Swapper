//! Whether a launch setting (Deceive) may restart the Riot client right away.
//! The restart itself is the normal account switch onto the signed-in account.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RestartPlan {
    /// No client running: the saved setting applies on the next launch.
    Save,
    Restart,
    /// Champion select or ready check: restarting costs a dodge, so ask first.
    Ask,
    /// A game is running or wrapping up: never restart, apply afterwards.
    Defer,
}

pub fn plan(client_running: bool, game_running: bool, phase: &str) -> RestartPlan {
    if game_running || crate::lifecycle::in_game(phase) {
        RestartPlan::Defer
    } else if !client_running {
        RestartPlan::Save
    } else if matches!(phase, "ChampSelect" | "ReadyCheck") {
        RestartPlan::Ask
    } else {
        RestartPlan::Restart
    }
}

async fn current_plan() -> Result<RestartPlan, String> {
    let (client, game) = tauri::async_runtime::spawn_blocking(|| {
        (crate::riot::client_running(), crate::riot::ensure_no_game().is_err())
    })
    .await
    .map_err(|e| e.to_string())?;
    // The phase read only matters while a client runs.
    let phase = if client && !game { crate::runes::current_status().await.phase } else { String::new() };
    Ok(plan(client, game, &phase))
}

#[tauri::command]
pub(crate) async fn restart_plan() -> Result<RestartPlan, String> {
    current_plan().await
}

/// What the tray's "Restart client" does. The tray cannot show the champion
/// select confirm, so it only restarts when that is plainly safe and otherwise
/// says why it did not.
#[derive(Debug, PartialEq, Eq)]
enum TrayRestart {
    Restart,
    Skip(&'static str),
}

fn tray_action(plan: RestartPlan, has_active_account: bool) -> TrayRestart {
    match plan {
        RestartPlan::Defer => TrayRestart::Skip("Can't restart during a game."),
        RestartPlan::Ask => {
            TrayRestart::Skip("You're in champion select. Restart from Swapper to confirm.")
        }
        RestartPlan::Save => TrayRestart::Skip("No Riot client is running."),
        RestartPlan::Restart if !has_active_account => {
            TrayRestart::Skip("Save the signed-in account in Swapper first.")
        }
        RestartPlan::Restart => TrayRestart::Restart,
    }
}

/// Tray menu entry: the normal switch onto the signed-in account, behind the same plan.
pub(crate) fn tray_restart(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;
        let plan = current_plan().await.unwrap_or(RestartPlan::Defer);
        let Some(state) = app.try_state::<crate::AppState>() else {
            return;
        };
        let active = state.config.lock().ok().and_then(|config| config.active_id);
        match (tray_action(plan, active.is_some()), active) {
            (TrayRestart::Restart, Some(id)) => {
                if let Err(reason) = crate::commands::accounts::start_switch(&app, &state, id) {
                    crate::notify::restart_skipped(&app, &reason);
                }
            }
            (TrayRestart::Skip(reason), _) => crate::notify::restart_skipped(&app, reason),
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarts_only_an_idle_running_client() {
        assert_eq!(plan(true, false, "None"), RestartPlan::Restart);
        assert_eq!(plan(true, false, "Lobby"), RestartPlan::Restart);
        assert_eq!(plan(true, false, "Unavailable"), RestartPlan::Restart);
        assert_eq!(plan(false, false, ""), RestartPlan::Save);
    }

    #[test]
    fn asks_before_costing_a_dodge() {
        assert_eq!(plan(true, false, "ChampSelect"), RestartPlan::Ask);
        assert_eq!(plan(true, false, "ReadyCheck"), RestartPlan::Ask);
    }

    #[test]
    fn never_restarts_during_or_around_a_game() {
        for phase in ["InProgress", "Reconnect", "WaitingForStats", "PreEndOfGame"] {
            assert_eq!(plan(true, false, phase), RestartPlan::Defer);
        }
        assert_eq!(plan(true, true, "None"), RestartPlan::Defer);
        assert_eq!(plan(false, true, ""), RestartPlan::Defer);
    }

    #[test]
    fn tray_restarts_only_when_plainly_safe() {
        assert_eq!(tray_action(RestartPlan::Restart, true), TrayRestart::Restart);
        for unsafe_plan in [RestartPlan::Defer, RestartPlan::Ask, RestartPlan::Save] {
            assert!(matches!(tray_action(unsafe_plan, true), TrayRestart::Skip(_)));
        }
        assert!(matches!(tray_action(RestartPlan::Restart, false), TrayRestart::Skip(_)));
    }
}
