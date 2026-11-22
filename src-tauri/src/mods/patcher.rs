//! Finding, version-checking and installing LTK Manager. Swapper does not
//! run LTK's patcher (see `super::ltk`); it only needs LTK Manager installed,
//! current and set up.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use tauri::AppHandle;

/// The patcher host LTK Manager ships; an old install's version is read from it.
const PATCHER_EXE: &str = "ltk_patcher_host.exe";
/// LTK Manager's own executable, whose file version is the installed version.
const MANAGER_EXE: &str = "ltk-manager.exe";
/// Oldest LTK Manager that works with current League builds. From 1.32.1 the
/// patcher DLL is downloaded instead of shipped, so an older copy stops with
/// `Unsupported Game Build`.
pub const MIN_LTK_VERSION: &str = "1.32.1";
/// Short warning shown when LTK Manager is too old.
pub const OUTDATED_MESSAGE: &str =
    "LTK Manager is out of date. Update it or the Korean font and custom skins won't work.";

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

/// Latest LTK Manager version seen on GitHub, with when it was fetched.
static LATEST_CACHE: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);

/// Parses a dotted version such as `v1.32.1` into its numeric parts.
pub fn parse_version(text: &str) -> Option<Vec<u32>> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    let text = text.split(['-', '+']).next().unwrap_or("");
    if text.is_empty() {
        return None;
    }
    text.split('.').map(|part| part.parse::<u32>().ok()).collect()
}

fn compare_versions(left: &[u32], right: &[u32]) -> std::cmp::Ordering {
    let len = left.len().max(right.len());
    for index in 0..len {
        let ordering = left.get(index).unwrap_or(&0).cmp(right.get(index).unwrap_or(&0));
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    std::cmp::Ordering::Equal
}

/// Whether `installed` is older than [`MIN_LTK_VERSION`]. Unknown versions are not outdated.
pub fn is_outdated(installed: Option<&str>) -> bool {
    let (Some(installed), Some(minimum)) = (installed.and_then(parse_version), parse_version(MIN_LTK_VERSION)) else {
        return false;
    };
    compare_versions(&installed, &minimum) == std::cmp::Ordering::Less
}

fn at_least_minimum(installed: Option<&str>) -> bool {
    let (Some(installed), Some(minimum)) = (installed.and_then(parse_version), parse_version(MIN_LTK_VERSION)) else {
        return false;
    };
    compare_versions(&installed, &minimum) != std::cmp::Ordering::Less
}

/// The first install directory that has LTK Manager itself.
pub fn manager_path_in(dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    dirs.into_iter().map(|dir| dir.join(MANAGER_EXE)).find(|exe| exe.is_file())
}

/// LTK Manager's own path, or `None` when it is not installed.
pub fn manager_path() -> Option<PathBuf> {
    manager_path_in(patcher_dirs())
}

/// The installed LTK Manager version in `dirs`, from the manager executable
/// (falling back to the patcher host beside it).
pub fn installed_version_in(dirs: &[PathBuf]) -> Option<String> {
    for dir in dirs {
        if dir.join(MANAGER_EXE).is_file() {
            if let Some(version) = crate::windows::file_version(&dir.join(MANAGER_EXE)) {
                return Some(version);
            }
        }
        if dir.join(PATCHER_EXE).is_file() {
            if let Some(version) = crate::windows::file_version(&dir.join(PATCHER_EXE)) {
                return Some(version);
            }
        }
    }
    None
}

/// The installed LTK Manager version, or `None` when unknown.
pub fn installed_version() -> Option<String> {
    installed_version_in(&patcher_dirs())
}

/// Whether LTK Manager is installed.
pub fn ltk_installed() -> bool {
    manager_path().is_some()
}

/// The latest LTK Manager version already fetched, if any. Never fetches.
pub fn cached_latest_version() -> Option<String> {
    LATEST_CACHE.lock().unwrap_or_else(|error| error.into_inner()).as_ref().and_then(|(_, version)| version.clone())
}

/// Fetches the latest LTK Manager version from GitHub, cached for an hour.
/// Failure-tolerant: any error keeps the cached value and is only logged.
pub async fn refresh_latest_version() -> Option<String> {
    {
        let cache = LATEST_CACHE.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((fetched, version)) = cache.as_ref() {
            if fetched.elapsed() < Duration::from_secs(3600) {
                return version.clone();
            }
        }
    }
    let fetched = fetch_latest_version().await;
    let version = fetched.clone().or_else(cached_latest_version);
    *LATEST_CACHE.lock().unwrap_or_else(|error| error.into_inner()) = Some((Instant::now(), fetched));
    version
}

async fn fetch_latest_version() -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Swapper/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let latest: LtkLatest = client
        .get(format!("{LTK_RELEASE_URL}/latest.json"))
        .send()
        .await
        .map_err(|error| eprintln!("LTK release check failed: {error}"))
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    latest.version.map(|version| version.trim().trim_start_matches(['v', 'V']).to_string()).filter(|version| !version.is_empty())
}

/// Where LTK Manager installs: its older per-user location, then per-machine.
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

/// The Windows platform entry of LTK Manager's Tauri updater `latest.json`.
#[derive(serde::Deserialize)]
struct LtkPlatform {
    url: String,
    signature: String,
}

#[derive(serde::Deserialize)]
struct LtkLatest {
    #[serde(default)]
    version: Option<String>,
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
        .map_err(|error| { eprintln!("LTK releases unreachable: {error}"); "Couldn't reach LTK Manager. Check your connection and retry.".to_string() })?
        .error_for_status()
        .map_err(|error| { eprintln!("LTK release info failed: {error}"); "Couldn't read LTK Manager info. Try again shortly.".to_string() })?
        .json()
        .await
        .map_err(|error| { eprintln!("LTK release info failed: {error}"); "Couldn't read LTK Manager info. Try again shortly.".to_string() })?;
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
        .map_err(|_| "LTK Manager couldn't be verified. Try again.".to_string())?;
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
    let url = url::Url::parse("https://wiki.leaguetoolkit.dev/").map_err(|error| error.to_string())?;
    crate::commands::guide::open_in_browser(&url).map_err(|error| { eprintln!("LTK website open failed: {error}"); "Couldn't open the LTK site. Try again.".to_string() })
}

/// Downloads LTK Manager's official installer, minisign-verifies it against
/// LTK's updater key, runs it silently (`/S`; Windows shows one UAC prompt for
/// the per-machine install), then sets LTK Manager up (`super::ltk`). A
/// current install is only set up.
///
/// This is a trust boundary: the downloaded executable is only run once its
/// signature verifies. A declined UAC prompt surfaces as that installer's
/// non-zero exit code, not a hang.
#[tauri::command]
pub async fn install_ltk_manager(app: AppHandle) -> Result<(), String> {
    if manager_path().is_none() || !at_least_minimum(installed_version().as_deref()) {
        install().await?;
    }
    tokio::task::spawn_blocking(move || super::ltk::request_setup(&app))
        .await
        .map_err(|error| error.to_string())?
}

async fn install() -> Result<(), String> {
    let client = installer_client()?;
    let (installer_url, signature) = ltk_installer_source(&client).await?;
    let installer = client
        .get(&installer_url)
        .send()
        .await
        .map_err(|error| { eprintln!("LTK download failed: {error}"); "Couldn't download LTK Manager. Check your connection and retry.".to_string() })?
        .error_for_status()
        .map_err(|error| { eprintln!("LTK download failed: {error}"); "Couldn't download LTK Manager. Check your connection and retry.".to_string() })?
        .bytes()
        .await
        .map_err(|error| { eprintln!("LTK download failed: {error}"); "Couldn't download LTK Manager. Check your connection and retry.".to_string() })?;
    verify_minisign(LTK_PUBKEY, &signature, &installer)?;

    // A fresh unpredictable directory, so nothing can pre-plant or swap the file path.
    let dir = std::env::temp_dir().join(format!("Swapper-LTK-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&dir)
        .map_err(|error| { eprintln!("LTK installer save failed: {error}"); "Couldn't save the installer. Check disk space and retry.".to_string() })?;
    let path = dir.join("setup.exe");
    let saved = fs::write(&path, &installer)
        .map_err(|error| { eprintln!("LTK installer save failed: {error}"); "Couldn't save the installer. Check disk space and retry.".to_string() })
        // Re-verify what is on disk, so the bytes that run are the bytes that were signed.
        .and_then(|()| {
            let on_disk = fs::read(&path)
                .map_err(|error| { eprintln!("LTK installer save failed: {error}"); "Couldn't save the installer. Check disk space and retry.".to_string() })?;
            verify_minisign(LTK_PUBKEY, &signature, &on_disk)
        });
    if let Err(error) = saved {
        let _ = fs::remove_dir_all(&dir);
        return Err(error);
    }
    let run = {
        let installer_path = path.to_string_lossy().into_owned();
        // Per-machine: it needs admin, so a plain CreateProcess fails with error
        // 740. ShellExecuteExW "runas" shows the UAC prompt instead.
        tokio::task::spawn_blocking(move || {
            crate::remote::network::run_elevated(&installer_path, "/S", INSTALL_WAIT_MS)
        })
        .await
    };
    let _ = fs::remove_dir_all(&dir);
    match run {
        Ok(Ok(0)) => {}
        Ok(Ok(code)) => {
            eprintln!("LTK installer exited with code {code}"); return Err("LTK Manager didn't install. Try again.".to_string());
        }
        Ok(Err(crate::remote::network::ElevateError::Cancelled)) => {
            return Err(
                "LTK Manager wasn't installed. Windows permission was declined.".to_string(),
            );
        }
        Ok(Err(crate::remote::network::ElevateError::Failed)) => {
            return Err("Couldn't run the LTK installer. Try again.".to_string());
        }
        Err(error) => { eprintln!("LTK installer run failed: {error}"); return Err("Couldn't run the LTK installer. Try again.".to_string()); }
    }
    if manager_path().is_none() {
        return Err("LTK Manager installed, but Swapper can't find it. Reinstall it.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_discovery_picks_the_first_dir_that_has_it() {
        let base = std::env::temp_dir().join(format!("swapper-ltk-discovery-{}", std::process::id()));
        let per_user = base.join("per-user");
        let per_machine = base.join("per-machine");
        fs::create_dir_all(&per_user).unwrap();
        fs::create_dir_all(&per_machine).unwrap();
        let dirs = || vec![per_user.clone(), per_machine.clone()];

        assert_eq!(manager_path_in(dirs()), None, "empty dirs have no LTK Manager");

        fs::write(per_machine.join(MANAGER_EXE), b"host").unwrap();
        assert_eq!(
            manager_path_in(dirs()),
            Some(per_machine.join(MANAGER_EXE)),
            "the per-machine install is found when it is the only one"
        );

        fs::write(per_user.join(MANAGER_EXE), b"host").unwrap();
        assert_eq!(
            manager_path_in(dirs()),
            Some(per_user.join(MANAGER_EXE)),
            "the per-user install wins when both exist"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn outdated_versions_are_below_the_minimum() {
        assert!(is_outdated(Some("1.27.1")));
        assert!(is_outdated(Some("1.32.0")));
        assert!(is_outdated(Some("v1.31.9")));
        assert!(!is_outdated(Some("1.32.1")));
        assert!(!is_outdated(Some("1.32.2")));
        assert!(!is_outdated(Some("1.33.0")));
        assert!(!is_outdated(Some("2.0.0")));
        assert!(!is_outdated(None));
        assert!(!is_outdated(Some("")));
        assert!(!is_outdated(Some("not-a-version")));
        assert_eq!(parse_version("v1.32.1"), Some(vec![1, 32, 1]));
        assert_eq!(parse_version("1.32"), Some(vec![1, 32]));
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

