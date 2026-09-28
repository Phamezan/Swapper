//! Account session repair: when a switch fails because a saved session
//! cannot be restored, the user signs in to that account in Riot Client and
//! Swapper replaces the saved snapshot in place after verifying the PUUID.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use uuid::Uuid;

use crate::identity::{self, Detection, DetectedIdentity};
use crate::riot::{self, SwitchFailureKind};
use crate::vault::{self, Account, Config};

/// Whether a switch failure genuinely means the saved session is dead.
///
/// [`SwitchFailureKind::SessionRestore`] covers both a snapshot Swapper can
/// no longer read (missing file, corrupt data, or Windows refusing to unlock
/// it) and one it could not write back into the Riot Client data directory.
/// In both cases Swapper cannot put that account's session back and signing
/// in again is the only recovery. Everything else is a transient refusal
/// (game running, client busy, launcher missing, launch failure) that the
/// normal error message already explains.
pub(crate) fn is_repairable(kind: SwitchFailureKind) -> bool {
    matches!(kind, SwitchFailureKind::SessionRestore)
}

/// A legacy account saved before PUUIDs were recorded cannot prove which
/// Riot login created it, so its session must never be replaced blind.
pub(crate) fn ensure_repairable(account: &Account) -> Result<(), String> {
    if account.puuid.is_some() {
        return Ok(());
    }
    Err(unverifiable_message())
}

fn unverifiable_message() -> String {
    "This account was saved without an account identifier, so its sign-in cannot be verified. Remove the account and add it again.".into()
}

/// Pure policy for the repair flow: may `identity`'s live session replace the
/// saved snapshot of the account being repaired?
pub(crate) enum RepairCheck {
    /// The live login is provably the same account; replace its snapshot.
    Match,
    /// The live login belongs to a different Riot account.
    Mismatch,
    /// The saved account no longer exists (removed while repairing).
    AccountMissing,
    /// The saved account has no PUUID, so ownership cannot be proven.
    Unverifiable,
}

pub(crate) fn repair_check(saved: Option<&Account>, identity: &DetectedIdentity) -> RepairCheck {
    let Some(account) = saved else {
        return RepairCheck::AccountMissing;
    };
    let Some(puuid) = account.puuid.as_deref() else {
        return RepairCheck::Unverifiable;
    };
    if puuid == identity.puuid {
        RepairCheck::Match
    } else {
        RepairCheck::Mismatch
    }
}

/// Replaces `id`'s saved session with the live one, which the caller has
/// already detected. The account keeps its id, so no duplicate is created.
pub(crate) fn complete(config: &mut Config, id: Uuid, identity: &DetectedIdentity) -> Result<(), String> {
    if identity.puuid.trim().is_empty() {
        return Err("Could not read account identity. Make sure Riot Client is signed in.".into());
    }
    let saved = config.accounts.iter().find(|a| a.id == id);
    match repair_check(saved, identity) {
        RepairCheck::Match => {}
        RepairCheck::Mismatch => {
            let name = saved
                .map(Account::display_name)
                .unwrap_or_else(|| "the saved account".into());
            return Err(format!(
                "Different Riot account detected. Sign out in Riot Client and sign in to {name}, then retry. Swapper did not change the saved session."
            ));
        }
        RepairCheck::AccountMissing => return Err("Account no longer exists.".into()),
        RepairCheck::Unverifiable => return Err(unverifiable_message()),
    }
    riot::ensure_no_game().map_err(|error| error.message())?;
    let snapshot = vault::capture()?;
    // Clone before applying identity: the snapshot-replace helper rolls the
    // account back from its own pre-save clone, so identity fields mutated
    // beforehand would otherwise survive a failed save.
    let account_before = config.accounts.iter().find(|a| a.id == id).cloned();
    if let Some(account) = config.accounts.iter_mut().find(|a| a.id == id) {
        identity::apply_identity(account, identity);
    }
    // The repaired session is the live one now, so it becomes the active
    // account — the same end state a successful switch would have reached.
    let previous_active = config.active_id;
    config.active_id = Some(id);
    if let Err(e) = vault::replace_account_snapshot(config, id, &snapshot) {
        config.active_id = previous_active;
        if let Some(before) = account_before {
            if let Some(account) = config.accounts.iter_mut().find(|a| a.id == id) {
                *account = before;
            }
        }
        return Err(e);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Post-switch session watch
// ---------------------------------------------------------------------------

/// How long a switched-in account is watched for a dead-on-arrival session,
/// and how often it is probed in that window.
const WATCH_WINDOW: Duration = Duration::from_secs(45);
const WATCH_INTERVAL: Duration = Duration::from_secs(5);

/// One identity probe after a switch, or the watch window running out.
enum SessionObservation {
    /// Riot Client reports the account that was switched to.
    SignedIn,
    /// Riot Client reports a different account than the one switched to.
    SignedInAsDifferent,
    /// Riot Client is up but no account is signed in (login screen).
    AtLogin,
    /// Riot Client is not running.
    ClientGone,
    /// The client was found but its identity could not be read.
    Unreadable,
    /// The watch window elapsed without a decisive observation.
    WindowEnded,
}

enum SessionVerdict {
    KeepWatching,
    /// Stop without telling the user anything.
    Silent,
    /// The session looks expired; offer Repair.
    Expired,
    /// A different account signed in; offer Repair.
    Mismatch,
}

fn observe(detection: &Detection, expected_puuid: &str) -> SessionObservation {
    match detection {
        Detection::Identified(identity) if identity.puuid == expected_puuid => {
            SessionObservation::SignedIn
        }
        Detection::Identified(_) => SessionObservation::SignedInAsDifferent,
        Detection::NotReady => SessionObservation::AtLogin,
        Detection::NotRunning => SessionObservation::ClientGone,
        Detection::Unavailable => SessionObservation::Unreadable,
    }
}

/// Pure policy for the post-switch watch: what does this sequence of
/// observations say about the switched-in session?
///
/// The first decisive observation wins. Before Riot Client has answered at
/// least once, "not running" or "unreadable" only means it is still booting;
/// afterwards they mean the client is gone and the watch stops quietly. An
/// expired verdict requires the client to have been seen sitting at its
/// login screen up to the end of the window — a client that never appeared
/// raises no alarm.
fn expiry_verdict(observations: &[SessionObservation]) -> SessionVerdict {
    let mut client_seen = false;
    for observation in observations {
        match observation {
            SessionObservation::SignedIn => return SessionVerdict::Silent,
            SessionObservation::SignedInAsDifferent => return SessionVerdict::Mismatch,
            SessionObservation::AtLogin => client_seen = true,
            SessionObservation::ClientGone | SessionObservation::Unreadable if client_seen => {
                return SessionVerdict::Silent;
            }
            SessionObservation::ClientGone | SessionObservation::Unreadable => {}
            SessionObservation::WindowEnded => {
                return if client_seen {
                    SessionVerdict::Expired
                } else {
                    SessionVerdict::Silent
                };
            }
        }
    }
    SessionVerdict::KeepWatching
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionExpiredPayload {
    id: Uuid,
    name: String,
    mismatch: bool,
}

/// Watches a freshly switched-in account for a session that was dead on
/// arrival: Riot Client sitting at its login screen for the whole window, or
/// a different account signing in. Runs in the background, never blocks or
/// delays the switch result, and stops as soon as any other account action
/// starts (the switch guard epoch moves on).
pub(crate) fn spawn_session_watch(
    app: AppHandle,
    account_id: Uuid,
    account_name: String,
    expected_puuid: String,
    watch_epoch: u64,
    epoch: Arc<AtomicU64>,
) {
    tauri::async_runtime::spawn(async move {
        let deadline = Instant::now() + WATCH_WINDOW;
        let mut observations = Vec::new();
        loop {
            if epoch.load(Ordering::SeqCst) != watch_epoch {
                return;
            }
            if Instant::now() >= deadline {
                observations.push(SessionObservation::WindowEnded);
            } else {
                // The cheap probe (no League enrichment) is enough: only the
                // signed-in PUUID matters here.
                let detection = identity::detect_account().await;
                // The probe can outlive the epoch: re-check before acting on
                // what it saw.
                if epoch.load(Ordering::SeqCst) != watch_epoch {
                    return;
                }
                observations.push(observe(&detection, &expected_puuid));
            }
            match expiry_verdict(&observations) {
                SessionVerdict::KeepWatching => tokio::time::sleep(WATCH_INTERVAL).await,
                SessionVerdict::Silent => return,
                SessionVerdict::Expired => {
                    notify_session_expired(&app, account_id, &account_name, false);
                    return;
                }
                SessionVerdict::Mismatch => {
                    notify_session_expired(&app, account_id, &account_name, true);
                    return;
                }
            }
        }
    });
}

/// Tells the user, in-app and through a Windows notification, that the
/// account needs the repair flow.
fn notify_session_expired(app: &AppHandle, account_id: Uuid, account_name: &str, mismatch: bool) {
    let _ = app.emit(
        "session_expired",
        SessionExpiredPayload {
            id: account_id,
            name: account_name.to_string(),
            mismatch,
        },
    );
    let body = if mismatch {
        format!("Riot Client signed in to a different account than {account_name}. Use Repair Account to fix it.")
    } else {
        format!("The saved session for {account_name} has expired. Use Repair Account to sign in again.")
    };
    let _ = app
        .notification()
        .builder()
        .title("Session needs repair")
        .body(body)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    use SessionObservation::{AtLogin, ClientGone, SignedIn, SignedInAsDifferent, Unreadable, WindowEnded};
    use SessionVerdict::{Expired, KeepWatching, Mismatch, Silent};

    fn identity(puuid: &str) -> DetectedIdentity {
        DetectedIdentity {
            puuid: puuid.into(),
            game_name: "Example".into(),
            tag_line: "ABC".into(),
            platform: Some("EUW1".into()),
            region: Some("EUW".into()),
            profile_icon_id: None,
            icon_data_url: None,
        }
    }

    fn account(puuid: Option<&str>) -> Account {
        Account {
            id: Uuid::new_v4(),
            name: "Example#TAG".into(),
            puuid: puuid.map(str::to_string),
            game_name: Some("Example".into()),
            tag_line: Some("TAG".into()),
            platform: Some("EUW1".into()),
            region: Some("EUW".into()),
            profile_icon_id: None,
            nickname: None,
            vault_id: Uuid::new_v4(),
        }
    }

    #[test]
    fn matching_puuid_replaces_the_saved_snapshot() {
        let saved = account(Some("PUUID-A"));
        assert!(matches!(
            repair_check(Some(&saved), &identity("PUUID-A")),
            RepairCheck::Match
        ));
    }

    #[test]
    fn a_different_signed_in_account_never_replaces_the_snapshot() {
        let saved = account(Some("PUUID-A"));
        assert!(matches!(
            repair_check(Some(&saved), &identity("PUUID-B")),
            RepairCheck::Mismatch
        ));
    }

    #[test]
    fn a_removed_account_cannot_be_repaired() {
        assert!(matches!(
            repair_check(None, &identity("PUUID-A")),
            RepairCheck::AccountMissing
        ));
    }

    #[test]
    fn an_account_saved_without_puuid_cannot_be_verified() {
        let saved = account(None);
        assert!(matches!(
            repair_check(Some(&saved), &identity("PUUID-A")),
            RepairCheck::Unverifiable
        ));
    }

    #[test]
    fn only_dead_session_failures_offer_repair() {
        assert!(is_repairable(SwitchFailureKind::SessionRestore));
        for kind in [
            SwitchFailureKind::GameRunning,
            SwitchFailureKind::ClientBusy,
            SwitchFailureKind::InspectionFailed,
            SwitchFailureKind::AccountMissing,
            SwitchFailureKind::LauncherMissing,
            SwitchFailureKind::Launch,
            SwitchFailureKind::Other,
        ] {
            assert!(!is_repairable(kind));
        }
    }

    #[test]
    fn signing_in_ends_the_watch_without_an_alarm() {
        assert!(matches!(expiry_verdict(&[SignedIn]), Silent));
        assert!(matches!(expiry_verdict(&[AtLogin, AtLogin, SignedIn]), Silent));
    }

    #[test]
    fn a_window_spent_at_the_login_screen_means_the_session_expired() {
        assert!(matches!(expiry_verdict(&[AtLogin, AtLogin, WindowEnded]), Expired));
        assert!(matches!(expiry_verdict(&[Unreadable, AtLogin, WindowEnded]), Expired));
    }

    #[test]
    fn a_different_account_signing_in_is_reported_as_a_mismatch() {
        assert!(matches!(expiry_verdict(&[SignedInAsDifferent]), Mismatch));
        assert!(matches!(
            expiry_verdict(&[AtLogin, SignedInAsDifferent, WindowEnded]),
            Mismatch
        ));
    }

    #[test]
    fn a_client_that_closes_mid_watch_raises_no_alarm() {
        assert!(matches!(expiry_verdict(&[AtLogin, ClientGone]), Silent));
        assert!(matches!(expiry_verdict(&[AtLogin, ClientGone, WindowEnded]), Silent));
    }

    #[test]
    fn an_absent_or_unreadable_client_is_never_an_expiry() {
        // Riot Client may still be booting: keep watching until the window ends.
        assert!(matches!(expiry_verdict(&[ClientGone, ClientGone]), KeepWatching));
        assert!(matches!(expiry_verdict(&[ClientGone, WindowEnded]), Silent));
        assert!(matches!(expiry_verdict(&[Unreadable, Unreadable]), KeepWatching));
        assert!(matches!(expiry_verdict(&[Unreadable, WindowEnded]), Silent));
        assert!(matches!(expiry_verdict(&[WindowEnded]), Silent));
    }

    #[test]
    fn the_first_decisive_observation_wins() {
        assert!(matches!(expiry_verdict(&[SignedIn, SignedInAsDifferent]), Silent));
        assert!(matches!(expiry_verdict(&[SignedInAsDifferent, ClientGone]), Mismatch));
    }

    #[test]
    fn probes_map_detections_by_puuid() {
        assert!(matches!(
            observe(&Detection::Identified(identity("PUUID-A")), "PUUID-A"),
            SessionObservation::SignedIn
        ));
        assert!(matches!(
            observe(&Detection::Identified(identity("PUUID-B")), "PUUID-A"),
            SessionObservation::SignedInAsDifferent
        ));
        assert!(matches!(observe(&Detection::NotReady, "PUUID-A"), AtLogin));
        assert!(matches!(observe(&Detection::NotRunning, "PUUID-A"), ClientGone));
        assert!(matches!(observe(&Detection::Unavailable, "PUUID-A"), Unreadable));
    }
}
