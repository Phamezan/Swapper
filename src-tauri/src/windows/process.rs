//! Process snapshot, graceful close, exit wait and force-kill helpers.
//!
//! The account-switch shutdown path used to spawn one visible `taskkill`
//! console per image (twice), then burn a fixed six second grace wait on
//! processes that could never receive a close signal. This module keeps the
//! mechanics generic so `crate::riot` only carries Riot policy.
//!
//! Enumeration is fallible on purpose: callers that guard the live Riot
//! session must treat "the process table could not be read" as a refusal to
//! proceed, never as "nothing is running".

use std::cell::Cell;
use std::collections::HashSet;
use std::fmt;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, HWND, LPARAM, TRUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{OpenProcess, WaitForMultipleObjects};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowThreadProcessId, PostMessageW, GW_OWNER, SC_CLOSE, WM_CLOSE,
    WM_SYSCOMMAND,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// `WaitForMultipleObjects` accepts at most 64 objects per call.
const MAX_WAIT_OBJECTS: usize = 64;
/// Poll cadence for processes whose handle cannot be opened.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// `Process32FirstW` reports this when the snapshot holds no processes.
const ERROR_NO_MORE_FILES: u32 = 18;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRef {
    pub pid: u32,
    pub image: String,
}

/// The process table could not be inspected. Callers that guard the live
/// session must fail closed when this happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotError {
    pub code: u32,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "process enumeration failed (win32 error {})", self.code)
    }
}

impl std::error::Error for SnapshotError {}

/// Owned `HANDLE`: closed on every exit path (success, timeout, error) so
/// `wait_for_exit` cannot leak handles when a process ignores the wait.
///
/// The counter exists for tests only and proves the invariant without
/// depending on the process-wide handle count.
#[cfg(test)]
static LIVE_HANDLES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
fn live_handle_count() -> usize {
    LIVE_HANDLES.load(std::sync::atomic::Ordering::SeqCst)
}

#[repr(transparent)]
struct OwnedHandle(HANDLE);

impl OwnedHandle {
    /// Takes ownership of a raw handle. `None` for the two failure sentinels
    /// (`NULL` and `INVALID_HANDLE_VALUE`), which must never be closed.
    fn from_raw(handle: HANDLE) -> Option<Self> {
        if handle.is_null() || handle as isize == -1 {
            return None;
        }
        #[cfg(test)]
        LIVE_HANDLES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Some(Self(handle))
    }

    fn as_raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
        #[cfg(test)]
        LIVE_HANDLES.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// One `CreateToolhelp32Snapshot` pass over the process table. Far cheaper
/// than a `sysinfo` refresh because nothing but name and PID is read.
///
/// Returns either the complete table or an error: a `Process32FirstW` or
/// `Process32NextW` failure other than `ERROR_NO_MORE_FILES` aborts the pass,
/// so a partially enumerated table is never reported as `Ok`.
pub fn snapshot() -> Result<Vec<ProcessRef>, SnapshotError> {
    let mut out = Vec::new();
    unsafe {
        let handle = OwnedHandle::from_raw(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0))
            .ok_or_else(|| SnapshotError { code: GetLastError() })?;
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut first = true;
        while advance(handle.as_raw(), &mut entry, first)? {
            first = false;
            out.push(ProcessRef {
                pid: entry.th32ProcessID,
                image: utf16(&entry.szExeFile),
            });
        }
    }
    Ok(out)
}

/// One `Process32FirstW` / `Process32NextW` step: `Ok(true)` means `entry`
/// holds the next process, `Ok(false)` means the table ended normally
/// (`ERROR_NO_MORE_FILES`), and `Err` means enumeration itself failed — a
/// failure must never be read as a clean end of the table.
unsafe fn advance(
    handle: HANDLE,
    entry: &mut PROCESSENTRY32W,
    first: bool,
) -> Result<bool, SnapshotError> {
    let advanced = if first {
        Process32FirstW(handle, entry)
    } else {
        Process32NextW(handle, entry)
    };
    if advanced != 0 {
        return Ok(true);
    }
    let code = GetLastError();
    if code == ERROR_NO_MORE_FILES {
        return Ok(false);
    }
    Err(SnapshotError { code })
}

/// Processes whose image base name matches any of `names` (case-insensitive).
pub fn matching(names: &[&str]) -> Result<Vec<ProcessRef>, SnapshotError> {
    Ok(snapshot()?
        .into_iter()
        .filter(|process| {
            names
                .iter()
                .any(|name| process.image.eq_ignore_ascii_case(name))
        })
        .collect())
}

pub fn any_running(names: &[&str]) -> Result<bool, SnapshotError> {
    Ok(!matching(names)?.is_empty())
}

/// Image base name of a PID, from one snapshot pass.
pub fn image_for_pid(pid: u32) -> Result<Option<String>, SnapshotError> {
    Ok(snapshot()?
        .into_iter()
        .find(|process| process.pid == pid)
        .map(|process| process.image))
}

/// Distinct image names, preserving first-seen casing. Renderer processes
/// with the same name collapse into a single kill.
pub fn unique_images(processes: &[ProcessRef]) -> Vec<String> {
    let mut images: Vec<String> = Vec::new();
    for process in processes {
        if !images
            .iter()
            .any(|seen| seen.eq_ignore_ascii_case(&process.image))
        {
            images.push(process.image.clone());
        }
    }
    images
}

thread_local! {
    static WINDOW_HITS: Cell<usize> = const { Cell::new(0) };
    static POST_CLOSE: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "system" fn visit_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let target = lparam as u32;
    let mut owner_pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut owner_pid);
    if owner_pid != target {
        return TRUE;
    }
    // Owned windows are dialogs, not the application root.
    if !GetWindow(hwnd, GW_OWNER).is_null() {
        return TRUE;
    }
    WINDOW_HITS.with(|hits| hits.set(hits.get() + 1));
    if POST_CLOSE.with(|post| post.get()) {
        PostMessageW(hwnd, WM_SYSCOMMAND, SC_CLOSE as usize, 0);
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
    TRUE
}

fn visit(pid: u32, post_close: bool) -> usize {
    if pid == 0 {
        return 0;
    }
    WINDOW_HITS.with(|hits| hits.set(0));
    POST_CLOSE.with(|post| post.set(post_close));
    unsafe { EnumWindows(Some(visit_window), pid as LPARAM) };
    WINDOW_HITS.with(|hits| hits.get())
}

/// How many top-level windows a process owns. Zero means no graceful close
/// signal can ever reach it, so callers skip the grace wait.
pub fn count_top_level_windows(pid: u32) -> usize {
    visit(pid, false)
}

/// Ask every top-level window of every PID to close. Returns how many windows
/// were reached; `0` means the graceful phase should be skipped entirely.
pub fn post_wm_close(pids: &[u32]) -> usize {
    pids.iter().map(|&pid| visit(pid, true)).sum()
}

/// Wait until every PID has exited, returning as soon as the last one does.
///
/// Real process handles are used where possible; PIDs whose handle cannot be
/// opened (access denied) fall back to snapshot polling. Every handle this
/// function opens is owned by [`OwnedHandle`] and therefore closed on success,
/// timeout and error alike.
pub fn wait_for_exit(pids: &[u32], timeout: Duration) -> bool {
    let mut pending: Vec<u32> = Vec::new();
    for &pid in pids {
        if pid != 0 && !pending.contains(&pid) {
            pending.push(pid);
        }
    }
    if pending.is_empty() {
        return true;
    }

    let mut handles: Vec<OwnedHandle> = Vec::new();
    let mut poll: Vec<u32> = Vec::new();
    for pid in &pending {
        let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, *pid) };
        match OwnedHandle::from_raw(handle) {
            Some(handle) => handles.push(handle),
            // A failed open is ambiguous: access denied (alive) or gone.
            None => poll.push(*pid),
        }
    }
    if !poll.is_empty() {
        if let Ok(still_alive) = alive(&poll) {
            poll = still_alive;
        }
    }

    let deadline = Instant::now() + timeout;
    loop {
        if handles.is_empty() && poll.is_empty() {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        if !handles.is_empty() {
            let end = handles.len().min(MAX_WAIT_OBJECTS);
            let batch: Vec<HANDLE> = handles[..end].iter().map(OwnedHandle::as_raw).collect();
            let status = unsafe {
                WaitForMultipleObjects(batch.len() as u32, batch.as_ptr(), TRUE, wait_ms(remaining))
            };
            if status == WAIT_OBJECT_0 {
                // Dropping closes these handles; the rest keep waiting.
                handles.drain(..end);
                continue;
            }
            if status != WAIT_TIMEOUT {
                // WAIT_FAILED: stop trusting handles for the remainder.
                handles.clear();
                poll.extend(pending.iter().copied());
                poll.sort_unstable();
                poll.dedup();
                if let Ok(still_alive) = alive(&poll) {
                    poll = still_alive;
                }
                continue;
            }
        }

        if !poll.is_empty() {
            // An unreadable table keeps the conservative "still alive" answer.
            if let Ok(table) = snapshot() {
                let still_alive: HashSet<u32> = table.into_iter().map(|p| p.pid).collect();
                poll.retain(|pid| still_alive.contains(pid));
            }
            if poll.is_empty() && handles.is_empty() {
                return true;
            }
            thread::sleep(remaining.min(POLL_INTERVAL));
        }
    }
    false
}

fn alive(pids: &[u32]) -> Result<Vec<u32>, SnapshotError> {
    let wanted: HashSet<u32> = pids.iter().copied().collect();
    Ok(snapshot()?
        .into_iter()
        .map(|process| process.pid)
        .filter(|pid| wanted.contains(pid))
        .collect())
}

fn wait_ms(remaining: Duration) -> u32 {
    remaining.as_millis().min(u128::from(u32::MAX)) as u32
}

/// Terminate every image in `images` concurrently (one `taskkill` per unique
/// image, all spawned at once). Every spawned process is joined before this
/// returns, so no kill can outlive its caller. `/T` keeps descendant
/// processes handled.
pub fn force_kill(images: &[String]) {
    if images.is_empty() {
        return;
    }
    let mut children = Vec::with_capacity(images.len());
    for image in images {
        let mut command = Command::new("taskkill");
        command
            .args(["/F", "/T", "/IM", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        if let Ok(child) = command.spawn() {
            children.push(child);
        }
    }
    for mut child in children {
        let _ = child.wait();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StopOptions {
    /// How long to wait for processes that were asked to close themselves.
    pub grace: Duration,
    /// How long to wait after a force kill before re-checking.
    pub force_wait: Duration,
    /// Force-kill rounds before giving up.
    pub max_force_rounds: u32,
}

impl Default for StopOptions {
    fn default() -> Self {
        Self {
            grace: Duration::from_millis(1500),
            force_wait: Duration::from_millis(2000),
            max_force_rounds: 2,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StopError {
    /// A process from `forbidden` is running; nothing may be terminated.
    ForbiddenPresent,
    /// Images that survived every force-kill round.
    StillRunning(Vec<String>),
    /// The process table could not be read, so neither the safety check nor
    /// the target list can be trusted. Nothing was assumed and nothing is
    /// reported as stopped.
    EnumerationFailed(SnapshotError),
}

#[derive(Debug, Default)]
pub struct StopReport {
    /// Unique images that were found running.
    pub images: Vec<String>,
    pub pids: usize,
    /// PIDs that actually received a graceful close request.
    pub graceful_delivered: usize,
    pub graceful_ms: Duration,
    pub force_ms: Duration,
    pub total_ms: Duration,
    pub force_rounds: u32,
}

/// Stop every process matching `targets`, refusing to run if anything in
/// `forbidden` is alive.
///
/// 1. Every inspection — the initial one, the one after the graceful close,
///    and the one before each force-kill round — re-checks `forbidden`, so a
///    game that starts while the stop is in flight aborts the stop instead of
///    being filtered away with the targets.
/// 2. Processes are asked to close themselves — but only when they own a
///    top-level window, because a windowless process cannot receive `WM_CLOSE`
///    and would otherwise cost a full grace timeout.
/// 3. Whatever survives is force-killed in parallel and waited on with real
///    process handles, so there is no fixed sleep and no serial `taskkill`.
///
/// If the process table cannot be read at any point, this returns
/// [`StopError::EnumerationFailed`] instead of reporting success: an
/// unreadable process list is never treated as an empty one.
pub fn stop_images(
    targets: &[&str],
    forbidden: &[&str],
    options: StopOptions,
) -> Result<StopReport, StopError> {
    stop_images_with(snapshot, targets, forbidden, options)
}

/// Fail the stop as soon as `processes` contains anything from `forbidden`.
/// Called on every fresh inspection: `stop_images` waits up to ~1.5 s for the
/// graceful close, and a League/Valorant process can start inside that window
/// before a `/T` force kill would sweep up its descendants.
fn ensure_forbidden_absent(
    processes: &[ProcessRef],
    forbidden: &[&str],
) -> Result<(), StopError> {
    let hit = processes.iter().any(|process| {
        forbidden
            .iter()
            .any(|name| process.image.eq_ignore_ascii_case(name))
    });
    if hit {
        Err(StopError::ForbiddenPresent)
    } else {
        Ok(())
    }
}

/// [`stop_images`] with the inspection step injected, so tests can prove that
/// an enumeration failure aborts instead of degrading into "nothing running",
/// and that a forbidden process appearing mid-stop is caught.
fn stop_images_with<F>(
    mut inspect: F,
    targets: &[&str],
    forbidden: &[&str],
    options: StopOptions,
) -> Result<StopReport, StopError>
where
    F: FnMut() -> Result<Vec<ProcessRef>, SnapshotError>,
{
    let started = Instant::now();
    let mut report = StopReport::default();

    let mut processes = inspect().map_err(StopError::EnumerationFailed)?;
    ensure_forbidden_absent(&processes, forbidden)?;
    processes.retain(|process| {
        targets
            .iter()
            .any(|name| process.image.eq_ignore_ascii_case(name))
    });
    if processes.is_empty() {
        report.total_ms = started.elapsed();
        return Ok(report);
    }

    report.images = unique_images(&processes);
    report.pids = processes.len();

    let mut delivered: Vec<u32> = Vec::new();
    for image in &report.images {
        let pids: Vec<u32> = processes
            .iter()
            .filter(|process| process.image.eq_ignore_ascii_case(image))
            .map(|process| process.pid)
            .collect();
        if post_wm_close(&pids) > 0 {
            delivered.extend(pids);
        }
    }
    report.graceful_delivered = delivered.len();
    if !delivered.is_empty() {
        let phase = Instant::now();
        let _ = wait_for_exit(&delivered, options.grace);
        report.graceful_ms = phase.elapsed();
    }

    let phase = Instant::now();
    let mut rounds = 0u32;
    loop {
        let alive_processes = inspect().map_err(StopError::EnumerationFailed)?;
        ensure_forbidden_absent(&alive_processes, forbidden)?;
        let mut alive_processes = alive_processes;
        alive_processes.retain(|process| {
            targets
                .iter()
                .any(|name| process.image.eq_ignore_ascii_case(name))
        });
        if alive_processes.is_empty() {
            break;
        }
        rounds += 1;
        if rounds > options.max_force_rounds {
            report.force_ms = phase.elapsed();
            report.force_rounds = rounds - 1;
            report.total_ms = started.elapsed();
            return Err(StopError::StillRunning(unique_images(&alive_processes)));
        }
        force_kill(&unique_images(&alive_processes));
        let pids: Vec<u32> = alive_processes
            .iter()
            .map(|process| process.pid)
            .collect();
        let _ = wait_for_exit(&pids, options.force_wait);
    }
    report.force_ms = phase.elapsed();
    report.force_rounds = rounds;
    report.total_ms = started.elapsed();
    Ok(report)
}

fn utf16(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own_image() -> String {
        std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .expect("test binary has a file name")
    }

    #[test]
    fn snapshot_finds_this_process() {
        let pid = std::process::id();
        let processes = snapshot().expect("snapshot");
        let own = processes
            .iter()
            .find(|process| process.pid == pid)
            .expect("own pid missing from snapshot");
        assert!(own.image.eq_ignore_ascii_case(&own_image()));
    }

    #[test]
    fn unique_images_deduplicates_case_insensitively() {
        let processes = vec![
            ProcessRef { pid: 1, image: "RiotClientUxRender.exe".into() },
            ProcessRef { pid: 2, image: "riotclientuxrender.EXE".into() },
            ProcessRef { pid: 3, image: "RiotClientUx.exe".into() },
        ];
        assert_eq!(
            unique_images(&processes),
            vec![
                "RiotClientUxRender.exe".to_string(),
                "RiotClientUx.exe".to_string()
            ]
        );
    }

    #[test]
    fn window_lookup_ignores_unknown_pids() {
        assert_eq!(count_top_level_windows(u32::MAX), 0);
        assert_eq!(post_wm_close(&[0, u32::MAX]), 0);
    }

    #[test]
    fn wait_returns_immediately_for_nothing_and_times_out_for_self() {
        assert!(wait_for_exit(&[], Duration::from_millis(50)));
        assert!(!wait_for_exit(&[std::process::id()], Duration::from_millis(100)));
    }

    /// Regression test for the handle leak: every path through
    /// `wait_for_exit` (timeout, success, empty input) must release the
    /// handles it opened, because Riot processes that ignore `WM_CLOSE`
    /// routinely hit the timeout path.
    #[test]
    fn wait_for_exit_closes_every_handle_on_timeout_and_success() {
        // Other tests in this module also open handles transiently, hence the
        // small allowance; a leak of even a few iterations is far larger.
        const ALLOWANCE: usize = 4;
        let baseline = live_handle_count();

        for _ in 0..50 {
            assert!(!wait_for_exit(&[std::process::id()], Duration::from_millis(2)));
        }
        assert!(
            live_handle_count() <= baseline + ALLOWANCE,
            "handles leaked on the timeout path: {} -> {}",
            baseline,
            live_handle_count()
        );

        assert!(wait_for_exit(&[], Duration::from_millis(5)));
        assert!(wait_for_exit(&[0], Duration::from_millis(5)));

        let mut command = Command::new("cmd");
        command
            .args(["/c", "ping", "-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        let child = command.spawn().expect("spawn wait fixture");
        let pid = child.id();
        let reaper = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
        });
        assert!(wait_for_exit(&[pid], Duration::from_secs(15)));
        reaper.join().expect("reaper thread");

        assert!(
            live_handle_count() <= baseline + ALLOWANCE,
            "handles leaked on the success path: {} -> {}",
            baseline,
            live_handle_count()
        );
    }

    #[test]
    fn stopping_an_absent_image_is_a_no_op() {
        let report = stop_images(
            &["SwapperBenchNotRunning.exe"],
            &[],
            StopOptions::default(),
        )
        .expect("absent target");
        assert!(report.images.is_empty());
        assert_eq!(report.force_rounds, 0);
        assert_eq!(report.graceful_delivered, 0);
    }

    #[test]
    fn a_forbidden_process_blocks_the_stop() {
        let own = own_image();
        let error = stop_images(
            &["SwapperBenchNotRunning.exe"],
            &[own.as_str()],
            StopOptions::default(),
        )
        .expect_err("forbidden process is running");
        assert_eq!(error, StopError::ForbiddenPresent);
    }

    /// An unreadable process table must never be read as "nothing running":
    /// even with no possible targets, the refusal is what protects the live
    /// Riot session.
    #[test]
    fn enumeration_failure_fails_closed() {
        let failure = SnapshotError { code: 5 };
        let error = stop_images_with(
            || Err(failure),
            &["SwapperBenchNotRunning.exe"],
            &[],
            StopOptions::default(),
        )
        .expect_err("inspection failed");
        assert_eq!(error, StopError::EnumerationFailed(failure));
    }

    #[test]
    fn a_successful_empty_inspection_is_still_a_no_op() {
        let report = stop_images_with(
            || Ok(Vec::new()),
            &["SwapperBenchNotRunning.exe"],
            &[],
            StopOptions::default(),
        )
        .expect("empty table inspected");
        assert!(report.images.is_empty());
        assert_eq!(report.force_rounds, 0);
    }

    /// A false `Process32NextW` is only "end of table" for
    /// `ERROR_NO_MORE_FILES`; any other failure must surface as an error
    /// instead of silently truncating the table into a partial `Ok`.
    #[test]
    fn a_failed_enumeration_step_is_an_error_not_the_end_of_the_table() {
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let invalid_handle = -1isize as HANDLE;

        let step = unsafe { advance(invalid_handle, &mut entry, false) };
        let error = step.expect_err("a failing step must not read as end of table");
        assert_ne!(error.code, ERROR_NO_MORE_FILES);
    }

    /// The stop waits for the graceful close before force killing. A game
    /// that starts inside that window must abort the stop at the next
    /// inspection, before any `taskkill /F /T /IM` runs — `/T` would take
    /// its descendants with it.
    #[test]
    fn a_game_starting_during_grace_blocks_the_force_kill() {
        let target = ProcessRef {
            pid: 0xFFFF_FFFE,
            image: "SwapperBenchTarget.exe".into(),
        };
        let started_game = ProcessRef {
            pid: 0xFFFF_FFFD,
            image: "League of Legends.exe".into(),
        };
        let mut inspections = 0usize;

        let result = stop_images_with(
            || {
                inspections += 1;
                if inspections == 1 {
                    Ok(vec![target.clone()])
                } else {
                    Ok(vec![target.clone(), started_game.clone()])
                }
            },
            &["SwapperBenchTarget.exe"],
            &["League of Legends.exe"],
            StopOptions {
                grace: Duration::from_millis(20),
                force_wait: Duration::from_millis(20),
                max_force_rounds: 2,
            },
        );

        assert_eq!(
            result.expect_err("forbidden process appeared mid-stop"),
            StopError::ForbiddenPresent
        );
        assert_eq!(
            inspections, 2,
            "must stop at the post-grace inspection, before any force round"
        );
    }
}
