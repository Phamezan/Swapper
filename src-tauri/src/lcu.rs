use base64::Engine;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{
    connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream,
};

static LOCKFILE_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static LAST_PROCESS_SCAN: Mutex<Option<Instant>> = Mutex::new(None);
static SCANNED_ENDPOINT: Mutex<Option<(u32, LcuEndpoint)>> = Mutex::new(None);

const PROCESS_SCAN_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LcuEndpoint {
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn parse_lockfile(body: &str) -> Option<(u32, LcuEndpoint)> {
    let line = body.lines().next()?.trim();
    let mut parts = line.split(':');
    let name = parts.next()?.trim();
    let pid: u32 = parts.next()?.trim().parse().ok()?;
    let port: u16 = parts.next()?.trim().parse().ok()?;
    let password = parts.next()?.trim();
    let protocol = parts.next()?.trim();
    // Riot writes `LeagueClient`; some builds write `LeagueClientUx`.
    if !(name.eq_ignore_ascii_case("LeagueClient") || name.eq_ignore_ascii_case("LeagueClientUx"))
        || pid == 0
        || port == 0
        || password.is_empty()
        || protocol.is_empty()
    {
        return None;
    }
    Some((
        pid,
        LcuEndpoint {
            port,
            password: password.to_string(),
            protocol: protocol.to_string(),
        },
    ))
}

pub fn lockfile_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let root = PathBuf::from(local).join("Riot Games");
        candidates.push(root.join("League of Legends").join("lockfile"));
        candidates.push(root.join("LeagueOfLegends").join("lockfile"));
    }
    for base in [
        PathBuf::from(r"C:\Riot Games"),
        PathBuf::from(r"C:\Program Files\Riot Games"),
        PathBuf::from(r"C:\Program Files (x86)\Riot Games"),
    ] {
        candidates.push(base.join("League of Legends").join("lockfile"));
    }
    candidates
}

/// Whether a League lockfile exists; a file stat only, no process scan.
pub fn lockfile_present() -> bool {
    lock(&LOCKFILE_PATH).iter().any(|path| path.is_file())
        || lockfile_candidates().iter().any(|path| path.is_file())
}

pub fn discover() -> Option<LcuEndpoint> {
    if let Some(path) = lock(&LOCKFILE_PATH).clone() {
        if let Some(endpoint) = read_at(&path) {
            return Some(endpoint);
        }
    }
    for path in lockfile_candidates() {
        if let Some(endpoint) = read_at(&path) {
            *lock(&LOCKFILE_PATH) = Some(path);
            return Some(endpoint);
        }
    }
    process_endpoint()
}

/// Whether the League Client process itself is alive. Used to tell a client
/// that is still starting (or a stale lockfile) apart from one that is gone.
///
/// Enumeration failures are surfaced so callers can pick the conservative
/// answer; this probe never gates account-switch safety.
pub fn league_process_running() -> Result<bool, crate::windows::process::SnapshotError> {
    crate::windows::process::any_running(&["LeagueClientUx.exe"])
}

pub async fn connect(
    endpoint: &LcuEndpoint,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, String> {
    let scheme = if endpoint.protocol.eq_ignore_ascii_case("http") {
        "ws"
    } else {
        "wss"
    };
    let url = format!("{scheme}://127.0.0.1:{}/", endpoint.port);
    let mut request = url
        .into_client_request()
        .map_err(|e| format!("League client URL is invalid: {e}"))?;
    let value = HeaderValue::from_str(&authorization(endpoint))
        .map_err(|e| format!("League client credentials are invalid: {e}"))?;
    request.headers_mut().insert("Authorization", value);
    let tls = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| format!("League client TLS setup failed: {e}"))?;
    let connector = Connector::NativeTls(tls);
    connect_async_tls_with_config(request, None, false, Some(connector))
        .await
        .map(|(socket, _)| socket)
        .map_err(|e| format!("League client connection failed: {e}"))
}

pub fn scheme(endpoint: &LcuEndpoint) -> &'static str {
    if endpoint.protocol.eq_ignore_ascii_case("http") {
        "http"
    } else {
        "https"
    }
}

pub fn authorization(endpoint: &LcuEndpoint) -> String {
    let raw =
        base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", endpoint.password));
    format!("Basic {raw}")
}

pub fn api_url(endpoint: &LcuEndpoint, path: &str) -> String {
    format!("{}://127.0.0.1:{}{path}", scheme(endpoint), endpoint.port)
}

fn read_at(path: &std::path::Path) -> Option<LcuEndpoint> {
    if !path.is_file() {
        return None;
    }
    // Riot keeps the lockfile open. Share read/write/delete so we can inspect it.
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(0x7)
        .open(path)
        .ok()?;
    let mut body = String::new();
    file.read_to_string(&mut body).ok()?;
    let (pid, endpoint) = parse_lockfile(&body)?;
    // The lockfile outlives a client shutdown, so a lone file read would report a
    // closed client as running. Require the recorded pid to still be League.
    league_pid_alive(pid).then_some(endpoint)
}

fn league_pid_alive(pid: u32) -> bool {
    // A single Toolhelp32 pass is far cheaper than a full sysinfo refresh, and
    // `discover` runs from the champion-select watcher every few seconds.
    match crate::windows::process::image_for_pid(pid) {
        Ok(Some(image)) => {
            image.eq_ignore_ascii_case("LeagueClient.exe")
                || image.eq_ignore_ascii_case("LeagueClientUx.exe")
        }
        // The pid is gone, so the lockfile is stale.
        Ok(None) => false,
        // An unreadable table is treated as "still there": discover returns the
        // endpoint and the caller's request decides whether it is real.
        Err(_) => true,
    }
}

fn process_endpoint() -> Option<LcuEndpoint> {
    {
        let mut last = lock(&LAST_PROCESS_SCAN);
        if let Some(at) = *last {
            if at.elapsed() < PROCESS_SCAN_INTERVAL {
                return cached_endpoint(&mut lock(&SCANNED_ENDPOINT), league_pid_alive);
            }
        }
        *last = Some(Instant::now());
    }
    let found = scan_system();
    *lock(&SCANNED_ENDPOINT) = found.clone();
    found.map(|(_, endpoint)| endpoint)
}

/// The last scan's endpoint while its process is still alive; drops it otherwise.
fn cached_endpoint(
    cache: &mut Option<(u32, LcuEndpoint)>,
    alive: impl Fn(u32) -> bool,
) -> Option<LcuEndpoint> {
    if !cache.as_ref().is_some_and(|(pid, _)| alive(*pid)) {
        *cache = None;
    }
    cache.as_ref().map(|(_, endpoint)| endpoint.clone())
}

/// Plain `refresh_processes` leaves `cmd` at `UpdateKind::Never`, so every
/// process reports an empty command line. Command lines must be requested
/// explicitly or `--app-port` / `--remoting-auth-token` are never found.
fn cmd_scan_system() -> sysinfo::System {
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing().with_cmd(sysinfo::UpdateKind::Always),
    );
    system
}

fn scan_system() -> Option<(u32, LcuEndpoint)> {
    let system = cmd_scan_system();
    first_endpoint(system.processes().iter().filter_map(|(pid, process)| {
        if !process.name().to_string_lossy().eq_ignore_ascii_case("LeagueClientUx.exe") {
            return None;
        }
        let command = process
            .cmd()
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        Some((pid.as_u32(), command))
    }))
}

/// The first usable endpoint; a client with a malformed command line is skipped.
fn first_endpoint(clients: impl Iterator<Item = (u32, String)>) -> Option<(u32, LcuEndpoint)> {
    for (pid, command) in clients {
        let Some(port) = arg_value(&command, "--app-port=").and_then(|p| p.parse::<u16>().ok())
        else {
            continue;
        };
        let Some(password) = arg_value(&command, "--remoting-auth-token=") else {
            continue;
        };
        if port == 0 || password.is_empty() {
            continue;
        }
        return Some((
            pid,
            LcuEndpoint {
                port,
                password,
                protocol: "https".to_string(),
            },
        ));
    }
    None
}

fn arg_value(command: &str, key: &str) -> Option<String> {
    command.split_whitespace().find_map(|token| {
        token
            .trim_matches('"')
            .strip_prefix(key)
            .map(|value| value.trim_matches('"').to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lockfile_line() {
        let (pid, endpoint) = parse_lockfile("LeagueClient:1234:51234:abcTOKEN:https\n").unwrap();
        assert_eq!(pid, 1234);
        assert_eq!(endpoint.port, 51234);
        assert_eq!(endpoint.password, "abcTOKEN");
        assert_eq!(endpoint.protocol, "https");
    }

    #[test]
    fn parses_lockfile_line_from_ux_process() {
        let (pid, endpoint) = parse_lockfile("LeagueClientUx:1234:51234:abcTOKEN:https\n").unwrap();
        assert_eq!(pid, 1234);
        assert_eq!(endpoint.port, 51234);
    }

    #[test]
    fn rejects_garbage_lockfile_content() {
        assert!(parse_lockfile("").is_none());
        assert!(parse_lockfile("not a lockfile").is_none());
        assert!(parse_lockfile("name:pid:port:password:protocol\n").is_none());
        assert!(parse_lockfile("LeagueClient:123:0:tok:https\n").is_none());
        assert!(parse_lockfile("LeagueClient:0:51234:tok:https\n").is_none());
        assert!(parse_lockfile("OtherProcess:123:51234:tok:https\n").is_none());
    }

    fn endpoint(port: u16) -> LcuEndpoint {
        LcuEndpoint { port, password: "p".into(), protocol: "https".into() }
    }

    #[test]
    fn throttled_scan_reuses_endpoint_while_process_lives() {
        let mut cache = Some((7, endpoint(5000)));
        assert_eq!(cached_endpoint(&mut cache, |pid| pid == 7), Some(endpoint(5000)));
        assert_eq!(cached_endpoint(&mut cache, |pid| pid == 7), Some(endpoint(5000)));
        assert_eq!(cached_endpoint(&mut cache, |_| false), None);
        assert!(cache.is_none());
        assert_eq!(cached_endpoint(&mut None, |_| true), None);
    }

    #[test]
    fn scan_skips_malformed_clients_instead_of_giving_up() {
        let clients = vec![
            (1, "LeagueClientUx.exe --app-port=abc --remoting-auth-token=x".to_string()),
            (2, "LeagueClientUx.exe --remoting-auth-token=x".to_string()),
            (3, "LeagueClientUx.exe --app-port=0 --remoting-auth-token=x".to_string()),
            (4, "LeagueClientUx.exe --app-port=6000 --remoting-auth-token=tok".to_string()),
        ];
        let (pid, found) = first_endpoint(clients.into_iter()).unwrap();
        assert_eq!((pid, found.port, found.password.as_str()), (4, 6000, "tok"));
    }

    #[test]
    fn extracts_lcu_arguments_from_command_line() {
        let command = r#""C:\Riot Games\League of Legends\LeagueClientUx.exe" --app-port=51234 --remoting-auth-token=secret"#;
        assert_eq!(arg_value(command, "--app-port=").as_deref(), Some("51234"));
        assert_eq!(
            arg_value(command, "--remoting-auth-token=").as_deref(),
            Some("secret")
        );
        assert_eq!(arg_value(command, "--missing="), None);
    }

    #[test]
    fn refresh_kind_used_by_scan_populates_command_lines() {
        let system = cmd_scan_system();
        assert!(
            system
                .processes()
                .values()
                .any(|process| !process.cmd().is_empty()),
            "no process reported a command line"
        );
    }

    #[test]
    fn builds_basic_authorization_and_api_url() {
        let endpoint = LcuEndpoint {
            port: 51234,
            password: "abcTOKEN".into(),
            protocol: "https".into(),
        };
        assert_eq!(authorization(&endpoint), "Basic cmlvdDphYmNUT0tFTg==");
        assert_eq!(
            api_url(&endpoint, "/lol-summoner/v1/current-summoner"),
            "https://127.0.0.1:51234/lol-summoner/v1/current-summoner"
        );
    }
}
