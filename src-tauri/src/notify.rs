use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::riot::{SwitchFailure, SwitchFailureKind};

pub fn switch_started(app: &AppHandle, account: &str) {
    let _ = app
        .notification()
        .builder()
        .title("Switching account")
        .body(format!("Switching to {account}…"))
        .show();
}

pub fn switch_succeeded(app: &AppHandle, account: &str, use_deceive: bool) {
    let title = if use_deceive {
        "Launching through Deceive"
    } else {
        "Account switched"
    };
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(format!("{account} is ready."))
        .show();
}

pub fn switch_failed(app: &AppHandle, account: &str, use_deceive: bool, failure: &SwitchFailure) {
    let body = human_reason(failure, account, use_deceive)
        .unwrap_or_else(|| format!("Couldn't switch to {account}. Try again."));
    let _ = app
        .notification()
        .builder()
        .title("Switch failed")
        .body(body)
        .show();
}

pub fn update_available(app: &AppHandle, version: &str) {
    let _ = app
        .notification()
        .builder()
        .title("Swapper update available")
        .body(format!("Version {version} is ready. Open Swapper to install it."))
        .show();
}

pub fn restart_skipped(app: &AppHandle, reason: &str) {
    let _ = app
        .notification()
        .builder()
        .title("Restart client")
        .body(reason)
        .show();
}

// Both lifecycle notifications use the notification's default sound.
pub fn ready_check_started(app: &AppHandle) {
    let _ = app
        .notification()
        .builder()
        .title("Match found")
        .body("A ready check started. Accept it in the League client.")
        .show();
}

pub fn champ_select_started(app: &AppHandle) {
    let _ = app
        .notification()
        .builder()
        .title("Champion select")
        .body("Champion select has begun.")
        .show();
}

/// Fired once per LAN address change while a phone is paired. The notification
/// plugin has no cheap click action on Windows, so there is no click handler.
pub fn lan_reconnect_needed(app: &AppHandle) {
    let _ = app
        .notification()
        .builder()
        .title("Phone needs to reconnect")
        .body("Your PC address changed. Scan the new QR code in Swapper.")
        .show();
}

pub(crate) fn human_reason(failure: &SwitchFailure, account: &str, use_deceive: bool) -> Option<String> {
    let text = match failure.kind {
        // The process check wrote this one already, naming the game when it could.
        SwitchFailureKind::GameRunning => failure.detail.clone(),
        SwitchFailureKind::ClientBusy => "Riot Client is still open. Close it fully, then try again.".to_string(),
        SwitchFailureKind::InspectionFailed => "Couldn't check for running games. Nothing changed \u{2014} try again.".to_string(),
        SwitchFailureKind::AccountMissing => "That account was removed. Add it again to switch.".to_string(),
        SwitchFailureKind::LauncherMissing if use_deceive => "Deceive is missing. Reinstall Swapper.".to_string(),
        SwitchFailureKind::LauncherMissing => "Riot Client wasn't found. Set its path in Settings.".to_string(),
        SwitchFailureKind::SessionRestore => format!("Couldn't restore {account}'s sign-in. Try Repair account."),
        SwitchFailureKind::Launch if use_deceive => "Deceive didn't start. Try switching again.".to_string(),
        SwitchFailureKind::Launch => "Riot Client didn't start. Try switching again.".to_string(),
        SwitchFailureKind::Other => return None,
    };
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(kind: SwitchFailureKind) -> SwitchFailure {
        SwitchFailure { kind, detail: "Close your game first, then switch accounts.".into() }
    }

    #[test]
    fn game_running_names_the_game_the_check_saw() {
        let valorant = SwitchFailure { kind: SwitchFailureKind::GameRunning, detail: "VALORANT is running. Close it first.".into() };
        assert_eq!(human_reason(&valorant, "Example#EUW", false).as_deref(), Some("VALORANT is running. Close it first."));
    }

    #[test]
    fn restore_failures_explain_the_saved_session() {
        assert_eq!(
            human_reason(&failure(SwitchFailureKind::SessionRestore), "Example#EUW", false),
            Some("Couldn't restore Example#EUW's sign-in. Try Repair account.".to_string())
        );
    }

    #[test]
    fn launch_failures_name_the_launcher() {
        assert_eq!(
            human_reason(&failure(SwitchFailureKind::Launch), "Example#EUW", false),
            Some("Riot Client didn't start. Try switching again.".to_string())
        );
        assert_eq!(
            human_reason(&failure(SwitchFailureKind::Launch), "Example#EUW", true),
            Some("Deceive didn't start. Try switching again.".to_string())
        );
    }

    #[test]
    fn inspection_failures_do_not_blame_a_game() {
        assert_eq!(
            human_reason(&failure(SwitchFailureKind::InspectionFailed), "Example#EUW", false),
            Some(
                "Couldn't check for running games. Nothing changed \u{2014} try again."
                    .to_string()
            )
        );
    }

    #[test]
    fn friendly_categories_are_explicit_and_raw_errors_stay_hidden() {
        assert_eq!(
            human_reason(&failure(SwitchFailureKind::GameRunning), "Example#EUW", false),
            Some("Close your game first, then switch accounts.".to_string())
        );
        assert_eq!(
            human_reason(&failure(SwitchFailureKind::LauncherMissing), "Example#EUW", true),
            Some("Deceive is missing. Reinstall Swapper.".to_string())
        );
        assert_eq!(human_reason(&failure(SwitchFailureKind::Other), "Example#EUW", false), None);
    }
}
