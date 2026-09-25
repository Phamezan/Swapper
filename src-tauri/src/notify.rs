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
    let body = human_reason(failure.kind, account, use_deceive)
        .unwrap_or_else(|| format!("Could not switch to {account}."));
    let _ = app
        .notification()
        .builder()
        .title("Switch failed")
        .body(body)
        .show();
}

fn human_reason(kind: SwitchFailureKind, account: &str, use_deceive: bool) -> Option<String> {
    let text = match kind {
        SwitchFailureKind::GameRunning => "Close the running Riot game before switching accounts.".to_string(),
        SwitchFailureKind::ClientBusy => "Riot Client is still running. Close it from the tray and try again.".to_string(),
        SwitchFailureKind::InspectionFailed => "Swapper could not verify which Riot processes are running. No account or session data was changed - try again.".to_string(),
        SwitchFailureKind::AccountMissing => "Account no longer exists.".to_string(),
        SwitchFailureKind::LauncherMissing if use_deceive => "Bundled Deceive.exe was not found. Reinstall Swapper.".to_string(),
        SwitchFailureKind::LauncherMissing => "Riot Client was not found. Set its path in Settings.".to_string(),
        SwitchFailureKind::SessionRestore => format!("The saved session for {account} could not be restored."),
        SwitchFailureKind::Launch if use_deceive => "Deceive could not be launched.".to_string(),
        SwitchFailureKind::Launch => "Riot Client could not be launched.".to_string(),
        SwitchFailureKind::Other => return None,
    };
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_failures_explain_the_saved_session() {
        assert_eq!(
            human_reason(SwitchFailureKind::SessionRestore, "Example#EUW", false),
            Some("The saved session for Example#EUW could not be restored.".to_string())
        );
    }

    #[test]
    fn launch_failures_name_the_launcher() {
        assert_eq!(
            human_reason(SwitchFailureKind::Launch, "Example#EUW", false),
            Some("Riot Client could not be launched.".to_string())
        );
        assert_eq!(
            human_reason(SwitchFailureKind::Launch, "Example#EUW", true),
            Some("Deceive could not be launched.".to_string())
        );
    }

    #[test]
    fn inspection_failures_do_not_blame_a_game() {
        assert_eq!(
            human_reason(SwitchFailureKind::InspectionFailed, "Example#EUW", false),
            Some(
                "Swapper could not verify which Riot processes are running. No account or session data was changed - try again."
                    .to_string()
            )
        );
    }

    #[test]
    fn friendly_categories_are_explicit_and_raw_errors_stay_hidden() {
        assert_eq!(
            human_reason(SwitchFailureKind::GameRunning, "Example#EUW", false),
            Some("Close the running Riot game before switching accounts.".to_string())
        );
        assert_eq!(
            human_reason(SwitchFailureKind::LauncherMissing, "Example#EUW", true),
            Some("Bundled Deceive.exe was not found. Reinstall Swapper.".to_string())
        );
        assert_eq!(human_reason(SwitchFailureKind::Other, "Example#EUW", false), None);
    }
}
