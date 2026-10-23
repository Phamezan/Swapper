//! Gameflow lifecycle notifications and opportunistic Riot session refresh.
//! Gameflow transitions notify once; session refresh tracks the client run.

use tauri::AppHandle;

use crate::notify;

/// Refresh once after a matching login per service PID, and once on its exit.
/// API/lockfile outages do not reset the run; process inspection owns that edge.
#[derive(Default)]
pub(crate) struct SessionRefresh {
    run: Option<u32>,
    refreshed: Option<uuid::Uuid>,
    signed_in: Option<uuid::Uuid>,
}

impl SessionRefresh {
    fn observe(
        &mut self,
        config: &mut crate::vault::Config,
        detection: &crate::identity::Detection,
        run: Option<u32>,
    ) -> Result<(), String> {
        use crate::{identity::Detection, riot};
        if self.run != run {
            let refresh_exit =
                run.is_none() && self.signed_in.is_some() && self.signed_in == config.active_id;
            self.run = run;
            self.refreshed = None;
            self.signed_in = None;
            if refresh_exit {
                riot::refresh_active(config, &Detection::NotRunning)?;
            }
        }
        if run.is_none() {
            return Ok(());
        }
        match detection {
            Detection::Identified(identity) => {
                let matching = config
                    .active_id
                    .and_then(|id| config.accounts.iter().find(|a| a.id == id))
                    .filter(|account| riot::identity_safe_for(account, identity));
                self.signed_in = matching.map(|account| account.id);
                if self.signed_in.is_some()
                    && self.refreshed != self.signed_in
                    && riot::refresh_active(config, detection)?
                {
                    self.refreshed = self.signed_in;
                }
            }
            Detection::NotReady => self.signed_in = None,
            Detection::Unavailable | Detection::NotRunning => {}
        }
        Ok(())
    }
}

/// Runs independently of the gameflow watcher so an unresponsive Riot API
/// cannot delay ready-check notifications or champion-select actions.
pub(crate) fn spawn_session_refresh(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut observer = SessionRefresh::default();
        loop {
            refresh_session(&app, &mut observer).await;
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
}

/// A stale asynchronous probe must never
/// write a snapshot after an account action has replaced the live session.
async fn refresh_session(app: &AppHandle, observer: &mut SessionRefresh) {
    use tauri::Manager;
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    if state.is_switching() {
        return;
    }
    let epoch = state.switch_guard.epoch();
    let Ok(Ok(processes)) = tauri::async_runtime::spawn_blocking(|| {
        crate::windows::process::matching(&["RiotClientServices.exe"])
    })
    .await
    else {
        return;
    };
    let run = processes.first().map(|process| process.pid);
    let detection = crate::identity::detect_account().await;
    let Ok(mut config) = state.config.lock() else {
        return;
    };
    if state.is_switching() || state.switch_guard.epoch() != epoch {
        return;
    }
    // Opportunistic failures leave the saved vault intact. The switch path
    // retries synchronously and reports any error before clearing live files.
    let _ = observer.observe(&mut config, &detection, run);
}

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

/// Whether `event` brings the Fullsized App to the front. `transition` fires
/// once per champion select, so the window is raised once and never again if
/// the user switches away. The window's own `champ_select` handling routes it
/// to the Champion select page.
fn raises_full_app(event: LifecycleEvent, full_app_enabled: bool) -> bool {
    full_app_enabled && event == LifecycleEvent::ChampSelectStarted
}

/// Phases during which a game is running or wrapping up.
pub(crate) fn in_game(phase: &str) -> bool {
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
        tauri::async_runtime::spawn(crate::full_app::lp::record_after_game(app.clone()));
    }
    let Some(event) = transition(previous, current) else {
        return;
    };
    if raises_full_app(event, crate::full_app::enabled(app)) {
        let _ = crate::full_app::open(app, None);
    }
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
    fn session_refresh_saves_once_per_run_and_again_on_exit() {
        use crate::{
            identity::{DetectedIdentity, Detection},
            riot, vault,
        };
        vault::with_test_data(|root| {
            let (mut config, _) = riot::tests::session_fixture(root);
            let detection = Detection::Identified(DetectedIdentity {
                puuid: config.accounts[0].puuid.clone().unwrap(),
                game_name: "Example".into(),
                tag_line: "TAG".into(),
                platform: None,
                region: None,
                profile_icon_id: None,
                icon_data_url: None,
            });
            let mut observer = SessionRefresh::default();
            let before = config.accounts[0].vault_id;
            observer
                .observe(&mut config, &detection, Some(123))
                .unwrap();
            let first = config.accounts[0].vault_id;
            assert_ne!(first, before);
            observer
                .observe(&mut config, &detection, Some(123))
                .unwrap();
            observer
                .observe(&mut config, &Detection::Unavailable, Some(123))
                .unwrap();
            observer
                .observe(&mut config, &Detection::NotRunning, Some(123))
                .unwrap();
            observer
                .observe(&mut config, &detection, Some(123))
                .unwrap();
            assert_eq!(config.accounts[0].vault_id, first);
            observer
                .observe(&mut config, &Detection::NotRunning, None)
                .unwrap();
            let exit = config.accounts[0].vault_id;
            assert_ne!(exit, first);
            observer
                .observe(&mut config, &Detection::NotRunning, None)
                .unwrap();
            assert_eq!(config.accounts[0].vault_id, exit);
            observer
                .observe(&mut config, &detection, Some(456))
                .unwrap();
            assert_ne!(config.accounts[0].vault_id, exit);
        });
    }

    #[test]
    fn session_refresh_never_saves_an_unmatched_login_or_a_signed_out_exit() {
        use crate::{
            identity::{DetectedIdentity, Detection},
            riot, vault,
        };
        vault::with_test_data(|root| {
            let (mut config, _) = riot::tests::session_fixture(root);
            let before = config.accounts[0].vault_id;
            let mut detection = Detection::Identified(DetectedIdentity {
                puuid: "00000000-0000-0000-0000-000000000002".into(),
                game_name: "Example".into(),
                tag_line: "TAG".into(),
                platform: None,
                region: None,
                profile_icon_id: None,
                icon_data_url: None,
            });
            let mut observer = SessionRefresh::default();
            observer
                .observe(&mut config, &detection, Some(123))
                .unwrap();
            observer
                .observe(&mut config, &Detection::NotRunning, None)
                .unwrap();
            assert_eq!(config.accounts[0].vault_id, before);

            if let Detection::Identified(identity) = &mut detection {
                identity.puuid = config.accounts[0].puuid.clone().unwrap();
            }
            observer
                .observe(&mut config, &Detection::NotReady, Some(456))
                .unwrap();
            assert_eq!(config.accounts[0].vault_id, before);
            observer
                .observe(&mut config, &detection, Some(456))
                .unwrap();
            let signed_in = config.accounts[0].vault_id;
            assert_ne!(signed_in, before);
            observer
                .observe(&mut config, &Detection::NotReady, Some(456))
                .unwrap();
            observer
                .observe(&mut config, &Detection::NotRunning, None)
                .unwrap();
            assert_eq!(config.accounts[0].vault_id, signed_in);
        });
    }

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
    fn only_a_new_champ_select_raises_the_full_app() {
        let raised = |previous, current, on| {
            transition(previous, current).is_some_and(|event| raises_full_app(event, on))
        };
        assert!(raised(Some("ReadyCheck"), "ChampSelect", true));
        assert!(raised(None, "ChampSelect", true));
        assert!(!raised(Some("ReadyCheck"), "ChampSelect", false));
        // Polls inside the same champion select, other phases and ready checks stay quiet.
        assert!(!raised(Some("ChampSelect"), "ChampSelect", true));
        assert!(!raised(Some("Lobby"), "ReadyCheck", true));
        assert!(!raised(Some("ChampSelect"), "InProgress", true));
        // A dodge followed by a new champion select raises again.
        assert!(raised(Some("None"), "ChampSelect", true));
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
