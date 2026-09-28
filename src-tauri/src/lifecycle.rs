//! Gameflow lifecycle notifications, driven by the phase the champion-select
//! watcher already polls. A phase transition fires at most one notification:
//! one per ready check and one per champion select session, never one per
//! poll.

use tauri::AppHandle;

use crate::notify;

/// The lifecycle moments Swapper notifies about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    ReadyCheckStarted,
    ChampSelectStarted,
}

/// The notification a move into `current` calls for. Staying in a phase is the
/// same event continuing, so only the entry notifies; every other phase is a
/// lifecycle the user does not need a popup for.
fn transition(previous: Option<&str>, current: &str) -> Option<LifecycleEvent> {
    if previous == Some(current) {
        return None;
    }
    match current {
        "ReadyCheck" => Some(LifecycleEvent::ReadyCheckStarted),
        "ChampSelect" => Some(LifecycleEvent::ChampSelectStarted),
        _ => None,
    }
}

/// Whether `event` should notify under the current settings. Champion select
/// follows the master toggle; ready checks can be turned off independently.
fn should_notify(event: LifecycleEvent, enabled: bool, ready_check_enabled: bool) -> bool {
    enabled && (event == LifecycleEvent::ChampSelectStarted || ready_check_enabled)
}

/// Phases during which a game is running or wrapping up.
fn in_game(phase: &str) -> bool {
    matches!(
        phase,
        "InProgress" | "Reconnect" | "WaitingForStats" | "PreEndOfGame"
    )
}

/// Whether the move from `previous` to `current` means a game just finished.
fn game_ended(previous: Option<&str>, current: &str) -> bool {
    previous.is_some_and(in_game) && !in_game(current)
}

/// Observes one polled phase and fires the notification its transition calls
/// for. A failed read leaves the previous phase in place (the caller keeps its
/// state), so a reconnecting League client cannot re-fire an old transition.
pub fn observe(app: &AppHandle, previous: Option<&str>, current: &str) {
    if game_ended(previous, current) {
        // Imported item sets are only useful for the game they were made for;
        // clearing them keeps the shop from filling up with old builds.
        tauri::async_runtime::spawn(async {
            let _ = crate::runes::item_sets::remove_imported_sets().await;
        });
    }
    let Some(event) = transition(previous, current) else {
        return;
    };
    use tauri::Manager;
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    if should_notify(
        event,
        state.notifications_enabled(),
        state.ready_check_notifications(),
    ) {
        match event {
            LifecycleEvent::ReadyCheckStarted => notify::ready_check_started(app),
            LifecycleEvent::ChampSelectStarted => notify::champ_select_started(app),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaving_a_game_counts_as_the_game_ending() {
        assert!(game_ended(Some("InProgress"), "EndOfGame"));
        assert!(game_ended(Some("WaitingForStats"), "Lobby"));
        assert!(game_ended(Some("PreEndOfGame"), "None"));
        // Still in the game, or never in one.
        assert!(!game_ended(Some("InProgress"), "InProgress"));
        assert!(!game_ended(Some("InProgress"), "Reconnect"));
        assert!(!game_ended(Some("ChampSelect"), "InProgress"));
        assert!(!game_ended(None, "EndOfGame"));
    }

    #[test]
    fn a_new_ready_check_notifies_once() {
        assert_eq!(
            transition(None, "ReadyCheck"),
            Some(LifecycleEvent::ReadyCheckStarted)
        );
        assert_eq!(
            transition(Some("Matchmaking"), "ReadyCheck"),
            Some(LifecycleEvent::ReadyCheckStarted)
        );
        // Polls within the same ready check are the same event.
        assert_eq!(transition(Some("ReadyCheck"), "ReadyCheck"), None);
    }

    #[test]
    fn a_ready_check_after_a_dodge_notifies_again() {
        assert_eq!(transition(Some("ReadyCheck"), "None"), None);
        assert_eq!(
            transition(Some("None"), "ReadyCheck"),
            Some(LifecycleEvent::ReadyCheckStarted)
        );
    }

    #[test]
    fn champ_select_notifies_on_entry_and_not_per_poll() {
        assert_eq!(
            transition(Some("ReadyCheck"), "ChampSelect"),
            Some(LifecycleEvent::ChampSelectStarted)
        );
        assert_eq!(transition(None, "ChampSelect"), Some(LifecycleEvent::ChampSelectStarted));
        assert_eq!(transition(Some("ChampSelect"), "ChampSelect"), None);
    }

    #[test]
    fn other_phase_moves_never_notify() {
        assert_eq!(transition(None, "Lobby"), None);
        assert_eq!(transition(Some("None"), "Matchmaking"), None);
        assert_eq!(transition(Some("ChampSelect"), "GameInProgress"), None);
        assert_eq!(transition(Some("GameInProgress"), "None"), None);
    }

    #[test]
    fn ready_checks_can_be_disabled_independently() {
        assert!(should_notify(
            LifecycleEvent::ChampSelectStarted,
            true,
            false
        ));
        assert!(should_notify(LifecycleEvent::ReadyCheckStarted, true, true));
        assert!(!should_notify(
            LifecycleEvent::ReadyCheckStarted,
            true,
            false
        ));
        // The master toggle silences every lifecycle notification.
        assert!(!should_notify(
            LifecycleEvent::ChampSelectStarted,
            false,
            true
        ));
        assert!(!should_notify(
            LifecycleEvent::ReadyCheckStarted,
            false,
            false
        ));
    }
}
