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

#[tauri::command]
pub(crate) async fn restart_plan() -> Result<RestartPlan, String> {
    let (client, game) = tauri::async_runtime::spawn_blocking(|| {
        (crate::riot::client_running(), crate::riot::ensure_no_game().is_err())
    })
    .await
    .map_err(|e| e.to_string())?;
    // The phase read only matters while a client runs.
    let phase = if client && !game { crate::runes::current_status().await.phase } else { String::new() };
    Ok(plan(client, game, &phase))
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
}
