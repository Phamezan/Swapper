use crate::identity::{self, Detection, DetectedIdentity};
use crate::vault::{self, Account, Config};
use crate::windows::process::{self, SnapshotError, StopError, StopOptions, StopReport};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use uuid::Uuid;

const CLIENTS: &[&str] = &[
    "RiotClientServices.exe",
    "Riot Client.exe",
    "RiotClientUx.exe",
    "RiotClientUxRender.exe",
    "LeagueClient.exe",
    "LeagueClientUx.exe",
    "LeagueClientUxRender.exe",
    "Deceive.exe",
];
/// Riot Client processes ignore `WM_CLOSE` (Services) or only hide to the
/// tray (the Electron UI), so a graceful close just burns the grace period.
const FORCE_ONLY: &[&str] = &[
    "RiotClientServices.exe",
    "Riot Client.exe",
    "RiotClientUx.exe",
    "RiotClientUxRender.exe",
];
const GAMES: &[&str] = &[
    "League of Legends.exe",
    "LeagueofLegends.exe",
    "VALORANT-Win64-Shipping.exe",
];

/// Why a Riot process check refused to let a switch continue.
///
/// [`InspectionFailed`](Self::InspectionFailed) is the fail-closed case: the
/// process table could not be read, so Swapper cannot claim that no game is
/// running and must not touch the live session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessStateError {
    GameRunning,
    ClientStillRunning,
    InspectionFailed(SnapshotError),
}

impl ProcessStateError {
    pub fn message(self) -> String {
        match self {
            Self::GameRunning => {
                "Close the running Riot game before switching accounts.".into()
            }
            Self::ClientStillRunning => {
                "Riot Client is still running. Close it from the tray and try again.".into()
            }
            Self::InspectionFailed(error) => format!(
                "Swapper could not verify which Riot processes are running ({error}). No account or session data was changed - try again."
            ),
        }
    }
}

pub(crate) fn ensure_no_game() -> Result<(), ProcessStateError> {
    match process::any_running(GAMES) {
        Ok(true) => Err(ProcessStateError::GameRunning),
        Ok(false) => Ok(()),
        Err(error) => Err(ProcessStateError::InspectionFailed(error)),
    }
}

/// Whether any Riot client process is running; an unreadable process table
/// counts as not running, so a settings change never restarts on a guess.
pub(crate) fn client_running() -> bool {
    process::any_running(CLIENTS).unwrap_or(false)
}

/// Stop every Riot client process so the accounts can be swapped.
///
/// The game check is part of the same snapshot that collects the kill
/// targets, so a running game can never be terminated as a side effect of a
/// switch. Processes are asked to close themselves first; only what survives
/// is force killed, in parallel, with console windows hidden.
///
/// An unreadable process table aborts the switch instead of being treated as
/// "no clients and no game".
fn stop_clients() -> Result<StopReport, ProcessStateError> {
    let options = StopOptions {
        force_only: FORCE_ONLY,
        ..StopOptions::default()
    };
    match process::stop_images(CLIENTS, GAMES, options) {
        Ok(report) => Ok(report),
        Err(StopError::ForbiddenPresent) => Err(ProcessStateError::GameRunning),
        Err(StopError::StillRunning(_)) => Err(ProcessStateError::ClientStillRunning),
        Err(StopError::EnumerationFailed(error)) => {
            Err(ProcessStateError::InspectionFailed(error))
        }
    }
}

fn is_exe(path: &Path, expected: &str) -> bool {
    path.is_file()
        && path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(expected))
}

fn configured_path(path: &Option<String>, expected: &str) -> Option<PathBuf> {
    path.as_deref()
        .map(PathBuf::from)
        .filter(|p| is_exe(p, expected))
}

/// Path of a running executable. The name comes from one snapshot pass, and
/// sysinfo is only asked to resolve the path for the PIDs that matched.
fn running_path(exe: &str) -> Option<PathBuf> {
    // A failed enumeration degrades to the built-in path candidates below.
    let pids: Vec<Pid> = process::matching(&[exe])
        .ok()?
        .iter()
        .map(|found| Pid::from_u32(found.pid))
        .collect();
    if pids.is_empty() {
        return None;
    }
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        false,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    system
        .processes()
        .values()
        .filter_map(|process| process.exe().map(Path::to_path_buf))
        .find(|path| is_exe(path, exe))
}

pub fn riot_path(config: &Config) -> Option<PathBuf> {
    configured_path(&config.riot_exe, "RiotClientServices.exe")
        .or_else(|| running_path("RiotClientServices.exe"))
        .or_else(|| {
            let candidates = [
                PathBuf::from(r"C:\Riot Games\Riot Client\RiotClientServices.exe"),
                std::env::var_os("ProgramFiles")
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join(r"Riot Games\Riot Client\RiotClientServices.exe"),
                std::env::var_os("ProgramFiles(x86)")
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join(r"Riot Games\Riot Client\RiotClientServices.exe"),
            ];
            candidates
                .into_iter()
                .find(|p| is_exe(p, "RiotClientServices.exe"))
        })
}

pub fn deceive_path(config: &Config, bundled: &Path) -> Option<PathBuf> {
    is_exe(bundled, "Deceive.exe")
        .then(|| bundled.to_path_buf())
        .or_else(|| configured_path(&config.deceive_exe, "Deceive.exe"))
        .or_else(|| running_path("Deceive.exe"))
        .or_else(|| {
            let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
            let candidates = [
                local.join(r"Programs\Deceive\Deceive.exe"),
                local.join(r"Deceive\Deceive.exe"),
            ];
            candidates.into_iter().find(|p| is_exe(p, "Deceive.exe"))
        })
}

fn launch(path: &Path, args: &[&str]) -> Result<(), String> {
    Command::new(path)
        .args(args)
        .current_dir(path.parent().ok_or("Invalid executable path")?)
        .spawn()
        .map_err(|e| format!("Could not launch {}: {e}", path.display()))?;
    Ok(())
}

fn launch_selected(config: &Config, bundled_deceive: &Path) -> Result<(), String> {
    if config.use_deceive {
        let path = deceive_path(config, bundled_deceive)
            .ok_or("Bundled Deceive.exe was not found. Reinstall Swapper.")?;
        launch(&path, &["lol"])
    } else {
        let path =
            riot_path(config).ok_or("Riot Client was not found. Set its path in Settings.")?;
        launch(
            &path,
            &[
                "--launch-product=league_of_legends",
                "--launch-patchline=live",
            ],
        )
    }
}

/// A live API identity is authoritative; a captured session subject is the
/// fallback when Riot Client cannot identify a login. Unknown ownership skips.
fn verified_live_identity(
    config: &Config,
    detection: &Detection,
    snapshot: Option<&vault::Snapshot>,
) -> Result<bool, String> {
    let Some(active) = config
        .active_id
        .and_then(|id| config.accounts.iter().find(|a| a.id == id))
    else {
        return Ok(false);
    };
    if active.puuid.is_none() {
        return Ok(false);
    }
    let safe = match detection {
        Detection::Identified(identity) => identity_safe_for(active, identity),
        _ => {
            let Some(puuid) = snapshot.and_then(vault::Snapshot::puuid) else {
                return Ok(false);
            };
            active.puuid.as_deref() == Some(puuid.as_str())
        }
    };
    if !safe {
        return Err(mismatch_message(active));
    }
    Ok(true)
}

/// Refuse an ownership mismatch before stopping any clients. Closed clients
/// have no API identity, so inspect a capture without modifying the live files.
fn verify_before_stop(config: &Config, detection: &Detection) -> Result<(), String> {
    if config.active_id.is_none() || !vault::live_session_exists()? {
        return Ok(());
    }
    let snapshot = if matches!(detection, Detection::Identified(_)) {
        None
    } else {
        Some(vault::capture()?)
    };
    verified_live_identity(config, detection, snapshot.as_ref())?;
    Ok(())
}

/// Pure policy for `verified_live_identity`: may `identity` be written into
/// `active`'s saved snapshot?
pub(crate) fn identity_safe_for(
    active: &Account,
    identity: &DetectedIdentity,
) -> bool {
    active.puuid.as_deref() == Some(identity.puuid.as_str())
}

fn mismatch_message(account: &Account) -> String {
    format!(
        "Different Riot account detected. The current Riot login no longer matches the saved account {}. Save this account separately or sign back into the expected account.",
        account.display_name()
    )
}

/// Refresh through one save path, verifying ownership against the capture that
/// will actually be saved. Returns false when ownership cannot be established.
pub(crate) fn refresh_active(config: &mut Config, detection: &Detection) -> Result<bool, String> {
    refresh_active_with_snapshot(config, detection, None)
}

fn refresh_active_with_snapshot(
    config: &mut Config,
    detection: &Detection,
    snapshot: Option<&vault::Snapshot>,
) -> Result<bool, String> {
    if config.active_id.is_none() {
        return Ok(false);
    }
    let owned;
    let fresh = match snapshot {
        Some(s) => s,
        None => {
            if !vault::live_session_exists()? {
                return Ok(false);
            }
            owned = vault::capture()?;
            &owned
        }
    };
    if !verified_live_identity(config, detection, Some(fresh))? {
        return Ok(false);
    }
    let active = config.active_id.unwrap();
    let index = config
        .accounts
        .iter()
        .position(|a| a.id == active)
        .ok_or("Active account is missing from Swapper settings")?;
    let new_vault = vault::save_snapshot(fresh)?;
    let previous = config.accounts[index].clone();
    let old_vault = previous.vault_id;
    if let Detection::Identified(identity) = detection {
        identity::apply_identity(&mut config.accounts[index], identity);
    }
    config.accounts[index].vault_id = new_vault;
    if let Err(e) = vault::save(config) {
        config.accounts[index] = previous;
        let _ = vault::remove_snapshot(new_vault);
        return Err(e);
    }
    let _ = vault::remove_snapshot(old_vault);
    Ok(true)
}

pub fn open_riot(config: &Config) -> Result<(), String> {
    let path = riot_path(config).ok_or("Riot Client was not found. Set its path in Settings.")?;
    launch(&path, &[])
}

pub fn begin_add(config: &mut Config, detection: &Detection) -> Result<(), String> {
    ensure_no_game().map_err(|error| error.message())?;
    if config.active_id.is_none() && vault::live_session_exists()? {
        return Err("Save the current Riot sign-in first, then start a new account.".into());
    }
    let riot = riot_path(config).ok_or("Riot Client was not found. Set its path in Settings.")?;
    // Verify the live identity while Riot Client is still running, so a
    // manual Riot login change can never corrupt the active account's vault.
    verify_before_stop(config, detection)?;
    stop_clients().map_err(|error| error.message())?;
    refresh_active(config, detection)?;
    vault::clear_live()?;
    let previous_active = config.active_id;
    config.active_id = None;
    if let Err(e) = vault::save(config) {
        config.active_id = previous_active;
        if let Some(id) = previous_active {
            if let Some(account) = config.accounts.iter().find(|a| a.id == id) {
                if let Ok(snapshot) = vault::read_snapshot(account.vault_id) {
                    let _ = vault::restore(&snapshot);
                }
            }
        }
        return Err(e);
    }
    launch(&riot, &[])
}

/// A matching display ID is not proof of ownership when the stable IDs differ.
/// Keep both saved sessions intact and require the user to resolve the conflict.
fn conflicting_riot_id(config: &Config, identity: &DetectedIdentity) -> bool {
    config.accounts.iter().any(|account| {
        account.puuid.as_deref() != Some(identity.puuid.as_str())
            && account
                .riot_id()
                .is_some_and(|riot_id| riot_id.eq_ignore_ascii_case(&identity.riot_id()))
    })
}

pub fn complete_add(config: &mut Config, identity: &DetectedIdentity) -> Result<(), String> {
    if identity.puuid.trim().is_empty() {
        return Err("Could not read account identity. Make sure Riot Client is signed in.".into());
    }
    if let Some(index) = config
        .accounts
        .iter()
        .position(|a| a.puuid.as_deref() == Some(identity.puuid.as_str()))
    {
        // Already saved: refresh that account's metadata and session in place
        // instead of creating a duplicate account or vault entry.
        stop_clients().map_err(|error| error.message())?;
        let snapshot = vault::capture()?;
        let new_vault = vault::save_snapshot(&snapshot)?;
        let previous = config.accounts[index].clone();
        let old_vault = previous.vault_id;
        identity::apply_identity(&mut config.accounts[index], identity);
        config.accounts[index].vault_id = new_vault;
        let previous_active = config.active_id;
        config.active_id = Some(config.accounts[index].id);
        if let Err(e) = vault::save(config) {
            config.accounts[index] = previous;
            config.active_id = previous_active;
            vault::remove_snapshot(new_vault)?;
            return Err(e);
        }
        let _ = vault::remove_snapshot(old_vault);
        return Ok(());
    }
    if conflicting_riot_id(config, identity) {
        return Err("An account with this Riot ID is already saved, but its account identifier differs. Swapper will not create a duplicate or overwrite that saved session automatically.".into());
    }
    stop_clients().map_err(|error| error.message())?;
    let snapshot = vault::capture()?;
    let vault_id = vault::save_snapshot(&snapshot)?;
    let account = Account {
        id: Uuid::new_v4(),
        name: identity.riot_id(),
        puuid: Some(identity.puuid.clone()),
        game_name: Some(identity.game_name.clone()),
        tag_line: Some(identity.tag_line.clone()),
        platform: identity.platform.clone(),
        region: identity.region.clone(),
        profile_icon_id: identity.profile_icon_id,
        nickname: None,
        vault_id,
    };
    let previous_active = config.active_id;
    config.active_id = Some(account.id);
    let account_id = account.id;
    config.accounts.push(account);
    if let Err(e) = vault::save(config) {
        config.accounts.pop();
        config.active_id = previous_active;
        vault::remove_snapshot(vault_id)?;
        return Err(e);
    }
    if let Some(icon_data_url) = identity.icon_data_url.as_deref() {
        // Profile art is presentation-only; a cache write failure must never
        // invalidate a successfully saved Riot account session.
        let _ = vault::save_profile_icon(account_id, icon_data_url);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchFailureKind {
    GameRunning,
    ClientBusy,
    InspectionFailed,
    AccountMissing,
    LauncherMissing,
    SessionRestore,
    Launch,
    Other,
}

#[derive(Debug)]
pub struct SwitchFailure {
    pub kind: SwitchFailureKind,
    pub detail: String,
}

impl SwitchFailure {
    fn new(kind: SwitchFailureKind, detail: impl Into<String>) -> Self {
        Self { kind, detail: detail.into() }
    }
}

impl From<String> for SwitchFailure {
    fn from(detail: String) -> Self {
        Self::new(SwitchFailureKind::Other, detail)
    }
}

/// Maps a process-state refusal onto the switch failure the UI explains.
/// An unreadable process table is reported as its own kind so the user is
/// never told a game is running when Swapper simply could not look.
fn switch_failure(error: ProcessStateError) -> SwitchFailure {
    let kind = match error {
        ProcessStateError::GameRunning => SwitchFailureKind::GameRunning,
        ProcessStateError::ClientStillRunning => SwitchFailureKind::ClientBusy,
        ProcessStateError::InspectionFailed(_) => SwitchFailureKind::InspectionFailed,
    };
    SwitchFailure::new(kind, error.message())
}

pub fn switch_account(
    config: &mut Config,
    id: Uuid,
    bundled_deceive: &Path,
    detection: &Detection,
) -> Result<(), SwitchFailure> {
    let t_pre = Instant::now();
    ensure_no_game().map_err(switch_failure)?;
    let target = config
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| SwitchFailure::new(SwitchFailureKind::AccountMissing, "Account no longer exists"))?;
    // Check the target and launch executable before any live session is changed.
    let mut target_snapshot = vault::read_snapshot(target.vault_id)
        .map_err(|e| SwitchFailure::new(SwitchFailureKind::SessionRestore, e))?;
    if config.use_deceive {
        deceive_path(config, bundled_deceive)
            .ok_or_else(|| SwitchFailure::new(SwitchFailureKind::LauncherMissing, "Bundled Deceive.exe was not found. Reinstall Swapper."))?;
    } else {
        riot_path(config).ok_or_else(|| SwitchFailure::new(SwitchFailureKind::LauncherMissing, "Riot Client was not found. Set its path in Settings."))?;
    }
    // Verify the live identity before Riot Client is closed, so a manual
    // Riot login change can never overwrite another account's saved session.
    verify_before_stop(config, detection)?;
    let pre_ms = t_pre.elapsed();

    let t_stop = Instant::now();
    let stop_report = stop_clients().map_err(switch_failure)?;
    let stop_ms = t_stop.elapsed();

    let t_capture = Instant::now();
    let previous = if vault::live_session_exists()? {
        Some(vault::capture()?)
    } else {
        None
    };
    refresh_active_with_snapshot(config, detection, previous.as_ref())?;
    let capture_ms = t_capture.elapsed();

    let t_restore = Instant::now();
    if config.active_id == Some(id) {
        let fresh_id = config
            .accounts
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .vault_id;
        target_snapshot = vault::read_snapshot(fresh_id)
            .map_err(|e| SwitchFailure::new(SwitchFailureKind::SessionRestore, e))?;
    }
    if let Err(e) = vault::restore(&target_snapshot) {
        if let Some(previous) = previous.as_ref() {
            let _ = vault::restore(previous);
        } else {
            let _ = vault::clear_live();
        }
        return Err(SwitchFailure::new(SwitchFailureKind::SessionRestore, format!("Could not restore account: {e}")));
    }
    let previous_active = config.active_id;
    config.active_id = Some(id);
    if let Err(e) = vault::save(config) {
        config.active_id = previous_active;
        if let Some(previous) = previous.as_ref() {
            let _ = vault::restore(previous);
        } else {
            let _ = vault::clear_live();
        }
        return Err(e.into());
    }
    let restore_save_ms = t_restore.elapsed();

    let t_launch = Instant::now();
    if let Err(e) = launch_selected(config, bundled_deceive) {
        if let Some(previous) = previous.as_ref() {
            let _ = vault::restore(previous);
        } else {
            let _ = vault::clear_live();
        }
        config.active_id = previous_active;
        let _ = vault::save(config);
        return Err(SwitchFailure::new(SwitchFailureKind::Launch, e));
    }
    let launch_ms = t_launch.elapsed();

    if cfg!(debug_assertions) {
        eprintln!(
            "[perf-swap] switch_account: pre_checks={pre_ms:?}, stop_clients={stop_ms:?} (graceful_ms={:?}, force_ms={:?}, force_rounds={}, pids={}), capture/refresh={capture_ms:?}, restore_save={restore_save_ms:?}, launch={launch_ms:?}",
            stop_report.graceful_ms,
            stop_report.force_ms,
            stop_report.force_rounds,
            stop_report.pids
        );
    }
    Ok(())
}

pub fn set_nickname(config: &mut Config, id: Uuid, name: String) -> Result<(), String> {
    let nickname = name.trim();
    if nickname.chars().count() > 40 {
        return Err("Nickname must be 40 characters or fewer.".into());
    }
    let nickname = (!nickname.is_empty()).then(|| nickname.to_string());
    if let Some(candidate) = &nickname {
        if config
            .accounts
            .iter()
            .any(|a| a.id != id && a.display_name().eq_ignore_ascii_case(candidate))
        {
            return Err("An account with this name already exists.".into());
        }
    }
    let account = config
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or("Account no longer exists")?;
    let previous = account.nickname.clone();
    account.nickname = nickname;
    if let Err(e) = vault::save(config) {
        if let Some(account) = config.accounts.iter_mut().find(|a| a.id == id) {
            account.nickname = previous;
        }
        return Err(e);
    }
    Ok(())
}

pub fn remove_account(config: &mut Config, id: Uuid) -> Result<(), String> {
    let index = config
        .accounts
        .iter()
        .position(|a| a.id == id)
        .ok_or("Account no longer exists")?;
    let account = config.accounts.remove(index);
    let previous_active = config.active_id;
    if config.active_id == Some(id) {
        config.active_id = None;
    }
    if let Err(e) = vault::save(config) {
        config.accounts.insert(index, account);
        config.active_id = previous_active;
        return Err(e);
    }
    let _ = vault::remove_snapshot(account.vault_id);
    let _ = vault::remove_profile_icon(account.id);
    Ok(())
}

pub fn save_settings(
    config: &mut Config,
    use_deceive: bool,
    riot_exe: Option<String>,
    bundled_deceive: &Path,
) -> Result<(), String> {
    let riot_exe = riot_exe.filter(|s| !s.trim().is_empty());
    if let Some(path) = &riot_exe {
        if !is_exe(Path::new(path), "RiotClientServices.exe") {
            return Err("Choose a valid RiotClientServices.exe path.".into());
        }
    }
    let previous = (config.use_deceive, config.riot_exe.clone());
    config.use_deceive = use_deceive;
    config.riot_exe = riot_exe;
    if use_deceive && deceive_path(config, bundled_deceive).is_none() {
        (config.use_deceive, config.riot_exe) = previous;
        return Err("Bundled Deceive.exe was not found. Reinstall Swapper.".into());
    }
    if let Err(e) = vault::save(config) {
        (config.use_deceive, config.riot_exe) = previous;
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn session_fixture(root: &Path) -> (Config, vault::Snapshot) {
        let puuid = "00000000-0000-0000-0000-000000000001";
        let active = account(Some(puuid));
        let mut config = Config {
            active_id: Some(active.id),
            accounts: vec![active],
            ..Config::default()
        };
        config.accounts[0].vault_id =
            vault::save_snapshot(&vault::Snapshot { entries: vec![] }).unwrap();
        let bytes = vault::test_session_yaml(puuid);
        let path = root.join("Riot Games/Riot Client/Data/RiotGamesPrivateSettings.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &bytes).unwrap();
        let snapshot = vault::Snapshot {
            entries: vec![vault::Entry {
                relative: "client/private.yaml".into(),
                bytes,
            }],
        };
        (config, snapshot)
    }

    #[test]
    fn closed_client_refresh_saves_matching_snapshot() {
        vault::with_test_data(|root| {
            let (mut config, snapshot) = session_fixture(root);
            let old = config.accounts[0].vault_id;
            refresh_active_with_snapshot(&mut config, &Detection::NotRunning, Some(&snapshot))
                .unwrap();
            assert_ne!(config.accounts[0].vault_id, old);
            let saved = vault::read_snapshot(config.accounts[0].vault_id).unwrap();
            assert_eq!(saved.entries[0].bytes, snapshot.entries[0].bytes);
            assert_eq!(config.accounts[0].game_name.as_deref(), Some("Example"));
        });
    }

    #[test]
    fn closed_client_refresh_refuses_different_and_skips_unreadable_subject() {
        vault::with_test_data(|root| {
            let (mut config, mut snapshot) = session_fixture(root);
            let old = config.accounts[0].vault_id;
            config.accounts[0].puuid = Some("00000000-0000-0000-0000-000000000002".into());
            assert!(refresh_active_with_snapshot(
                &mut config,
                &Detection::NotRunning,
                Some(&snapshot)
            )
            .is_err());
            assert_eq!(config.accounts[0].vault_id, old);
            snapshot.entries[0].bytes = b"unreadable: [yaml".to_vec();
            refresh_active_with_snapshot(&mut config, &Detection::NotRunning, Some(&snapshot))
                .unwrap();
            assert_eq!(config.accounts[0].vault_id, old);
        });
    }

    #[test]
    fn live_identity_remains_authoritative_and_legacy_accounts_stay_untouched() {
        vault::with_test_data(|root| {
            let (mut config, snapshot) = session_fixture(root);
            let old = config.accounts[0].vault_id;
            let mismatch = Detection::Identified(identity("00000000-0000-0000-0000-000000000002"));
            assert!(refresh_active_with_snapshot(&mut config, &mismatch, Some(&snapshot)).is_err());
            assert_eq!(config.accounts[0].vault_id, old);
            let matching =
                Detection::Identified(identity(config.accounts[0].puuid.as_deref().unwrap()));
            let unreadable = vault::Snapshot { entries: vec![] };
            assert!(verified_live_identity(&config, &matching, Some(&unreadable)).unwrap());
            for detection in [Detection::NotReady, Detection::Unavailable] {
                assert!(verified_live_identity(&config, &detection, Some(&snapshot)).unwrap());
            }
            config.accounts[0].puuid = None;
            assert!(!refresh_active_with_snapshot(
                &mut config,
                &Detection::NotRunning,
                Some(&snapshot)
            )
            .unwrap());
            assert!(
                !refresh_active_with_snapshot(&mut config, &matching, Some(&snapshot)).unwrap()
            );
            assert_eq!(config.accounts[0].vault_id, old);
        });
    }

    #[test]
    fn closed_client_preflight_refuses_mismatch_before_any_vault_change() {
        vault::with_test_data(|root| {
            let (mut config, _) = session_fixture(root);
            assert!(verify_before_stop(&config, &Detection::NotRunning).is_ok());
            config.accounts[0].puuid = Some("00000000-0000-0000-0000-000000000002".into());
            assert!(verify_before_stop(&config, &Detection::NotRunning).is_err());
        });
    }

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
    fn matching_puuid_is_safe_to_refresh() {
        let active = account(Some("PUUID-A"));
        assert!(identity_safe_for(&active, &identity("PUUID-A")));
    }

    #[test]
    fn different_live_account_is_never_safe_to_refresh() {
        let active = account(Some("PUUID-A"));
        assert!(!identity_safe_for(&active, &identity("PUUID-B")));
    }

    #[test]
    fn legacy_account_is_not_safe_to_refresh_from_an_unproven_login() {
        let active = account(None);
        assert!(!identity_safe_for(&active, &identity("PUUID-A")));
    }

    #[test]
    fn same_riot_id_with_different_puuid_is_a_duplicate_conflict() {
        let mut saved = account(Some("OLD-PUUID"));
        saved.game_name = Some("Example".into());
        saved.tag_line = Some("ABC".into());
        let config = Config {
            accounts: vec![saved],
            ..Config::default()
        };

        assert!(conflicting_riot_id(&config, &identity("NEW-PUUID")));
        assert!(!conflicting_riot_id(&config, &identity("OLD-PUUID")));
    }

    #[test]
    fn refresh_active_with_snapshot_skips_when_no_active_or_unverified() {
        let active = account(Some("PUUID-A"));
        let mut config = Config {
            accounts: vec![active.clone()],
            active_id: None,
            ..Config::default()
        };
        let snapshot = vault::Snapshot { entries: vec![] };
        // No active account set: skips cleanly without touching vault or erroring
        assert!(refresh_active_with_snapshot(&mut config, &Detection::Identified(identity("PUUID-A")), Some(&snapshot)).is_ok());

        // Neither a live identity nor a readable offline subject: skips cleanly.
        config.active_id = Some(active.id);
        assert!(refresh_active_with_snapshot(&mut config, &Detection::NotRunning, Some(&snapshot)).is_ok());
    }
}
