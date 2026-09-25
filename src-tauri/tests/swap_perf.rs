//! Before/after measurements for the account-switch shutdown path.
//!
//! Run in release mode:
//! `cargo test --release --test swap_perf -- --ignored --nocapture`
//!
//! Every fixture is a renamed copy of a stock Windows binary (never a real
//! Riot image), so no measurement can touch the Riot processes or the vault.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use swapper_lib::windows::process::{self, StopOptions, StopReport};
use sysinfo::{ProcessesToUpdate, System, UpdateKind};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Only used to measure how expensive the old lookup was; never spawned.
const CLIENTS: &[&str] = &[
    "RiotClientServices.exe",
    "RiotClientUx.exe",
    "RiotClientUxRender.exe",
    "LeagueClient.exe",
    "LeagueClientUx.exe",
    "LeagueClientUxRender.exe",
    "Deceive.exe",
];
/// Never spawned either; the old code checked this before every kill.
const GAMES: &[&str] = &[
    "League of Legends.exe",
    "LeagueofLegends.exe",
    "VALORANT-Win64-Shipping.exe",
];

const WINDOWED_IMAGE: &str = "SwapperBenchWindowed.exe";
const HEADLESS_IMAGE: &str = "SwapperBenchHeadless.exe";

fn fixture(name: &str, source: &Path) -> PathBuf {
    let dir = std::env::temp_dir().join("swapper-bench");
    fs::create_dir_all(&dir).expect("fixture dir");
    let target = dir.join(name);
    fs::copy(source, &target).expect("copy fixture");
    target
}

fn spawn(path: &Path, args: &[&str], no_window: bool) -> Child {
    let mut command = Command::new(path);
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if no_window {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().expect("spawn fixture")
}

/// Spawn a GUI fixture and wait until its window actually exists, so the
/// graceful phase is measured instead of racing window creation.
fn spawn_windowed(path: &Path) -> Child {
    let child = spawn(path, &[], false);
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if process::count_top_level_windows(pid) > 0 {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    child
}

fn reap(mut children: Vec<Child>) {
    for child in &mut children {
        let _ = child.kill();
        let _ = child.wait();
    }
}

// --------------------------------------------------------------------------
// Old algorithm, ported verbatim from the pre-optimisation riot.rs:
// sysinfo lookup, serial `taskkill` per name (visible consoles), a six second
// grace window polled at 250 ms, a force pass, then a fixed 300 ms sleep.
// --------------------------------------------------------------------------

fn legacy_named(names: &[&str]) -> Vec<String> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    system
        .processes()
        .values()
        .filter_map(|process| {
            let name = process.name().to_string_lossy().to_string();
            names
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
                .then_some(name)
        })
        .collect()
}

fn legacy_taskkill(args: &[&str]) {
    let _ = Command::new("taskkill")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn legacy_stop(images: &[String]) -> Result<Duration, String> {
    let started = Instant::now();
    let wanted: Vec<&str> = images.iter().map(String::as_str).collect();
    let _ = legacy_named(GAMES); // old ensure_no_game()
    let names = legacy_named(&wanted);
    if names.is_empty() {
        return Ok(started.elapsed());
    }
    for name in &names {
        legacy_taskkill(&["/IM", name, "/T"]);
    }
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        if legacy_named(&wanted).is_empty() {
            return Ok(started.elapsed());
        }
        thread::sleep(Duration::from_millis(250));
    }
    for name in &names {
        legacy_taskkill(&["/F", "/IM", name, "/T"]);
    }
    thread::sleep(Duration::from_millis(300));
    if legacy_named(&wanted).is_empty() {
        Ok(started.elapsed())
    } else {
        Err(format!("{images:?} survived the force pass"))
    }
}

// --------------------------------------------------------------------------

fn discovery_report() {
    let iters = 15;
    let count = |names: &[&str]| process::matching(names).expect("snapshot").len();

    let t = Instant::now();
    for _ in 0..iters {
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);
        let n = system
            .processes()
            .values()
            .filter(|p| CLIENTS.iter().any(|c| p.name().eq_ignore_ascii_case(c)))
            .count();
        std::hint::black_box(n);
    }
    println!(
        "sysinfo refresh (default kind)        : {:>8.3?} / call",
        t.elapsed() / iters
    );

    let t = Instant::now();
    for _ in 0..iters {
        let mut system = System::new_all();
        system.refresh_processes(ProcessesToUpdate::All, true);
        std::hint::black_box(system.processes().len());
    }
    println!(
        "sysinfo System::new_all + refresh     : {:>8.3?} / call",
        t.elapsed() / iters
    );

    let t = Instant::now();
    for _ in 0..iters {
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
        );
        std::hint::black_box(system.processes().len());
    }
    println!(
        "sysinfo refresh (exe path only)       : {:>8.3?} / call",
        t.elapsed() / iters
    );

    let t = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(count(CLIENTS));
    }
    println!(
        "ToolHelp snapshot + name match        : {:>8.3?} / call",
        t.elapsed() / iters
    );
}

fn spawn_report() {
    let iters = 10;
    let missing = "SwapperBenchDefinitelyNotRunning.exe";

    let mut visible = Command::new("taskkill");
    visible
        .args(["/IM", missing, "/T"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let t = Instant::now();
    for _ in 0..iters {
        let _ = visible.status();
    }
    let visible_ms = t.elapsed() / iters;

    let mut hidden = Command::new("taskkill");
    hidden
        .args(["/IM", missing, "/T"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    let t = Instant::now();
    for _ in 0..iters {
        let _ = hidden.status();
    }
    let hidden_ms = t.elapsed() / iters;

    println!("taskkill spawn (visible console)      : {visible_ms:>8.3?} / call");
    println!("taskkill spawn (CREATE_NO_WINDOW)     : {hidden_ms:>8.3?} / call");
    println!(
        "old force pass, 14 spawns serially    : {:>8.3?} (2 passes = 14 consoles)",
        visible_ms * 14
    );

    // Force-kill concurrency: the same seven missing images, all spawned at
    // once by force_kill, compared with the serial cost of the old path.
    let images: Vec<String> = (0..7).map(|i| format!("SwapperBenchMissing{i}.exe")).collect();
    let t = Instant::now();
    process::force_kill(&images);
    let parallel = t.elapsed();
    println!("force_kill, 7 images in parallel      : {parallel:>8.3?}");
    println!(
        "  serial equivalent (7 visible spawns) : {:>8.3?}",
        visible_ms * 7
    );
}

struct Scenario {
    label: &'static str,
    /// Images the fixtures are expected to produce after deduplication.
    expect_images: usize,
    /// Whether at least one fixture must own a top-level window.
    expect_windows: bool,
    spawn: Box<dyn Fn() -> Vec<Child>>,
}

fn scenario_report(scenario: Scenario) {
    let children = (scenario.spawn)();
    let pids: Vec<u32> = children.iter().map(|child| child.id()).collect();
    let found: Vec<process::ProcessRef> = pids
        .iter()
        .filter_map(|pid| {
            process::image_for_pid(*pid).expect("snapshot").map(|image| process::ProcessRef {
                pid: *pid,
                image,
            })
        })
        .collect();
    let images: Vec<String> = process::unique_images(&found);
    assert!(
        images.iter().all(|image| image.starts_with("SwapperBench")),
        "fixture naming guard failed: {images:?}"
    );
    assert_eq!(
        images.len(),
        scenario.expect_images,
        "{}: expected {} fixture image(s), got {images:?}",
        scenario.label,
        scenario.expect_images
    );
    let windows: Vec<usize> = pids
        .iter()
        .map(|pid| process::count_top_level_windows(*pid))
        .collect();
    if scenario.expect_windows {
        assert!(
            windows.iter().sum::<usize>() > 0,
            "{}: no top-level window found, graceful path would not be measured",
            scenario.label
        );
    }

    let legacy = legacy_stop(&images);
    reap(children);

    let children = (scenario.spawn)();
    let pids: Vec<u32> = children.iter().map(|child| child.id()).collect();
    let wanted: Vec<&str> = images.iter().map(String::as_str).collect();
    let started = Instant::now();
    let stop = process::stop_images(&wanted, &[], StopOptions::default());
    let modern_ms = started.elapsed();
    reap(children);

    let (legacy_ms, legacy_note) = match legacy {
        Ok(duration) => (Some(duration), "ok".to_string()),
        Err(error) => (None, error),
    };
    let report: StopReport = match stop {
        Ok(report) => report,
        Err(error) => panic!("{}: new stop failed: {error:?}", scenario.label),
    };

    println!("scenario: {}", scenario.label);
    println!(
        "  fixtures             : {} pid(s), {} image(s), windows {windows:?}",
        pids.len(),
        images.len()
    );
    match legacy_ms {
        Some(duration) => println!("  legacy stop (old)    : {duration:>8.3?}   {legacy_note}"),
        None => println!("  legacy stop (old)    :   failed   {legacy_note}"),
    }
    println!(
        "  new stop             : {modern_ms:>8.3?}   graceful {}/{}, grace {:?}, force {:?}, rounds {}",
        report.graceful_delivered,
        report.pids,
        report.graceful_ms,
        report.force_ms,
        report.force_rounds
    );
    if let Some(duration) = legacy_ms {
        let speedup = duration.as_secs_f64() / modern_ms.as_secs_f64().max(1e-6);
        println!("  speedup              : {speedup:>8.1}x");
    }
}

#[test]
#[ignore]
fn discovery_only() {
    let iters = 30;
    let t = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(process::snapshot().expect("snapshot").len());
    }
    println!("snapshot() (raw)             : {:?} / call", t.elapsed() / iters);
    println!("processes seen               : {}", process::snapshot().expect("snapshot").len());

    let t = Instant::now();
    for _ in 0..iters {
        unsafe {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPPROCESS,
            };
            let handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            assert!(handle != windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE);
            CloseHandle(handle);
        }
    }
    println!("raw CreateToolhelp32Snapshot : {:?} / call", t.elapsed() / iters);

    let t = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(process::matching(CLIENTS).expect("snapshot").len());
    }
    println!("matching(CLIENTS)            : {:?} / call", t.elapsed() / iters);

    let t = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(process::any_running(GAMES).expect("snapshot"));
    }
    println!("any_running(GAMES)           : {:?} / call", t.elapsed() / iters);

    let t = Instant::now();
    for _ in 0..iters {
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);
        std::hint::black_box(system.processes().len());
    }
    println!("sysinfo refresh (default)    : {:?} / call", t.elapsed() / iters);
}

#[test]
#[ignore]
fn perf_report() {
    println!("\n=== discovery ===");
    discovery_report();

    println!("\n=== taskkill spawn cost ===");
    spawn_report();

    println!("\n=== shutdown scenarios ===");
    let windowed = fixture(WINDOWED_IMAGE, Path::new(r"C:\Windows\System32\mstsc.exe"));
    let headless = fixture(HEADLESS_IMAGE, Path::new(r"C:\Windows\System32\ping.exe"));

    scenario_report(Scenario {
        label: "none (nothing running)",
        expect_images: 0,
        expect_windows: false,
        spawn: Box::new(Vec::new),
    });

    scenario_report(Scenario {
        label: "windowed (graceful close reachable)",
        expect_images: 1,
        expect_windows: true,
        spawn: {
            let path = windowed.clone();
            Box::new(move || vec![spawn_windowed(&path)])
        },
    });

    scenario_report(Scenario {
        label: "headless (no window, grace cannot help)",
        expect_images: 1,
        expect_windows: false,
        spawn: {
            let path = headless.clone();
            Box::new(move || vec![spawn(&path, &["-t", "127.0.0.1"], true)])
        },
    });

    scenario_report(Scenario {
        label: "mixed + duplicate images (3 pids, 2 images)",
        expect_images: 2,
        expect_windows: true,
        spawn: {
            let windowed = windowed.clone();
            let headless = headless.clone();
            Box::new(move || {
                vec![
                    spawn_windowed(&windowed),
                    spawn(&headless, &["-t", "127.0.0.1"], true),
                    spawn(&headless, &["-t", "127.0.0.1"], true),
                ]
            })
        },
    });

    // Modelled real-world cost of the old path with all seven client images
    // running: every lookup is a full sysinfo refresh, every name costs two
    // visible `taskkill` consoles, and a client that ignores the first pass
    // burned the full six second grace window.
    println!("\n=== modelled legacy cost, 7 client images running ===");
    let t = Instant::now();
    for _ in 0..5 {
        let mut fresh = System::new();
        fresh.refresh_processes(ProcessesToUpdate::All, true);
        std::hint::black_box(fresh.processes().len());
    }
    let lookup = t.elapsed() / 5;
    let mut visible = Command::new("taskkill");
    visible
        .args(["/IM", "SwapperBenchDefinitelyNotRunning.exe", "/T"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let t = Instant::now();
    for _ in 0..5 {
        let _ = visible.status();
    }
    let spawn_cost = t.elapsed() / 5;
    let total = lookup * 4 + spawn_cost * 14 + Duration::from_secs(6) + Duration::from_millis(300);
    println!("  sysinfo lookups (4)                 : {:?}", lookup * 4);
    println!("  visible taskkill spawns (14)        : {:?}", spawn_cost * 14);
    println!("  fixed grace window + tail sleep     : 6.300s");
    println!("  total (graceful kill refused)       : {total:?}");
    println!("  new path, same 7 images             : one snapshot + WM_CLOSE + parallel hidden force");

    // Never leave fixtures behind.
    process::force_kill(&[
        WINDOWED_IMAGE.to_string(),
        HEADLESS_IMAGE.to_string(),
        "SwapperBenchDefinitelyNotRunning.exe".to_string(),
    ]);
}

/// One-shot helper: which stock Windows binaries survive being copied to a
/// temp dir, and does the copy own a top-level window we can close?
#[test]
#[ignore]
fn probe_window_fixtures() {
    const CANDIDATES: &[(&str, &[&str])] = &[
        (r"C:\Windows\System32\charmap.exe", &[]),
        (r"C:\Windows\System32\sigverif.exe", &[]),
        (r"C:\Windows\System32\msinfo32.exe", &[]),
        (r"C:\Windows\System32\Taskmgr.exe", &[]),
        (r"C:\Windows\System32\eudcedit.exe", &[]),
        (r"C:\Windows\System32\mstsc.exe", &[]),
        (r"C:\Windows\System32\mmc.exe", &[]),
        (r"C:\Windows\System32\osk.exe", &[]),
        (r"C:\Windows\System32\notepad.exe", &[]),
        (
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            &[
                "-NoProfile",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MessageBox]::Show('hold')",
            ],
        ),
    ];

    let dir = std::env::temp_dir().join("swapper-bench-probe");
    fs::create_dir_all(&dir).expect("probe dir");
    for (source, args) in CANDIDATES {
        let image = format!(
            "SwapperProbe-{}",
            Path::new(source)
                .file_name()
                .expect("source has a name")
                .to_string_lossy()
        );
        let target = dir.join(&image);
        if let Err(error) = fs::copy(source, &target) {
            println!("{image:<36} copy failed : {error}");
            continue;
        }
        let child = match Command::new(&target)
            .args(*args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                println!("{image:<36} spawn failed: {error}");
                continue;
            }
        };
        let pid = child.id();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut windows = 0;
        while Instant::now() < deadline {
            windows = process::count_top_level_windows(pid);
            if windows > 0 {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let alive = process::image_for_pid(pid).expect("snapshot").is_some();
        println!("{image:<36} alive={alive:<5} windows={windows}");
        process::force_kill(&[image.clone()]);
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        thread::sleep(Duration::from_millis(300));
    }
}
