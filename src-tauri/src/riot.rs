use crate::identity::{self, Detection, DetectedIdentity};
use crate::vault::{self, Account, Config, LaunchTarget};
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
    /// The running game's name, when the check saw which one.
    GameRunning(Option<&'static str>),
    ClientStillRunning,
    InspectionFailed(SnapshotError),
}

impl ProcessStateError {
    pub fn message(self) -> String {
        match self {
            Self::GameRunning(Some(game)) => format!("{game} is running. Close it first."),
            Self::GameRunning(None) => "Close your game first, then switch accounts.".into(),
            Self::ClientStillRunning => {
                "Riot Client is still open. Close it fully, then try again.".into()
            }
            Self::InspectionFailed(error) => {
                eprintln!("Process check failed: {error}");
                "Couldn't check for running games. Nothing changed \u{2014} try again.".into()
            }
        }
    }
}

pub(crate) fn ensure_no_game() -> Result<(), ProcessStateError> {
    match process::matching(GAMES) {
        Ok(running) => match running.first() {
            Some(game) => Err(ProcessStateError::GameRunning(Some(game_name(&game.image)))),
            None => Ok(()),
        },
        Err(error) => Err(ProcessStateError::InspectionFailed(error)),
    }
}

/// Display name for an image in [`GAMES`].
fn game_name(image: &str) -> &'static str {
    if image.to_ascii_uppercase().starts_with("VALORANT") { "VALORANT" } else { "League of Legends" }
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
        // The League client rarely closes on request; live swaps showed it needing the kill anyway.
        grace: std::time::Duration::from_millis(200),
        ..StopOptions::default()
    };
    match process::stop_images(CLIENTS, GAMES, options) {
        Ok(report) => Ok(report),
        Err(StopError::ForbiddenPresent) => Err(ProcessStateError::GameRunning(None)),
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
        .map_err(|e| { eprintln!("Launch failed for {}: {e}", path.display()); "Couldn't start the client. Try again.".to_string() })?;
    Ok(())
}

fn launch_selected(config: &Config, target: LaunchTarget, bundled_deceive: &Path) -> Result<(), String> {
    if config.use_deceive {
        let path = deceive_path(config, bundled_deceive)
            .ok_or("Deceive is missing. Reinstall Swapper.")?;
        // Deceive only proxies chat; the product itself is launched later through the Riot Client API.
        launch(&path, &[if target == LaunchTarget::Valorant { "valorant" } else { "lol" }])
    } else {
        let path =
            riot_path(config).ok_or("Riot Client wasn't found. Set its path in Settings.")?;
        match target.product() {
            Some(product) => launch(&path, &[&format!("--launch-product={product}"), "--launch-patchline=live"]),
            None => launch(&path, &[]),
        }
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
    let Some(live) = live_puuid(detection, snapshot) else {
        return Ok(false);
    };
    if active.puuid.as_deref() != Some(live.as_str()) {
        return Err(mismatch_message(config, active, &live));
    }
    Ok(true)
}

/// The live sign-in's PUUID: the API identity when known, else the captured session's subject.
fn live_puuid(detection: &Detection, snapshot: Option<&vault::Snapshot>) -> Option<String> {
    match detection {
        Detection::Identified(identity) => Some(identity.puuid.clone()),
        _ => snapshot.and_then(vault::Snapshot::puuid),
    }
}

/// Makes `active_id` follow the live sign-in without ever writing a session anywhere.
/// A live login that is another saved account becomes the active one; an unsaved
/// one detaches the active account so the normal "save your current sign-in" flow applies.
fn reconcile_active_with_live(config: &mut Config, live: &str) -> Result<(), String> {
    let active_puuid = config
        .active_id
        .and_then(|id| config.accounts.iter().find(|a| a.id == id))
        .and_then(|a| a.puuid.clone());
    if active_puuid.as_deref().is_none_or(|p| p == live) {
        return Ok(());
    }
    let owner = config
        .accounts
        .iter()
        .find(|a| a.puuid.as_deref() == Some(live))
        .map(|a| a.id);
    let previous = config.active_id;
    config.active_id = owner;
    if let Err(e) = vault::save(config) {
        config.active_id = previous;
        return Err(e);
    }
    match owner {
        Some(_) => Ok(()),
        None => Err(UNSAVED_LIVE_MESSAGE.into()),
    }
}

const UNSAVED_LIVE_MESSAGE: &str =
    "An account that isn't saved is signed in. Save it with Add account, then switch. Nothing was changed.";

/// Refuse an ownership mismatch before stopping any clients. Closed clients
/// have no API identity, so inspect a capture without modifying the live files.
fn verify_before_stop(config: &mut Config, detection: &Detection) -> Result<(), String> {
    if config.active_id.is_none() || !vault::live_session_exists()? {
        return Ok(());
    }
    let snapshot = if matches!(detection, Detection::Identified(_)) {
        None
    } else {
        Some(vault::capture()?)
    };
    if let Some(live) = live_puuid(detection, snapshot.as_ref()) {
        reconcile_active_with_live(config, &live)?;
    }
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

fn mismatch_message(config: &Config, active: &Account, live: &str) -> String {
    match config.accounts.iter().find(|a| a.puuid.as_deref() == Some(live)) {
        Some(live) => format!(
            "{} is signed in, not {}. Nothing was changed.",
            live.display_name(),
            active.display_name()
        ),
        None => "A different account is signed in. Nothing was changed.".into(),
    }
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
        .ok_or("That account was removed. Add it again to switch.")?;
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
    let path = riot_path(config).ok_or("Riot Client wasn't found. Set its path in Settings.")?;
    launch(&path, &[])
}

pub fn begin_add(config: &mut Config, detection: &Detection) -> Result<(), String> {
    ensure_no_game().map_err(|error| error.message())?;
    if config.active_id.is_none() && vault::live_session_exists()? {
        return Err("Save your current sign-in first, then add another account.".into());
    }
    let riot = riot_path(config).ok_or("Riot Client wasn't found. Set its path in Settings.")?;
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
                    if let Err(e) = vault::restore(&snapshot) {
                        eprintln!("Add-account rollback failed: {e}");
                        return Err(vault::ROLLBACK_FAILED.to_string());
                    }
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
        return Err("Couldn't read your sign-in. Make sure Riot Client is open.".into());
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
        return Err("That Riot ID is already saved differently. Remove one, then add again.".into());
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
        launch_target: None,
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
        ProcessStateError::GameRunning(_) => SwitchFailureKind::GameRunning,
        ProcessStateError::ClientStillRunning => SwitchFailureKind::ClientBusy,
        ProcessStateError::InspectionFailed(_) => SwitchFailureKind::InspectionFailed,
    };
    SwitchFailure::new(kind, error.message())
}

/// Puts the pre-switch live session back (or clears it when there was none).
/// A failure is logged by the vault and reported to the caller, never swallowed.
fn roll_back_live(previous: Option<&vault::Snapshot>) -> bool {
    match previous {
        Some(previous) => vault::restore(previous),
        None => vault::clear_live(),
    }
    .is_ok()
}

pub fn switch_account(
    config: &mut Config,
    id: Uuid,
    bundled_deceive: &Path,
    detection: &Detection,
    // Called once the instant refusals are out of the way, so a refused switch never announces itself.
    on_started: impl FnOnce(),
) -> Result<(), SwitchFailure> {
    let t_pre = Instant::now();
    // Restoring would overwrite a sign-in that was never saved (same rule as begin_add).
    if config.active_id.is_none() && vault::live_session_exists()? {
        return Err(SwitchFailure::new(SwitchFailureKind::Other, "Save your current sign-in first, then switch accounts."));
    }
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
            .ok_or_else(|| SwitchFailure::new(SwitchFailureKind::LauncherMissing, "Deceive is missing. Reinstall Swapper."))?;
    } else {
        riot_path(config).ok_or_else(|| SwitchFailure::new(SwitchFailureKind::LauncherMissing, "Riot Client wasn't found. Set its path in Settings."))?;
    }
    // Verify the live identity before Riot Client is closed, so a manual
    // Riot login change can never overwrite another account's saved session.
    verify_before_stop(config, detection)?;
    let pre_ms = t_pre.elapsed();
    on_started();

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
        eprintln!("Session restore failed: {e}");
        // restore() is all-or-nothing, so this rollback only matters if it was interrupted late.
        // The snapshot was already validated, so this is a disk/lock problem: Repair wouldn't help.
        let detail = if roll_back_live(previous.as_ref()) { e } else { vault::ROLLBACK_FAILED.to_string() };
        return Err(SwitchFailure::new(SwitchFailureKind::Other, detail));
    }
    let previous_active = config.active_id;
    config.active_id = Some(id);
    if let Err(e) = vault::save(config) {
        config.active_id = previous_active;
        if !roll_back_live(previous.as_ref()) {
            return Err(vault::ROLLBACK_FAILED.to_string().into());
        }
        return Err(e.into());
    }
    let restore_save_ms = t_restore.elapsed();

    let t_launch = Instant::now();
    if let Err(e) = launch_selected(config, config.launch_target_for(id), bundled_deceive) {
        let rolled_back = roll_back_live(previous.as_ref());
        config.active_id = previous_active;
        let _ = vault::save(config);
        let detail = if rolled_back { e } else { vault::ROLLBACK_FAILED.to_string() };
        return Err(SwitchFailure::new(SwitchFailureKind::Launch, detail));
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
        return Err("Nickname is too long. Keep it to 40 characters or fewer.".into());
    }
    let nickname = (!nickname.is_empty()).then(|| nickname.to_string());
    if let Some(candidate) = &nickname {
        if config
            .accounts
            .iter()
            .any(|a| a.id != id && a.display_name().eq_ignore_ascii_case(candidate))
        {
            return Err("That name is already used. Pick a different one.".into());
        }
    }
    let account = config
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or("That account was removed. Add it again to switch.")?;
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

/// The saved accounts in the order `ids` names them. `ids` must name every
/// saved account exactly once, so a stale list can never drop or duplicate one.
pub fn reordered(accounts: &[Account], ids: &[String]) -> Result<Vec<Account>, String> {
    let stale = || "Your account list changed. Reopen Accounts and try again.".to_string();
    if ids.len() != accounts.len() {
        return Err(stale());
    }
    let mut seen = std::collections::HashSet::new();
    ids.iter()
        .map(|id| {
            let id = Uuid::parse_str(id).map_err(|_| stale())?;
            if !seen.insert(id) {
                return Err(stale());
            }
            accounts.iter().find(|a| a.id == id).cloned().ok_or_else(stale)
        })
        .collect()
}

pub fn reorder_accounts(config: &mut Config, ids: &[String]) -> Result<(), String> {
    let next = reordered(&config.accounts, ids)?;
    let previous = std::mem::replace(&mut config.accounts, next);
    if let Err(e) = vault::save(config) {
        config.accounts = previous;
        return Err(e);
    }
    Ok(())
}

pub fn remove_account(config: &mut Config, id: Uuid) -> Result<(), String> {
    let index = config
        .accounts
        .iter()
        .position(|a| a.id == id)
        .ok_or("That account was removed. Add it again to switch.")?;
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
            return Err("That file isn't Riot Client. Pick RiotClientServices.exe.".into());
        }
    }
    let previous = (config.use_deceive, config.riot_exe.clone());
    config.use_deceive = use_deceive;
    config.riot_exe = riot_exe;
    if use_deceive && deceive_path(config, bundled_deceive).is_none() {
        (config.use_deceive, config.riot_exe) = previous;
        return Err("Deceive is missing. Reinstall Swapper.".into());
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
        config.accounts[0].vault_id = vault::save_snapshot(&snapshot).unwrap();
        (config, snapshot)
    }

    #[test]
    fn switch_refuses_to_overwrite_an_unsaved_sign_in() {
        vault::with_test_data(|root| {
            let (mut config, _) = session_fixture(root);
            config.active_id = None;
            let target = config.accounts[0].id;
            let live = root.join("Riot Games/Riot Client/Data/RiotGamesPrivateSettings.yaml");
            let before = std::fs::read(&live).unwrap();
            let error = switch_account(&mut config, target, Path::new("Deceive.exe"), &Detection::NotRunning, || panic!("a refused switch must not announce itself")).unwrap_err();
            assert!(format!("{error:?}").contains("Save your current sign-in first"));
            assert_eq!(std::fs::read(&live).unwrap(), before);
        });
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
            assert!(verify_before_stop(&mut config, &Detection::NotRunning).is_ok());
            config.accounts[0].puuid = Some("00000000-0000-0000-0000-000000000002".into());
            assert!(verify_before_stop(&mut config, &Detection::NotRunning).is_err());
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

    #[test]
    fn reordered_follows_the_given_ids() {
        let accounts = vec![account(None), account(None), account(None)];
        let ids: Vec<String> = [2, 0, 1].iter().map(|&i| accounts[i].id.to_string()).collect();
        let next = reordered(&accounts, &ids).unwrap();
        let got: Vec<Uuid> = next.iter().map(|a| a.id).collect();
        assert_eq!(got, vec![accounts[2].id, accounts[0].id, accounts[1].id]);
    }

    #[test]
    fn reordered_refuses_missing_extra_duplicate_and_unknown_ids() {
        let accounts = vec![account(None), account(None)];
        let a = accounts[0].id.to_string();
        let b = accounts[1].id.to_string();
        let unknown = Uuid::new_v4().to_string();
        for ids in [
            vec![a.clone()],
            vec![a.clone(), b.clone(), unknown.clone()],
            vec![a.clone(), a.clone()],
            vec![a.clone(), unknown],
            vec![a.clone(), "not-a-uuid".into()],
        ] {
            assert!(reordered(&accounts, &ids).is_err(), "{ids:?}");
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
            launch_target: None,
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

    // ---- swap e2e: refusals that must happen before any client is stopped ----
    //
    // Every switch_account test below is refused BEFORE `stop_clients`, so none
    // of them touches the real Riot processes of the machine running the tests.
    // If a regression let one reach `stop_clients`, it would kill real
    // Riot/League client processes on the developer machine.

    const PUUID_A: &str = "00000000-0000-0000-0000-000000000001";
    const PUUID_B: &str = "00000000-0000-0000-0000-000000000002";
    const PUUID_C: &str = "00000000-0000-0000-0000-000000000003";

    fn live_file(root: &Path) -> PathBuf {
        root.join("Riot Games/Riot Client/Data/RiotGamesPrivateSettings.yaml")
    }

    /// `session_fixture` (active account A, live session A) plus a saved target
    /// account B and a stand-in `Deceive.exe`, so the launcher check is
    /// deterministic on machines without Riot Client installed.
    fn two_accounts(root: &Path) -> (Config, Uuid, PathBuf) {
        let (mut config, _) = session_fixture(root);
        let mut target = account(Some(PUUID_B));
        target.vault_id = vault::save_snapshot(&vault::Snapshot {
            entries: vec![vault::Entry {
                relative: "client/private.yaml".into(),
                bytes: vault::test_session_yaml(PUUID_B),
            }],
        })
        .unwrap();
        let target_id = target.id;
        config.accounts.push(target);
        config.use_deceive = true;
        let deceive = root.join("Deceive.exe");
        std::fs::write(&deceive, b"stub").unwrap();
        (config, target_id, deceive)
    }

    fn assert_live_untouched(root: &Path, before: &[u8]) {
        assert_eq!(std::fs::read(live_file(root)).unwrap(), before);
    }

    // edge case 4
    #[test]
    fn switch_to_a_removed_account_is_refused_and_touches_nothing() {
        vault::with_test_data(|root| {
            let (mut config, _, deceive) = two_accounts(root);
            let before = std::fs::read(live_file(root)).unwrap();
            let error = switch_account(&mut config, Uuid::new_v4(), &deceive, &Detection::NotRunning, || panic!("a refused switch must not announce itself")).unwrap_err();
            assert_eq!(error.kind, SwitchFailureKind::AccountMissing);
            assert_live_untouched(root, &before);
        });
    }

    // edge case 5 (missing file)
    #[test]
    fn switch_with_a_missing_snapshot_fails_before_stopping_clients() {
        vault::with_test_data(|root| {
            let (mut config, target, deceive) = two_accounts(root);
            let before = std::fs::read(live_file(root)).unwrap();
            let active = config.active_id;
            let vault_id = config.accounts.iter().find(|a| a.id == target).unwrap().vault_id;
            vault::remove_snapshot(vault_id).unwrap();
            let error = switch_account(&mut config, target, &deceive, &Detection::NotRunning, || panic!("a refused switch must not announce itself")).unwrap_err();
            assert_eq!(error.kind, SwitchFailureKind::SessionRestore);
            assert_eq!(config.active_id, active, "active account must not change");
            assert_live_untouched(root, &before);
        });
    }

    // edge case 5 (unreadable / DPAPI-invalid / truncated blob)
    #[test]
    fn switch_with_a_corrupt_snapshot_fails_before_stopping_clients() {
        vault::with_test_data(|root| {
            let (mut config, target, deceive) = two_accounts(root);
            let before = std::fs::read(live_file(root)).unwrap();
            let vault_id = config.accounts.iter().find(|a| a.id == target).unwrap().vault_id;
            let blob = root.join("Swapper/vault").join(format!("{vault_id}.bin"));
            std::fs::write(&blob, b"not a dpapi blob").unwrap();
            let error = switch_account(&mut config, target, &deceive, &Detection::NotRunning, || panic!("a refused switch must not announce itself")).unwrap_err();
            assert_eq!(error.kind, SwitchFailureKind::SessionRestore);
            assert_live_untouched(root, &before);
            // Truncated real blob: protected bytes cut in half.
            let good = vault::save_snapshot(&vault::Snapshot { entries: vec![] }).unwrap();
            let bytes = std::fs::read(root.join("Swapper/vault").join(format!("{good}.bin"))).unwrap();
            std::fs::write(&blob, &bytes[..bytes.len() / 2]).unwrap();
            let error = switch_account(&mut config, target, &deceive, &Detection::NotRunning, || panic!("a refused switch must not announce itself")).unwrap_err();
            assert_eq!(error.kind, SwitchFailureKind::SessionRestore);
            assert_live_untouched(root, &before);
        });
    }

    // edge cases 5/26: a snapshot that decrypts but holds no sign-in is accepted pre-stop.
    #[test]
    fn a_saved_snapshot_without_a_sign_in_is_refused_before_stopping_clients() {
        vault::with_test_data(|root| {
            let (mut config, target, deceive) = two_accounts(root);
            let before = std::fs::read(live_file(root)).unwrap();
            let hollow = vault::save_snapshot(&vault::Snapshot { entries: vec![] }).unwrap();
            config.accounts.iter_mut().find(|a| a.id == target).unwrap().vault_id = hollow;
            assert!(vault::read_snapshot(hollow).is_err());
            let error = switch_account(&mut config, target, &deceive, &Detection::NotRunning, || panic!("a refused switch must not announce itself")).unwrap_err();
            assert_eq!(error.kind, SwitchFailureKind::SessionRestore);
            assert_eq!(error.detail, "Couldn't restore that sign-in. Try Repair account.");
            assert_live_untouched(root, &before);
        });
    }

    // edge cases 8/9
    #[test]
    fn switch_with_a_missing_launcher_fails_before_stopping_clients() {
        vault::with_test_data(|root| {
            let (mut config, target, _) = two_accounts(root);
            let missing = root.join("nowhere/Deceive.exe");
            // deceive_path also searches running processes and %LOCALAPPDATA%; skip on a machine that has one.
            if deceive_path(&config, &missing).is_some() {
                return;
            }
            let before = std::fs::read(live_file(root)).unwrap();
            let error = switch_account(&mut config, target, &missing, &Detection::NotRunning, || panic!("a refused switch must not announce itself")).unwrap_err();
            assert_eq!(error.kind, SwitchFailureKind::LauncherMissing);
            assert_live_untouched(root, &before);
        });
    }

    // edge cases 6/7: refusal kinds and wording
    #[test]
    fn process_refusals_map_to_distinct_switch_failures() {
        let cases = [
            (ProcessStateError::GameRunning(None), SwitchFailureKind::GameRunning),
            (ProcessStateError::GameRunning(Some("VALORANT")), SwitchFailureKind::GameRunning),
            (ProcessStateError::ClientStillRunning, SwitchFailureKind::ClientBusy),
        ];
        for (error, kind) in cases {
            assert_eq!(switch_failure(error).kind, kind);
        }
        let unreadable = switch_failure(ProcessStateError::InspectionFailed(SnapshotError { code: 5 }));
        assert_eq!(unreadable.kind, SwitchFailureKind::InspectionFailed);
        assert!(
            !unreadable.detail.to_lowercase().contains("close your game"),
            "an unreadable process table must never be reported as a running game"
        );
        assert!(unreadable.detail.contains("Nothing changed"));
        let named = switch_failure(ProcessStateError::GameRunning(Some(game_name("VALORANT-Win64-Shipping.exe"))));
        assert_eq!(named.detail, "VALORANT is running. Close it first.");
        assert_eq!(game_name("League of Legends.exe"), "League of Legends");
    }

    // edge cases 10/11: a different live login is refused whatever way it is detected.
    #[test]
    fn a_different_live_login_is_refused_for_every_detection_state() {
        vault::with_test_data(|root| {
            let (config, _, _) = two_accounts(root);
            let vault_before = config.accounts[0].vault_id;
            let mut config = config;
            // Live API says an unsaved account C is signed in while A is active.
            let identified = Detection::Identified(identity(PUUID_C));
            assert_eq!(verify_before_stop(&mut config, &identified).unwrap_err(), UNSAVED_LIVE_MESSAGE);
            // Offline: the live files hold C's session while A is active.
            for detection in [Detection::NotRunning, Detection::NotReady, Detection::Unavailable] {
                config.active_id = Some(config.accounts[0].id);
                std::fs::write(live_file(root), vault::test_session_yaml(PUUID_C)).unwrap();
                let offline_before = std::fs::read(live_file(root)).unwrap();
                assert_eq!(verify_before_stop(&mut config, &detection).unwrap_err(), UNSAVED_LIVE_MESSAGE);
                assert_live_untouched(root, &offline_before);
            }
            assert_eq!(config.accounts[0].vault_id, vault_before, "no snapshot may be overwritten");
            assert_eq!(config.active_id, None, "an unsaved live login detaches the active account");
            assert_eq!(vault::load().unwrap().active_id, None, "the detach is persisted");
        });
    }

    // edge case 11: an unreadable live subject never gets written under the active account.
    #[test]
    fn unverifiable_live_session_is_never_saved_over_the_active_account() {
        vault::with_test_data(|root| {
            let (mut config, _, _) = two_accounts(root);
            let vault_before = config.accounts[0].vault_id;
            let hollow = vault::Snapshot {
                entries: vec![vault::Entry { relative: "client/private.yaml".into(), bytes: b"psl: {}".to_vec() }],
            };
            for detection in [Detection::NotRunning, Detection::NotReady, Detection::Unavailable] {
                let saved = refresh_active_with_snapshot(&mut config, &detection, Some(&hollow)).unwrap();
                assert!(!saved);
                assert_eq!(config.accounts[0].vault_id, vault_before);
            }
        });
    }

    // edge case 12 (characterization, finding SW-05): a legacy active account (no PUUID) is
    // never verified, so another account's manual login is not detected and the next
    // switch overwrites it.
    #[test]
    fn legacy_active_account_skips_the_ownership_check() {
        vault::with_test_data(|root| {
            let (mut config, _, _) = two_accounts(root);
            config.accounts[0].puuid = None;
            std::fs::write(live_file(root), vault::test_session_yaml(PUUID_B)).unwrap();
            for detection in [Detection::NotRunning, Detection::Identified(identity(PUUID_B))] {
                assert!(verify_before_stop(&mut config, &detection).is_ok());
            }
        });
    }

    // edge case 23 / finding SW-02: after a crash between `vault::restore` and `vault::save`,
    // live = B but active_id = A. The live owner is adopted as active, with no snapshot written,
    // so switching works again. (Only the pre-stop check is called: a full switch would stop real clients.)
    #[test]
    fn half_switched_state_adopts_the_live_account_without_touching_any_snapshot() {
        vault::with_test_data(|root| {
            let (mut config, target, _) = two_accounts(root);
            let vaults: Vec<_> = config.accounts.iter().map(|a| a.vault_id).collect();
            std::fs::write(live_file(root), vault::test_session_yaml(PUUID_B)).unwrap();
            let before = std::fs::read(live_file(root)).unwrap();
            for detection in [Detection::NotRunning, Detection::Identified(identity(PUUID_B))] {
                config.active_id = Some(config.accounts[0].id);
                verify_before_stop(&mut config, &detection).unwrap();
                assert_eq!(config.active_id, Some(target));
                assert_eq!(vault::load().unwrap().active_id, Some(target), "the adoption is persisted");
            }
            assert_eq!(config.accounts.iter().map(|a| a.vault_id).collect::<Vec<_>>(), vaults);
            assert_live_untouched(root, &before);
        });
    }

    #[test]
    fn the_mismatch_message_names_both_accounts_when_the_live_one_is_saved() {
        vault::with_test_data(|root| {
            let (config, _, _) = two_accounts(root);
            let message = mismatch_message(&config, &config.accounts[0], PUUID_B);
            assert_eq!(message, format!("{} is signed in, not {}. Nothing was changed.", config.accounts[1].display_name(), config.accounts[0].display_name()));
            assert_eq!(mismatch_message(&config, &config.accounts[0], PUUID_C), "A different account is signed in. Nothing was changed.");
        });
    }

    // edge case 22: a failing state save rolls the account record and new blob back.
    #[test]
    fn failed_state_save_during_refresh_rolls_back_record_and_blob() {
        vault::with_test_data(|root| {
            let (mut config, snapshot) = session_fixture(root);
            let old = config.accounts[0].vault_id;
            let vault_dir = root.join("Swapper/vault");
            let blobs = || std::fs::read_dir(&vault_dir).unwrap().count();
            let before = blobs();
            // A directory where state.json must be renamed over makes `vault::save` fail.
            std::fs::create_dir_all(root.join("Swapper/state.json")).unwrap();
            let error = refresh_active_with_snapshot(&mut config, &Detection::NotRunning, Some(&snapshot)).unwrap_err();
            assert!(!error.is_empty());
            assert_eq!(config.accounts[0].vault_id, old, "account must keep pointing at its old snapshot");
            assert_eq!(blobs(), before, "the new blob must be removed again");
            assert!(vault::read_snapshot(old).is_ok());
        });
    }

    // edge cases 24/25: a refresh only ever replaces the ACTIVE account's snapshot, by PUUID.
    #[test]
    fn refresh_replaces_only_the_active_accounts_snapshot() {
        vault::with_test_data(|root| {
            let (mut config, _, _) = two_accounts(root);
            let other_before = config.accounts[1].vault_id;
            let fresh = vault::Snapshot {
                entries: vec![vault::Entry { relative: "client/private.yaml".into(), bytes: vault::test_session_yaml(PUUID_A) }],
            };
            assert!(refresh_active_with_snapshot(&mut config, &Detection::Identified(identity(PUUID_A)), Some(&fresh)).unwrap());
            assert_eq!(config.accounts[1].vault_id, other_before);
            assert_eq!(config.accounts[1].puuid.as_deref(), Some(PUUID_B));
            let saved = vault::read_snapshot(config.accounts[0].vault_id).unwrap();
            assert_eq!(saved.puuid().as_deref(), Some(PUUID_A));
        });
    }
}
