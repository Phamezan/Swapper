//! Optional Korean font: builds a WAD overlay from the bundled
//! `korean-font.fantome` with League Toolkit's Apache-2.0 crates and runs
//! League Toolkit's patcher while a game is loading or running, so League's
//! in-game UI uses the Korean font. Off by default; when off, nothing runs.
//!
//! The overlay build is incremental (the `ltk_overlay` state file skips an
//! unchanged build), so after the first game the patcher start is cheap. The
//! patcher scans for the game window itself and stops on stdin EOF, so the
//! child's stdin is kept piped and closed to stop it. Every stop path closes
//! it, so Swapper never leaves an orphan patcher behind.
//!
//! Swapper does not ship the patcher: it uses the signed one from the user's
//! LTK Manager install (Vanguard rejects the signature-stripped copy Swapper
//! used to bundle), resolved at use time so an install made while Swapper runs
//! is picked up. When LTK Manager is missing, [`install_ltk_manager`] fetches
//! and minisign-verifies its official installer.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use camino::{Utf8Path, Utf8PathBuf};
use ltk_overlay::{EnabledMod, FantomeContent, OverlayBuilder};
use minisign_verify::{PublicKey, Signature};
use tauri::{AppHandle, Manager};

/// Mod id recorded in the overlay state. Stable so incremental rebuilds and
/// the exact-match skip keep working across runs.
const MOD_ID: &str = "korean-font";

/// The patcher host executable name inside an LTK Manager install.
pub const PATCHER_EXE: &str = "ltk_patcher_host.exe";

/// How long to wait for the patcher to exit on its own after stdin closes,
/// before killing it.
const STOP_GRACE: Duration = Duration::from_secs(3);

/// Upper bound for the elevated LTK Manager install (it drops ~21 MB of files
/// and registers itself).
const INSTALL_WAIT_MS: u32 = 10 * 60 * 1000;

/// LTK Manager's updater public key: base64 of the minisign public key file
/// from their `tauri.conf.json` `plugins.updater.pubkey`. The installer Swapper
/// downloads is only run when it verifies against this.
const LTK_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEEwQjBFRkU5MDdENUVENTIKUldSUzdkVUg2ZSt3b0pldjRGZUQ3WkgwcjU0M2d5N1pnRGlhR0NPODkyMWpyaTEvUFQvWWlpcVkK";

/// Base for the "latest release" assets on GitHub.
const LTK_RELEASE_URL: &str =
    "https://github.com/LeagueToolkit/ltk-manager/releases/latest/download";

/// The patcher child, if one is running.
static PATCHER: Mutex<Option<Child>> = Mutex::new(None);
/// A build can outlive several watcher polls; this keeps one start in flight.
static STARTING: AtomicBool = AtomicBool::new(false);
/// Whether "LTK Manager missing" was already logged, so the watcher's frequent
/// polls during a game do not repeat it.
static MISSING_LOGGED: AtomicBool = AtomicBool::new(false);

/// Where LTK Manager installs the patcher host: its older per-user location
/// first, then the per-machine location it moved to. Read at use time so an
/// install made while Swapper runs is picked up.
fn patcher_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("LTK Manager"));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        dirs.push(PathBuf::from(program_files).join("LTK Manager"));
    }
    dirs
}

/// The first LTK Manager install that has the patcher host, or `None` when LTK
/// Manager is not installed. The host loads `ltk_patcher_dll.dll` from its own
/// directory, so only the host path is needed.
pub fn patcher_path() -> Option<PathBuf> {
    patcher_in(patcher_dirs())
}

/// The first candidate directory that actually contains the patcher host.
fn patcher_in(dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    dirs.into_iter()
        .map(|dir| dir.join(PATCHER_EXE))
        .find(|exe| exe.is_file())
}

/// Whether the toggle is on. The single source of truth is the persisted
/// config, so a disabled toggle never starts and an enabled one stops only by
/// closing stdin.
fn enabled(app: &AppHandle) -> bool {
    app.try_state::<crate::AppState>()
        .is_some_and(|state| state.korean_font_enabled())
}

/// Whether a game is loading or running under this gameflow phase. The patcher
/// is started ahead of the game window (champion select onwards) so it is
/// already scanning when the game process starts, and stopped once the game is
/// over.
fn phase_wants_patcher(phase: &str) -> bool {
    matches!(
        phase,
        "ReadyCheck"
            | "ChampSelect"
            | "Matchmaking"
            | "InProgress"
            | "Reconnect"
            | "WaitingForStats"
            | "PreEndOfGame"
    )
}

/// The League `Game` directory for a detected `League of Legends.exe`.
fn game_dir_from_exe(exe: &Utf8Path) -> Result<Utf8PathBuf, String> {
    exe.parent()
        .filter(|parent| !parent.as_str().is_empty())
        .map(Utf8Path::to_owned)
        .ok_or_else(|| "League installation path has no Game folder.".to_string())
}

/// Whether the patcher child is still alive. A child that has already exited
/// is dropped so a later start does not see a stale handle.
fn patcher_running() -> bool {
    let mut guard = match PATCHER.lock() {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    };
    match guard.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(None) => true,
            _ => {
                *guard = None;
                false
            }
        },
        None => false,
    }
}

/// Reacts to one polled gameflow phase: ensure the patcher is running while a
/// game is loading or running and the toggle is on, and stop it otherwise.
/// Called from the champion-select watcher and, with `"Offline"`, when League
/// disconnects so a crashed client cannot strand the patcher.
pub fn observe(app: &AppHandle, phase: &str) {
    if enabled(app) && phase_wants_patcher(phase) {
        ensure(app.clone());
    } else {
        stop();
    }
}

/// Starts the patcher for an already-running game when the toggle is turned
/// on, instead of waiting for the watcher's next poll.
pub fn on_enabled(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let status = crate::runes::current_status().await;
        if enabled(&app) && phase_wants_patcher(&status.phase) {
            ensure(app);
        }
    });
}

/// Builds the overlay and starts the patcher once. Idempotent: a running
/// patcher or an in-flight build makes this a no-op. Does nothing (and logs
/// once) when LTK Manager's patcher is not installed.
fn ensure(app: AppHandle) {
    if STARTING.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(host) = patcher_path() else {
        STARTING.store(false, Ordering::SeqCst);
        if !MISSING_LOGGED.swap(true, Ordering::SeqCst) {
            eprintln!(
                "Korean font: LTK Manager's patcher was not found. Install LTK Manager from Settings \u{2192} General."
            );
        }
        return;
    };
    MISSING_LOGGED.store(false, Ordering::SeqCst);
    if patcher_running() {
        STARTING.store(false, Ordering::SeqCst);
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(error) = build_and_start(&app, host).await {
            eprintln!("Korean font: {error}");
        }
        STARTING.store(false, Ordering::SeqCst);
    });
}

async fn build_and_start(app: &AppHandle, host: PathBuf) -> Result<(), String> {
    let (fantome, root) = {
        let state = app
            .try_state::<crate::AppState>()
            .ok_or("Swapper is not ready.")?;
        (
            state.bundled_korean_font.clone(),
            crate::vault::data_root()?.join("korean-font"),
        )
    };
    // Building reads the game's WAD index and compresses chunks; keep it off the
    // async runtime.
    let overlay = tokio::task::spawn_blocking(move || build_overlay(&fantome, &root))
        .await
        .map_err(|error| error.to_string())??;
    let child = spawn_patcher(&host, &overlay)?;
    *PATCHER.lock().unwrap_or_else(|error| error.into_inner()) = Some(child);
    Ok(())
}

/// Builds (incrementally, when possible) the overlay for the bundled mod and
/// returns the overlay directory the patcher should map.
fn build_overlay(fantome: &Path, root: &Path) -> Result<PathBuf, String> {
    let game_exe = ltk_mod_core::auto_detect_league_path()
        .ok_or("League of Legends installation was not found.")?;
    let game_dir = game_dir_from_exe(game_exe.as_path())?;

    let overlay_root = root.join("overlay");
    let state_dir = root.join("state");
    fs::create_dir_all(&overlay_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&state_dir).map_err(|error| error.to_string())?;

    let archive_path = Utf8PathBuf::from_path_buf(fantome.to_path_buf())
        .map_err(|_| "The bundled Korean font path is not valid UTF-8.".to_string())?;
    let file = fs::File::open(fantome)
        .map_err(|error| format!("Cannot open the bundled Korean font mod: {error}"))?;
    let content = FantomeContent::new(file)
        .map_err(|error| format!("Cannot read the bundled Korean font mod: {error}"))?
        .with_archive_path(archive_path);

    let overlay_utf8 = Utf8PathBuf::from_path_buf(overlay_root.clone())
        .map_err(|_| "The overlay path is not valid UTF-8.".to_string())?;
    let state_utf8 = Utf8PathBuf::from_path_buf(state_dir)
        .map_err(|_| "The overlay state path is not valid UTF-8.".to_string())?;

    let mut builder = OverlayBuilder::new(game_dir, overlay_utf8, state_utf8);
    builder.set_enabled_mods(vec![EnabledMod {
        id: MOD_ID.to_string(),
        content: Box::new(content),
        enabled_layers: None,
    }]);
    builder
        .build()
        .map_err(|error| format!("Could not build the Korean font overlay: {error}"))?;
    Ok(overlay_root)
}

fn spawn_patcher(host: &Path, overlay: &Path) -> Result<Child, String> {
    use std::os::windows::process::CommandExt;
    // No console window for the background patcher.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new(host)
        .arg("runoverlay")
        .arg(overlay)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| format!("Could not start the Korean font patcher: {error}"))
}

/// Stops the patcher by closing its stdin (the patcher stops on EOF) and, if
/// it does not exit promptly, killing it. Safe to call when nothing is running.
pub fn stop() {
    let child = {
        let mut guard = PATCHER.lock().unwrap_or_else(|error| error.into_inner());
        guard.take()
    };
    let Some(mut child) = child else {
        return;
    };
    // Closing stdin is the documented stop signal.
    drop(child.stdin.take());
    let deadline = Instant::now() + STOP_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The Windows platform entry of LTK Manager's Tauri updater `latest.json`.
#[derive(serde::Deserialize)]
struct LtkPlatform {
    url: String,
    signature: String,
}

#[derive(serde::Deserialize)]
struct LtkLatest {
    platforms: HashMap<String, LtkPlatform>,
}

/// The installer URL and its base64 minisign signature. Prefers the
/// unversioned release assets; falls back to `latest.json` when only the
/// versioned asset carries a `.sig` (the current release's naming).
async fn ltk_installer_source(client: &reqwest::Client) -> Result<(String, String), String> {
    let unversioned_sig = format!("{LTK_RELEASE_URL}/LTK.Manager_x64-setup.exe.sig");
    if let Ok(response) = client.get(&unversioned_sig).send().await {
        if response.status().is_success() {
            if let Ok(signature) = response.text().await {
                if !signature.trim().is_empty() {
                    return Ok((
                        format!("{LTK_RELEASE_URL}/LTK.Manager_x64-setup.exe"),
                        signature,
                    ));
                }
            }
        }
    }
    let latest: LtkLatest = client
        .get(format!("{LTK_RELEASE_URL}/latest.json"))
        .send()
        .await
        .map_err(|error| format!("Could not reach LTK Manager's releases: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Could not read LTK Manager's release info: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Could not read LTK Manager's release info: {error}"))?;
    let platform = latest
        .platforms
        .get("windows-x86_64")
        .ok_or("LTK Manager's latest release has no Windows installer.")?;
    Ok((platform.url.clone(), platform.signature.clone()))
}

/// Base64-decodes a minisign text file (the Tauri convention for pubkeys and
/// `.sig` assets) into the minisign text form the verifier parses.
fn decode_minisign_file(encoded: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| "The minisign data is not valid base64.".to_string())?;
    String::from_utf8(bytes).map_err(|_| "The minisign data is not valid UTF-8.".to_string())
}

/// Verifies `data` against a base64 minisign signature and public key. Any
/// failure means the data must not be trusted.
fn verify_minisign(pubkey_b64: &str, signature_b64: &str, data: &[u8]) -> Result<(), String> {
    let public_key = decode_minisign_file(pubkey_b64)
        .and_then(|text| PublicKey::decode(&text).map_err(|_| "The LTK Manager signing key is malformed.".to_string()))?;
    let signature = decode_minisign_file(signature_b64)
        .and_then(|text| Signature::decode(&text).map_err(|_| "The LTK Manager signature is malformed.".to_string()))?;
    public_key
        .verify(data, &signature, true)
        .map_err(|_| "The downloaded LTK Manager installer failed signature verification.".to_string())
}

/// The long-running download/execute client. No overall timeout on the body
/// beyond the connect/read timeouts, because the installer is ~21 MB.
fn installer_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("Swapper/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|error| error.to_string())
}

/// Opens League Toolkit's site in the default browser, so a user without LTK
/// Manager can read what it is before installing it. The URL is fixed; the
/// webview never chooses what Swapper opens.
#[tauri::command]
pub fn open_ltk_website() -> Result<(), String> {
    Command::new("explorer")
        .arg("https://wiki.leaguetoolkit.dev/")
        .spawn()
        .map(drop)
        .map_err(|error| format!("Could not open the LTK Manager website: {error}"))
}

/// Downloads LTK Manager's official installer, minisign-verifies it against
/// LTK's updater key, runs it silently (`/S`; Windows shows one UAC prompt for
/// the per-machine install) and confirms the patcher is discoverable afterwards.
///
/// This is a trust boundary: the downloaded executable is only run once its
/// signature verifies. A declined UAC prompt surfaces as that installer's
/// non-zero exit code, not a hang.
#[tauri::command]
pub async fn install_ltk_manager() -> Result<(), String> {
    let client = installer_client()?;
    let (installer_url, signature) = ltk_installer_source(&client).await?;
    let installer = client
        .get(&installer_url)
        .send()
        .await
        .map_err(|error| format!("Could not download LTK Manager: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Could not download LTK Manager: {error}"))?
        .bytes()
        .await
        .map_err(|error| format!("Could not download LTK Manager: {error}"))?;
    verify_minisign(LTK_PUBKEY, &signature, &installer)?;

    let path = std::env::temp_dir().join("Swapper-LTK-Manager-setup.exe");
    fs::write(&path, &installer)
        .map_err(|error| format!("Could not save the LTK Manager installer: {error}"))?;
    let run = {
        let installer_path = path.to_string_lossy().into_owned();
        // Per-machine: it needs admin, so a plain CreateProcess fails with error
        // 740. ShellExecuteExW "runas" shows the UAC prompt instead.
        tokio::task::spawn_blocking(move || {
            crate::remote::network::run_elevated(&installer_path, "/S", INSTALL_WAIT_MS)
        })
        .await
    };
    let _ = fs::remove_file(&path);
    match run {
        Ok(Ok(0)) => {}
        Ok(Ok(code)) => {
            return Err(format!("The LTK Manager installer exited with code {code}."));
        }
        Ok(Err(crate::remote::network::ElevateError::Cancelled)) => {
            return Err(
                "LTK Manager was not installed: Windows admin permission was declined.".to_string(),
            );
        }
        Ok(Err(crate::remote::network::ElevateError::Failed)) => {
            return Err("Could not run the LTK Manager installer.".to_string());
        }
        Err(error) => return Err(format!("Could not run the LTK Manager installer: {error}")),
    }
    if patcher_path().is_none() {
        return Err("LTK Manager installed but its patcher was not found.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patcher_runs_for_a_loading_or_running_game_only() {
        for phase in [
            "ReadyCheck",
            "ChampSelect",
            "Matchmaking",
            "InProgress",
            "Reconnect",
            "WaitingForStats",
            "PreEndOfGame",
        ] {
            assert!(phase_wants_patcher(phase), "{phase} should run the patcher");
        }
        for phase in ["None", "Lobby", "EndOfGame", "Offline", "Unavailable", ""] {
            assert!(
                !phase_wants_patcher(phase),
                "{phase} should not run the patcher"
            );
        }
    }

    #[test]
    fn game_dir_is_the_parent_of_the_league_executable() {
        let exe = Utf8Path::new(r"C:\Riot Games\League of Legends\Game\League of Legends.exe");
        assert_eq!(
            game_dir_from_exe(exe).unwrap().as_str(),
            r"C:\Riot Games\League of Legends\Game"
        );
        assert!(game_dir_from_exe(Utf8Path::new("League of Legends.exe")).is_err());
    }

    #[test]
    fn patcher_discovery_picks_the_first_dir_that_has_the_host() {
        let base = std::env::temp_dir().join(format!("swapper-ltk-discovery-{}", std::process::id()));
        let per_user = base.join("per-user");
        let per_machine = base.join("per-machine");
        fs::create_dir_all(&per_user).unwrap();
        fs::create_dir_all(&per_machine).unwrap();
        let dirs = || vec![per_user.clone(), per_machine.clone()];

        assert_eq!(patcher_in(dirs()), None, "empty dirs have no patcher");

        fs::write(per_machine.join(PATCHER_EXE), b"host").unwrap();
        assert_eq!(
            patcher_in(dirs()),
            Some(per_machine.join(PATCHER_EXE)),
            "the per-machine install is found when it is the only one"
        );

        fs::write(per_user.join(PATCHER_EXE), b"host").unwrap();
        assert_eq!(
            patcher_in(dirs()),
            Some(per_user.join(PATCHER_EXE)),
            "the per-user install wins when both exist"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn minisign_verification_accepts_a_good_pair_and_rejects_a_tampered_file() {
        // Known-good pair from the minisign-verify crate's own test vector.
        const PUBKEY_TEXT: &str = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n";
        const SIG_TEXT: &str = "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";
        let pubkey_b64 = base64::engine::general_purpose::STANDARD.encode(PUBKEY_TEXT);
        let signature_b64 = base64::engine::general_purpose::STANDARD.encode(SIG_TEXT);

        assert!(verify_minisign(&pubkey_b64, &signature_b64, b"test").is_ok());
        assert!(
            verify_minisign(&pubkey_b64, &signature_b64, b"tost").is_err(),
            "a tampered byte must fail verification"
        );
    }

    #[test]
    #[ignore = "network: downloads LTK Manager's latest release to prove the URL and minisign key"]
    fn real_ltk_release_download_verifies() {
        let client = installer_client().expect("client");
        let (url, signature) =
            tauri::async_runtime::block_on(ltk_installer_source(&client)).expect("release source");
        let bytes = tauri::async_runtime::block_on(async {
            client
                .get(&url)
                .send()
                .await
                .map_err(|error| error.to_string())?
                .error_for_status()
                .map_err(|error| error.to_string())?
                .bytes()
                .await
                .map_err(|error| error.to_string())
        })
        .expect("download installer");
        verify_minisign(LTK_PUBKEY, &signature, &bytes).expect("verify installer");
        eprintln!("verified {} bytes from {url}", bytes.len());
    }
}

