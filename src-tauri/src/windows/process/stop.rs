use super::{
    force_kill, post_wm_close, snapshot, terminate, unique_images, wait_for_exit, ProcessRef,
    SnapshotError,
};
use std::time::{Duration, Instant};

/// Whether `image` should be asked to close itself before being force killed.
fn wants_graceful(image: &str, force_only: &[&str]) -> bool {
    !force_only
        .iter()
        .any(|name| image.eq_ignore_ascii_case(name))
}

#[derive(Clone, Copy, Debug)]
pub struct StopOptions {
    /// How long to wait for processes that were asked to close themselves.
    pub grace: Duration,
    /// How long to wait after a force kill before re-checking.
    pub force_wait: Duration,
    /// Force-kill rounds before giving up.
    pub max_force_rounds: u32,
    /// Images that are force killed without a graceful close, because they
    /// are known to ignore or merely hide on `WM_CLOSE`.
    pub force_only: &'static [&'static str],
}

impl Default for StopOptions {
    fn default() -> Self {
        Self {
            grace: Duration::from_millis(1500),
            force_wait: Duration::from_millis(2000),
            max_force_rounds: 2,
            force_only: &[],
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
///    visible top-level window, because a windowless or tray-only process
///    does not act on `WM_CLOSE`
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
fn ensure_forbidden_absent(processes: &[ProcessRef], forbidden: &[&str]) -> Result<(), StopError> {
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
    for image in report
        .images
        .iter()
        .filter(|image| wants_graceful(image, options.force_only))
    {
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
        let pids: Vec<u32> = alive_processes.iter().map(|process| process.pid).collect();
        let unkillable = terminate(&pids);
        if !unkillable.is_empty() {
            let fallback: Vec<ProcessRef> = alive_processes
                .iter()
                .filter(|process| unkillable.contains(&process.pid))
                .cloned()
                .collect();
            force_kill(&unique_images(&fallback));
        }
        let _ = wait_for_exit(&pids, options.force_wait);
    }
    report.force_ms = phase.elapsed();
    report.force_rounds = rounds;
    report.total_ms = started.elapsed();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::super::tests::own_image;
    use super::*;

    #[test]
    fn force_only_images_skip_the_graceful_close() {
        let force_only = &["RiotClientServices.exe", "Riot Client.exe"];
        assert!(!wants_graceful("riot client.exe", force_only));
        assert!(!wants_graceful("RiotClientServices.exe", force_only));
        assert!(wants_graceful("LeagueClientUx.exe", force_only));
        assert!(wants_graceful("Riot Client.exe", &[]));
    }

    #[test]
    fn stopping_an_absent_image_is_a_no_op() {
        let report = stop_images(&["SwapperBenchNotRunning.exe"], &[], StopOptions::default())
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
                force_only: &[],
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
