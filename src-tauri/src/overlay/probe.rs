//! Opt-in measurement, isolated from the normal parallel test suite.
use super::*;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

struct Sample {
    threads: u32,
    resident_bytes: u64,
}

fn sample() -> Sample {
    unsafe {
        let handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = Process32FirstW(handle, &mut entry);
        let mut threads = 0;
        while found != 0 {
            if entry.th32ProcessID == std::process::id() {
                threads = entry.cntThreads;
                break;
            }
            found = Process32NextW(handle, &mut entry);
        }
        CloseHandle(handle);
        assert!(threads > 0);
        // Reuse sysinfo for memory; only this PID and only its memory field.
        let mut system = sysinfo::System::new();
        let pid = sysinfo::Pid::from_u32(std::process::id());
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            false,
            sysinfo::ProcessRefreshKind::nothing().with_memory(),
        );
        Sample {
            threads,
            resident_bytes: system.process(pid).unwrap().memory(),
        }
    }
}

fn cpu_ms() -> f64 {
    unsafe {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        assert_ne!(
            GetProcessTimes(
                GetCurrentProcess(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user
            ),
            0
        );
        let ticks =
            |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
        (ticks(kernel) + ticks(user)) as f64 / 10_000.0
    }
}

fn measure(label: &str) -> Sample {
    let before = cpu_ms();
    let started = Instant::now();
    std::thread::sleep(Duration::from_secs(4));
    let delta = cpu_ms() - before;
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let after = sample();
    println!(
        "{label}: threads={}, resident_KiB={}, CPU_delta_ms={:.3}, elapsed_ms={:.0}",
        after.threads,
        after.resident_bytes / 1024,
        delta,
        elapsed
    );
    after
}

#[test]
#[ignore = "run alone with --ignored --exact --nocapture --test-threads=1"]
fn idle_cost() {
    assert!(
        !crate::windows::process::any_running(&["League of Legends.exe"]).unwrap(),
        "Close the game before measuring idle cost"
    );
    // Warm the measuring API before establishing a baseline.
    sample();
    let baseline = measure("baseline");
    let mut worker = None;
    Controller::apply(&mut worker, Settings::default(), "emerald_plus").unwrap();
    let disabled = measure("disabled");
    assert_eq!(
        baseline.threads, disabled.threads,
        "disabled must add no threads"
    );
    Controller::apply(
        &mut worker,
        Settings {
            enabled: true,
            ..Settings::default()
        },
        "emerald_plus",
    )
    .unwrap();
    let enabled = measure("enabled, no game");
    assert_eq!(enabled.threads, disabled.threads + 1);
    Controller::apply(&mut worker, Settings::default(), "emerald_plus").unwrap();
    let stopped = measure("disabled again");
    assert_eq!(
        stopped.threads, baseline.threads,
        "disable must join the worker"
    );
}
