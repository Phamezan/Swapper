//! Notices a League Client that is running but not answering, so the sidebar can
//! offer a restart. Never restarts anything itself.
//!
//! Two signals only: the LCU API refusing the watcher's polls for a while, and
//! Windows reporting the client's window as hung. Both are plain reads (a
//! lockfile stat, the watcher's own poll result, `IsHungAppWindow`); nothing
//! touches the client process.
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};
use tauri::Emitter;

// shortcut: first guesses, tune from real reports of slow starts and stalls.
/// A client younger than this is allowed to be slow to answer.
const STARTUP_GRACE: Duration = Duration::from_secs(90);
/// The API must keep failing this long (after the grace) before it counts as dead.
const API_DEAD_AFTER: Duration = Duration::from_secs(40);
/// A hung window must stay hung this long.
const HUNG_AFTER: Duration = Duration::from_secs(10);
const TICK: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Ok,
    /// The API refuses requests although the client process is up.
    Dead,
    /// Windows reports the client window as not responding.
    Hung,
}

impl Health {
    fn code(self) -> u8 {
        self as u8
    }
    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Dead,
            2 => Self::Hung,
            _ => Self::Ok,
        }
    }
}

/// What one tick observed.
#[derive(Debug, Clone, Copy)]
pub struct Input {
    /// Age of LeagueClientUx when it runs with a lockfile; `None` otherwise.
    pub client_age: Option<Duration>,
    pub api_ok: bool,
    pub window_hung: bool,
    /// A game is running or Swapper itself is switching: never offer then.
    pub suppressed: bool,
}

#[derive(Default)]
pub struct Tracker {
    api_failing_since: Option<Instant>,
    hung_since: Option<Instant>,
}

impl Tracker {
    pub fn step(&mut self, now: Instant, input: Input) -> Health {
        let Some(age) = input.client_age.filter(|_| !input.suppressed) else {
            *self = Self::default();
            return Health::Ok;
        };
        // Failures inside the startup grace do not count towards the dead window.
        self.api_failing_since = if input.api_ok || age < STARTUP_GRACE {
            None
        } else {
            self.api_failing_since.or(Some(now))
        };
        self.hung_since = if input.window_hung {
            self.hung_since.or(Some(now))
        } else {
            None
        };
        let held = |since: Option<Instant>, limit| since.is_some_and(|at| now - at >= limit);
        if held(self.hung_since, HUNG_AFTER) {
            Health::Hung
        } else if held(self.api_failing_since, API_DEAD_AFTER) {
            Health::Dead
        } else {
            Health::Ok
        }
    }
}

static API_OK: AtomicBool = AtomicBool::new(true);
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The champion-select watcher reports each LCU poll here, so no extra request is made.
pub fn note_api(ok: bool) {
    API_OK.store(ok, Ordering::Relaxed);
}

#[tauri::command]
pub(crate) fn client_health() -> Health {
    Health::from_code(CURRENT.load(Ordering::Relaxed))
}

fn sample(app: &tauri::AppHandle) -> Input {
    use tauri::Manager;
    let none = Input {
        client_age: None,
        api_ok: true,
        window_hung: false,
        suppressed: false,
    };
    let Ok(clients) =
        crate::windows::process::matching(&["LeagueClientUx.exe", "LeagueClientUxRender.exe"])
    else {
        return none;
    };
    let Some(ux) = clients
        .iter()
        .find(|p| p.image.eq_ignore_ascii_case("LeagueClientUx.exe"))
    else {
        return none;
    };
    if !crate::lcu::lockfile_present() {
        return none;
    }
    let pids: Vec<u32> = clients.iter().map(|p| p.pid).collect();
    let switching = app
        .try_state::<crate::AppState>()
        .is_some_and(|state| state.switch_guard.is_switching());
    Input {
        client_age: crate::windows::process::process_age(ux.pid),
        api_ok: API_OK.load(Ordering::Relaxed),
        window_hung: crate::windows::process::has_hung_window(&pids),
        suppressed: switching || crate::riot::ensure_no_game().is_err(),
    }
}

pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tracker = Tracker::default();
        loop {
            tokio::time::sleep(TICK).await;
            let probe = app.clone();
            let Ok(input) = tauri::async_runtime::spawn_blocking(move || sample(&probe)).await
            else {
                continue;
            };
            let health = tracker.step(Instant::now(), input);
            if CURRENT.swap(health.code(), Ordering::Relaxed) != health.code() {
                let _ = app.emit("client_health", health);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);

    /// Runs `(seconds, input)` steps on one timeline and returns each answer.
    fn run(steps: &[(u64, Input)]) -> Vec<Health> {
        let (mut tracker, base) = (Tracker::default(), Instant::now());
        steps
            .iter()
            .map(|(at, input)| tracker.step(base + S * *at as u32, *input))
            .collect()
    }

    fn up(age: u64, api_ok: bool, window_hung: bool) -> Input {
        Input {
            client_age: Some(S * age as u32),
            api_ok,
            window_hung,
            suppressed: false,
        }
    }

    #[test]
    fn slow_startup_is_not_dead() {
        let out = run(&[
            (0, up(10, false, false)),
            (40, up(50, false, false)),
            (80, up(89, false, false)),
        ]);
        assert!(out.iter().all(|h| *h == Health::Ok));
    }

    #[test]
    fn brief_stall_after_startup_is_not_dead() {
        let out = run(&[
            (0, up(200, false, false)),
            (30, up(230, false, false)),
            (35, up(235, true, false)),
            (60, up(260, false, false)),
        ]);
        assert!(out.iter().all(|h| *h == Health::Ok));
    }

    #[test]
    fn truly_dead_api_is_reported_after_forty_seconds_and_clears_on_recovery() {
        let out = run(&[
            (0, up(200, false, false)),
            (39, up(239, false, false)),
            (40, up(240, false, false)),
            (45, up(245, true, false)),
        ]);
        assert_eq!(out, [Health::Ok, Health::Ok, Health::Dead, Health::Ok]);
    }

    #[test]
    fn failures_count_from_the_end_of_the_grace() {
        // Failing since boot: only 20 s past the grace at t=30, so not dead yet.
        let out = run(&[
            (0, up(80, false, false)),
            (10, up(90, false, false)),
            (30, up(110, false, false)),
            (50, up(130, false, false)),
        ]);
        assert_eq!(out, [Health::Ok, Health::Ok, Health::Ok, Health::Dead]);
    }

    #[test]
    fn hung_window_needs_ten_continuous_seconds() {
        let out = run(&[
            (0, up(30, true, true)),
            (9, up(39, true, true)),
            (10, up(40, true, true)),
            (12, up(42, true, false)),
            (14, up(44, true, true)),
            (20, up(50, true, true)),
        ]);
        assert_eq!(
            out,
            [Health::Ok, Health::Ok, Health::Hung, Health::Ok, Health::Ok, Health::Ok]
        );
    }

    #[test]
    fn nothing_is_offered_in_a_game_or_while_swapper_switches() {
        let quiet = Input {
            suppressed: true,
            ..up(500, false, true)
        };
        // The suppressed tick resets the clocks, so the next bad tick starts over.
        let out = run(&[(0, up(500, false, true)), (60, quiet), (61, up(561, false, true))]);
        assert_eq!(out, [Health::Ok, Health::Ok, Health::Ok]);
    }

    #[test]
    fn no_client_is_always_ok() {
        let gone = Input {
            client_age: None,
            ..up(0, false, true)
        };
        assert_eq!(run(&[(0, gone), (100, gone)]), [Health::Ok, Health::Ok]);
    }
}
