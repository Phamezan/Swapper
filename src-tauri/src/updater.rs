//! Automatic updates through the official Tauri updater, fed by GitHub
//! Releases.
//!
//! Swapper checks the release feed shortly after startup and roughly every six
//! hours while running — never while an account switch is in progress. The
//! result is published to both windows through the [`UPDATE_EVENT`] event and
//! readable any time through [`update_status`]. Installing downloads the signed
//! NSIS installer and hands over to it; on failure the command returns and the
//! current version keeps running.
//!
//! Update checking only runs when the release feed is properly configured.
//! Builds that still carry the [`PUBKEY_PLACEHOLDER`] signing key stay silent:
//! no checks, no errors, and Doctor reports the build as not configured.

use std::cmp::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Error as UpdaterError, UpdaterExt};

/// The signing key a release build must replace. While this placeholder is in
/// tauri.conf.json, update checking is disabled for the whole app.
pub const PUBKEY_PLACEHOLDER: &str = "REPLACE_WITH_UPDATER_PUBLIC_KEY";
/// How often a scheduled check runs while Swapper is up.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// How often the scheduler reconsiders whether a check is due — a check that
/// was skipped during a switch happens at the next tick, not a full interval later.
const SCHEDULE_TICK: Duration = Duration::from_secs(10 * 60);
/// The first check waits a moment so startup never competes with the tray,
/// the remote service and the rune watcher.
const STARTUP_DELAY: Duration = Duration::from_secs(10);
const UPDATE_EVENT: &str = "update_state";
/// Release notes are one short line in the UI.
const MAX_NOTES_CHARS: usize = 240;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStateKind {
    NotConfigured,
    Checking,
    Available,
    UpToDate,
    Installing,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSnapshot {
    pub state: UpdateStateKind,
    pub current_version: String,
    pub available_version: Option<String>,
    /// First line of the release notes for the announced version.
    pub notes: Option<String>,
    /// Short reason when the last check or install failed.
    pub message: Option<String>,
    /// Unix time of the last completed check, for "checked x minutes ago".
    pub last_check: Option<u64>,
}

pub struct UpdateState {
    app: AppHandle,
    snapshot: Mutex<UpdateSnapshot>,
    last_scheduled_check: Mutex<Option<Instant>>,
}

impl UpdateState {
    pub fn new(app: AppHandle) -> Self {
        let current_version = app.package_info().version.to_string();
        let state = if configured(&app) {
            UpdateStateKind::Checking
        } else {
            UpdateStateKind::NotConfigured
        };
        Self {
            app,
            snapshot: Mutex::new(UpdateSnapshot {
                state,
                current_version,
                available_version: None,
                notes: None,
                message: None,
                last_check: None,
            }),
            last_scheduled_check: Mutex::new(None),
        }
    }

    pub fn snapshot(&self) -> UpdateSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn publish(
        &self,
        state: UpdateStateKind,
        available_version: Option<String>,
        notes: Option<String>,
        message: Option<String>,
        mark_checked: bool,
    ) {
        let snapshot = {
            let mut snapshot = self
                .snapshot
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            snapshot.state = state;
            snapshot.available_version = available_version;
            snapshot.notes = notes;
            snapshot.message = message;
            if mark_checked {
                snapshot.last_check = Some(unix_now());
            }
            snapshot.clone()
        };
        let _ = self.app.emit(UPDATE_EVENT, snapshot);
    }

    fn set_checking(&self) {
        self.publish(UpdateStateKind::Checking, None, None, None, false);
    }

    fn set_available(&self, version: String, notes: Option<String>) {
        self.publish(UpdateStateKind::Available, Some(version), notes, None, true);
    }

    fn set_up_to_date(&self) {
        self.publish(UpdateStateKind::UpToDate, None, None, None, true);
    }

    fn set_failed(&self, message: String) {
        self.publish(UpdateStateKind::Failed, None, None, Some(message), true);
    }

    fn set_installing(&self) {
        self.publish(UpdateStateKind::Installing, None, None, None, false);
    }
}

/// True when the feed announces a version strictly newer than the installed one.
/// Anything else is treated as up to date, so a stale or mispublished feed can
/// never downgrade Swapper.
pub fn compare_versions(left: &str, right: &str) -> Ordering {
    let left = numeric_components(left);
    let right = numeric_components(right);
    for index in 0..left.len().max(right.len()) {
        match left.get(index).copied().unwrap_or(0).cmp(&right.get(index).copied().unwrap_or(0)) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

/// Numeric components of a version string, stopping at the first component
/// that is not purely numeric. The feed only announces stable releases, so a
/// pre-release label ("0.6.0-beta.1") counts as its release ("0.6.0") and
/// pre-release ordering is not modelled.
fn numeric_components(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches(['v', 'V'])
        .split('.')
        .take_while(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// Whether a scheduled check is due. `None` means Swapper never checked.
pub fn check_due(last: Option<Instant>, now: Instant) -> bool {
    match last {
        None => true,
        Some(last) => now
            .checked_duration_since(last)
            .is_some_and(|elapsed| elapsed >= CHECK_INTERVAL),
    }
}

/// A scheduled check only runs when checking is configured and nothing
/// sensitive is happening: never during an account switch or an install.
pub fn should_run_check(configured: bool, switching: bool, installing: bool) -> bool {
    configured && !switching && !installing
}

/// Update checking stays off until the release signing key replaces the
/// placeholder, so unconfigured builds never surface updater errors.
pub fn pubkey_configured(pubkey: Option<&str>) -> bool {
    pubkey
        .map(str::trim)
        .is_some_and(|key| !key.is_empty() && key != PUBKEY_PLACEHOLDER)
}

fn configured_pubkey(app: &AppHandle) -> Option<String> {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|plugin| plugin.get("pubkey"))
        .and_then(|key| key.as_str())
        .map(str::to_string)
}

fn configured(app: &AppHandle) -> bool {
    pubkey_configured(configured_pubkey(app).as_deref())
}

/// The first non-empty line of the release body, capped for the update card.
pub fn short_notes(body: Option<&str>) -> Option<String> {
    let line = body?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    Some(if line.chars().count() <= MAX_NOTES_CHARS {
        line.to_string()
    } else {
        let mut notes: String = line.chars().take(MAX_NOTES_CHARS).collect();
        notes.push('…');
        notes
    })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Checks the feed once and publishes the result. Used by the scheduler and by
/// the manual check command.
async fn run_check(app: &AppHandle) {
    let Some(state) = app.try_state::<UpdateState>() else {
        return;
    };
    if !configured(app) {
        state.publish(UpdateStateKind::NotConfigured, None, None, None, false);
        return;
    }
    state.set_checking();
    let result = async {
        let updater = app.updater()?;
        updater.check().await
    }
    .await;
    match result {
        Ok(Some(update)) => {
            if compare_versions(&update.version, &update.current_version) == Ordering::Greater {
                state.set_available(update.version, short_notes(update.body.as_deref()));
            } else {
                state.set_up_to_date();
            }
        }
        Ok(None) => state.set_up_to_date(),
        // A missing manifest means the newest published release predates the
        // updater — there is nothing to update to, so this is not a failure.
        Err(error) if is_missing_feed_error(&error) => state.set_up_to_date(),
        Err(error) => {
            let message = friendly_check_error(&error);
            state.set_failed(message);
        }
    }
}

/// Runs one scheduled check if the schedule and the current activity allow it.
async fn run_scheduled_check(app: &AppHandle) {
    let Some(state) = app.try_state::<UpdateState>() else {
        return;
    };
    let last = *state
        .last_scheduled_check
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !check_due(last, Instant::now()) {
        return;
    }
    let switching = app
        .try_state::<crate::AppState>()
        .is_some_and(|state| state.is_switching());
    let installing = state.snapshot().state == UpdateStateKind::Installing;
    if !should_run_check(configured(app), switching, installing) {
        return;
    }
    *state
        .last_scheduled_check
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(Instant::now());
    run_check(app).await;
}

/// The background scheduler: one check shortly after startup, then a tick
/// every [`SCHEDULE_TICK`] that runs a check when one is due.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        run_scheduled_check(&app).await;
        loop {
            tokio::time::sleep(SCHEDULE_TICK).await;
            run_scheduled_check(&app).await;
        }
    });
}

/// True when a check failed only because no endpoint served a release
/// manifest. The updater plugin answers a 404 (or any other non-success
/// status) from the feed with [`UpdaterError::ReleaseNotFound`] and no
/// underlying request error, so this is exactly the "the latest published
/// release has no latest.json yet" case — not a network failure, which always
/// surfaces as [`UpdaterError::Network`] or [`UpdaterError::Reqwest`] instead.
pub fn is_missing_feed_error(error: &UpdaterError) -> bool {
    matches!(error, UpdaterError::ReleaseNotFound)
}

fn friendly_check_error(error: &UpdaterError) -> String {
    match error {
        UpdaterError::Reqwest(problem) if problem.is_timeout() => {
            "The update check timed out. Check your internet connection, then retry.".into()
        }
        UpdaterError::Reqwest(problem) if problem.is_connect() => {
            "Could not reach the update server. Check your internet connection, then retry.".into()
        }
        _ => "Could not check for updates. Swapper keeps running the current version.".into(),
    }
}

fn friendly_install_error(error: &UpdaterError) -> String {
    match error {
        UpdaterError::Network(_) | UpdaterError::Reqwest(_) => {
            "The update download failed. Check your internet connection, then retry.".into()
        }
        UpdaterError::Minisign(_)
        | UpdaterError::SignatureUtf8(_)
        | UpdaterError::SignedVersionMismatch { .. } => {
            "The update could not be verified as an official Swapper release.".into()
        }
        _ => {
            "The update could not be installed. Swapper keeps running the current version.".into()
        }
    }
}

#[tauri::command]
pub fn update_status(state: State<'_, UpdateState>) -> UpdateSnapshot {
    state.snapshot()
}

/// Manual "Check for updates" from Settings.
#[tauri::command]
pub async fn update_check_now(app: AppHandle) -> Result<UpdateSnapshot, String> {
    let state = app
        .try_state::<UpdateState>()
        .ok_or("Swapper is still starting. Try again in a moment.")?;
    run_check(&app).await;
    Ok(state.snapshot())
}

/// Downloads the announced update and hands over to the NSIS installer. On
/// Windows the plugin exits the process itself once the installer is confirmed
/// running, and the installer relaunches Swapper afterwards — so every error
/// path here leaves the current version running.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<UpdateState>()
        .ok_or("Swapper is still starting. Try again in a moment.")?;
    match state.snapshot().state {
        UpdateStateKind::Installing => {
            return Err("An update is already being installed.".into());
        }
        UpdateStateKind::Available => {}
        _ => return Err("No update is ready to install. Check for updates first.".into()),
    }
    let switching = app
        .try_state::<crate::AppState>()
        .is_some_and(|state| state.is_switching());
    if switching {
        return Err("Wait for the account switch to finish, then update.".into());
    }
    state.set_installing();
    let update = match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update))
                if compare_versions(&update.version, &update.current_version)
                    == Ordering::Greater =>
            {
                update
            }
            Ok(_) => {
                state.set_up_to_date();
                return Err("No update is available anymore.".into());
            }
            Err(error) if is_missing_feed_error(&error) => {
                state.set_up_to_date();
                return Err("No update is available anymore.".into());
            }
            Err(error) => {
                let message = friendly_check_error(&error);
                state.set_failed(message.clone());
                return Err(message);
            }
        },
        Err(error) => {
            let message = friendly_check_error(&error);
            state.set_failed(message.clone());
            return Err(message);
        }
    };
    if let Err(error) = update.download_and_install(|_, _| {}, || {}).await {
        let message = friendly_install_error(&error);
        state.set_failed(message.clone());
        return Err(message);
    }
    // Reached only off Windows; the installer already took over there.
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_pubkey_keeps_checking_disabled() {
        assert!(!pubkey_configured(None));
        assert!(!pubkey_configured(Some("")));
        assert!(!pubkey_configured(Some("   ")));
        assert!(!pubkey_configured(Some(PUBKEY_PLACEHOLDER)));
        assert!(!pubkey_configured(Some("  REPLACE_WITH_UPDATER_PUBLIC_KEY ")));
        assert!(pubkey_configured(Some(
            "dW50cnVzdGVkIGNvbW1lbnQ6IHRoZSBwdWJsaWMga2V5"
        )));
        assert!(pubkey_configured(Some("  dW50cnVzdGVk  ")));
    }

    #[test]
    fn versions_compare_by_numeric_components() {
        use Ordering::{Equal, Greater, Less};
        assert_eq!(compare_versions("0.4.0", "0.4.0"), Equal);
        assert_eq!(compare_versions("0.4.0", "0.4.1"), Less);
        assert_eq!(compare_versions("0.4.1", "0.4.0"), Greater);
        assert_eq!(compare_versions("0.9.3", "0.10.0"), Less);
        assert_eq!(compare_versions("0.10.0", "1.0.0"), Less);
        assert_eq!(compare_versions("v0.5.0", "0.5.0"), Equal);
        // Missing components count as zero.
        assert_eq!(compare_versions("0.5", "0.5.0"), Equal);
        assert_eq!(compare_versions("0.5", "0.5.1"), Less);
        // Non-numeric parts never crash the comparison.
        assert_eq!(compare_versions("0.6.0-beta.1", "0.6.0"), Equal);
        assert_eq!(compare_versions("", "0.4.0"), Less);
    }

    #[test]
    fn scheduled_checks_wait_for_the_interval() {
        let now = Instant::now();
        assert!(check_due(None, now), "the very first check is always due");
        assert!(!check_due(Some(now), now));
        // A check ten minutes back is far inside the six-hour interval.
        assert!(!check_due(Some(now - Duration::from_secs(10 * 60)), now));
        assert!(check_due(Some(now), now + CHECK_INTERVAL));
    }

    #[test]
    fn scheduled_checks_skip_switches_installs_and_unconfigured_builds() {
        assert!(should_run_check(true, false, false));
        assert!(!should_run_check(false, false, false), "placeholder build");
        assert!(!should_run_check(true, true, false), "switch in progress");
        assert!(!should_run_check(true, false, true), "install in progress");
    }

    #[test]
    fn missing_feed_is_only_the_release_not_found_error() {
        // A 404 from the feed answers as ReleaseNotFound with no cause.
        assert!(is_missing_feed_error(&UpdaterError::ReleaseNotFound));
        // Network problems are real failures, never classified as a missing feed.
        assert!(!is_missing_feed_error(&UpdaterError::Network("timeout".into())));
        assert!(!is_missing_feed_error(&UpdaterError::EmptyEndpoints));
        assert!(!is_missing_feed_error(&UpdaterError::UnsupportedOs));
    }

    #[test]
    fn short_notes_takes_the_first_non_empty_line_and_caps_it() {
        assert_eq!(short_notes(None), None);
        assert_eq!(short_notes(Some("\n \r\n")), None);
        assert_eq!(
            short_notes(Some("\r\nFirst real line\n- detail")),
            Some("First real line".into())
        );
        let long = "x".repeat(MAX_NOTES_CHARS + 10);
        let capped = short_notes(Some(&long)).unwrap();
        assert_eq!(capped.chars().count(), MAX_NOTES_CHARS + 1);
        assert!(capped.ends_with('…'));
    }
}
