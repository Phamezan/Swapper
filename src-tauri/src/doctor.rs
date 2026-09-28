//! Swapper Doctor: the Settings health checks.
//!
//! Every check is read-only and bounded by its own timeout, and all checks run
//! concurrently so the Settings view never waits on one slow probe. Results
//! carry a short actionable message only: [`sanitize`] redacts anything that
//! must never reach the UI or the copied report — PUUIDs, Riot IDs, session
//! data, access tokens, pairing secrets and addresses — even if a message
//! somehow picks one up.

use std::future::Future;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tokio::time::timeout;

use crate::identity::{self, Detection};
use crate::remote::RemoteState;
use crate::{riot, updater, vault};

pub const CHECK_RIOT_CLIENT: &str = "riot-client";
pub const CHECK_LEAGUE_CLIENT: &str = "league-client";
pub const CHECK_SESSION: &str = "session";
pub const CHECK_OPGG: &str = "opgg";
pub const CHECK_LOLALYTICS: &str = "lolalytics";
pub const CHECK_PROBUILDS: &str = "probuilds";
pub const CHECK_REMOTE: &str = "remote";
pub const CHECK_TAILSCALE: &str = "tailscale";
pub const CHECK_DECEIVE: &str = "deceive";
pub const CHECK_VERSION: &str = "version";

/// Upper bound for one check. The slowest single probe inside a check is the
/// provider request timeout; the wrapper only catches runaway process scans.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
/// Short on purpose: availability, not a data fetch.
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(5);
const LCU_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DoctorStatus {
    Ok,
    Warn,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorCheck {
    pub id: String,
    /// Human name for the check, resolved from the id on the backend so the
    /// frontend never keeps its own copy.
    pub label: String,
    pub status: DoctorStatus,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
}

impl DoctorCheck {
    fn ok(id: &str, message: impl Into<String>) -> Self {
        Self::new(id, DoctorStatus::Ok, message)
    }

    fn warn(id: &str, message: impl Into<String>) -> Self {
        Self::new(id, DoctorStatus::Warn, message)
    }

    fn error(id: &str, message: impl Into<String>) -> Self {
        Self::new(id, DoctorStatus::Error, message)
    }

    fn new(id: &str, status: DoctorStatus, message: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label(id).to_string(),
            status,
            message: message.into(),
            detail: None,
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Single exit point for every check result: nothing reaches the frontend
    /// or the report unsanitized.
    fn sanitized(self) -> Self {
        Self {
            id: self.id,
            label: self.label,
            status: self.status,
            message: sanitize(&self.message),
            detail: self.detail.map(|detail| sanitize(&detail)),
        }
    }
}

/// Immutable snapshot of the bits of app state the checks read, taken once so
/// no check holds the config lock across an await point.
struct DoctorContext {
    config: vault::Config,
    bundled_deceive: PathBuf,
    remote: std::sync::Arc<crate::remote::RemoteCore>,
    /// Whether the Remote Control Tailscale transport is selected. The Tailscale
    /// CLI is only probed when it is.
    tailscale_selected: bool,
    /// What the background updater last found, if it is running.
    update: Option<updater::UpdateSnapshot>,
}

impl DoctorContext {
    fn capture(
        app: &tauri::AppHandle,
        state: &crate::AppState,
        remote_transport: Option<&str>,
    ) -> Self {
        let config = state
            .config
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        Self {
            config,
            bundled_deceive: state.bundled_deceive.clone(),
            remote: state.remote.clone(),
            tailscale_selected: remote_transport == Some("tailscale"),
            update: app.try_state::<updater::UpdateState>().map(|s| s.snapshot()),
        }
    }

    fn active_account(&self) -> Option<&vault::Account> {
        let active_id = self.config.active_id?;
        self.config.accounts.iter().find(|a| a.id == active_id)
    }
}

#[tauri::command]
pub async fn run_doctor(
    app: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    remote_transport: Option<String>,
) -> Result<Vec<DoctorCheck>, String> {
    let context = DoctorContext::capture(&app, &state, remote_transport.as_deref());
    Ok(run_all(&context).await)
}

/// One check, for the per-check Retry action.
#[tauri::command]
pub async fn run_doctor_check(
    app: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    id: String,
    remote_transport: Option<String>,
) -> Result<DoctorCheck, String> {
    let context = DoctorContext::capture(&app, &state, remote_transport.as_deref());
    Ok(run_one(&context, &id).await)
}

/// The sanitized plain-text report for Copy diagnostics. The frontend sends
/// the checks it is showing; [`render_report`] is the sanitizer of record.
#[tauri::command]
pub fn doctor_report(checks: Vec<DoctorCheck>) -> String {
    render_report(env!("CARGO_PKG_VERSION"), &checks)
}

async fn run_all(context: &DoctorContext) -> Vec<DoctorCheck> {
    let (
        riot_client,
        league_client,
        session,
        opgg,
        lolalytics,
        probuilds,
        remote,
        tailscale,
        deceive,
        version,
    ) = tokio::join!(
        run_one(context, CHECK_RIOT_CLIENT),
        run_one(context, CHECK_LEAGUE_CLIENT),
        run_one(context, CHECK_SESSION),
        run_one(context, CHECK_OPGG),
        run_one(context, CHECK_LOLALYTICS),
        run_one(context, CHECK_PROBUILDS),
        run_one(context, CHECK_REMOTE),
        run_one(context, CHECK_TAILSCALE),
        run_one(context, CHECK_DECEIVE),
        run_one(context, CHECK_VERSION),
    );
    vec![
        riot_client,
        league_client,
        session,
        opgg,
        lolalytics,
        probuilds,
        remote,
        tailscale,
        deceive,
        version,
    ]
}

async fn run_one(context: &DoctorContext, id: &str) -> DoctorCheck {
    match id {
        CHECK_RIOT_CLIENT => guarded(id, check_riot_client()).await,
        CHECK_LEAGUE_CLIENT => guarded(id, check_league_client()).await,
        CHECK_SESSION => guarded(id, check_session(context)).await,
        CHECK_OPGG => guarded(id, check_provider(id, &opgg_probe_url())).await,
        CHECK_LOLALYTICS => {
            guarded(id, check_provider(id, crate::runes::lolalytics::BASE_URL)).await
        }
        CHECK_PROBUILDS => guarded(id, check_provider(id, crate::runes::probuilds::ENDPOINT)).await,
        CHECK_REMOTE | CHECK_TAILSCALE => {
            let remote = context.remote.clone();
            let check = async move {
                if id == CHECK_REMOTE {
                    let summary =
                        tokio::task::spawn_blocking(move || remote.doctor_summary()).await;
                    match summary {
                        Ok(summary) => check_remote(&summary),
                        Err(_) => DoctorCheck::warn(
                            id,
                            "Could not read the Remote Control status. Retry.",
                        ),
                    }
                } else {
                    // The Tailscale CLI is only run when its transport is selected.
                    match context.tailscale_selected {
                        false => DoctorCheck::ok(
                            id,
                            "Not selected. Remote Control is using the LAN transport; select Tailscale in Settings to use it.",
                        ),
                        true => {
                            let availability =
                                tokio::task::spawn_blocking(move || remote.tailscale_availability())
                                    .await;
                            match availability {
                                Ok((installed, running)) => check_tailscale(installed, running),
                                Err(_) => DoctorCheck::warn(
                                    id,
                                    "Could not read the Tailscale status. Retry.",
                                ),
                            }
                        }
                    }
                }
            };
            guarded(id, check).await
        }
        CHECK_DECEIVE => guarded(id, async {
            // deceive_path walks the process table; keep it off the async threads,
            // together with the version-resource read for the found executable.
            let config = context.config.clone();
            let bundled = context.bundled_deceive.clone();
            let found = tokio::task::spawn_blocking(move || {
                riot::deceive_path(&config, &bundled).map(|path| {
                    let version = crate::windows::file_version(&path);
                    (path, version)
                })
            })
            .await
            .ok()
            .flatten();
            match found {
                Some((path, version)) if path == context.bundled_deceive => with_detail_or_none(
                    DoctorCheck::ok(id, "Bundled Deceive is available."),
                    version.map(|version| format!("File version {version}")),
                ),
                Some((_, version)) => with_detail_or_none(
                    DoctorCheck::ok(id, "Deceive was found on this PC."),
                    version.map(|version| format!("File version {version}")),
                ),
                None if context.config.use_deceive => DoctorCheck::error(
                    id,
                    "Deceive.exe is missing. Reinstall Swapper or turn off Launch through Deceive.",
                ),
                None => DoctorCheck::warn(
                    id,
                    "Deceive.exe is missing. Reinstall Swapper to launch through Deceive.",
                ),
            }
        })
        .await,
        CHECK_VERSION => {
            guarded(id, async {
                version_check(context.update.as_ref(), env!("CARGO_PKG_VERSION"))
            })
            .await
        }
        _ => DoctorCheck::error(id, "Unknown check."),
    }
}

/// Wraps one check so a stuck probe can never hang the Settings view.
async fn guarded(id: &str, check: impl Future<Output = DoctorCheck>) -> DoctorCheck {
    match timeout(CHECK_TIMEOUT, check).await {
        Ok(result) => result.sanitized(),
        Err(_) => DoctorCheck::warn(id, "This check timed out. Retry.").sanitized(),
    }
}

async fn check_riot_client() -> DoctorCheck {
    match crate::riot_client::detect().await {
        crate::riot_client::RiotProbe::Identified(_) => {
            DoctorCheck::ok(CHECK_RIOT_CLIENT, "Riot Client is connected.")
        }
        crate::riot_client::RiotProbe::NotRunning => DoctorCheck::warn(
            CHECK_RIOT_CLIENT,
            "Riot Client is not running. Start it to switch or add accounts.",
        ),
        crate::riot_client::RiotProbe::NotReady => DoctorCheck::warn(
            CHECK_RIOT_CLIENT,
            "Riot Client is open but not signed in yet. Sign in, then retry.",
        ),
        crate::riot_client::RiotProbe::Unavailable => DoctorCheck::error(
            CHECK_RIOT_CLIENT,
            "Riot Client could not be reached. Restart it, then retry.",
        ),
    }
}

async fn check_league_client() -> DoctorCheck {
    let endpoint = tokio::task::spawn_blocking(crate::lcu::discover)
        .await
        .ok()
        .flatten();
    if let Some(endpoint) = endpoint {
        let answered = probe_lcu(&endpoint).await;
        return if answered {
            DoctorCheck::ok(CHECK_LEAGUE_CLIENT, "League Client is running and answering.")
        } else {
            DoctorCheck::warn(
                CHECK_LEAGUE_CLIENT,
                "League Client was found but is not answering yet. If it just started, retry in a moment.",
            )
        };
    }
    let running = tokio::task::spawn_blocking(crate::lcu::league_process_running)
        .await
        .ok();
    match running {
        Some(Ok(true)) => DoctorCheck::warn(
            CHECK_LEAGUE_CLIENT,
            "League Client is starting. Retry in a moment.",
        ),
        Some(Ok(false)) => DoctorCheck::ok(
            CHECK_LEAGUE_CLIENT,
            "League Client is not running. It is only needed for runes and champion select.",
        ),
        _ => DoctorCheck::error(
            CHECK_LEAGUE_CLIENT,
            "Swapper could not inspect running processes. Restart Swapper, then retry.",
        ),
    }
}

/// One authorized GET against the found endpoint. Any HTTP answer proves the
/// API is up; the body is discarded and nothing from it is read.
async fn probe_lcu(endpoint: &crate::lcu::LcuEndpoint) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(LCU_PROBE_TIMEOUT)
        .timeout(LCU_PROBE_TIMEOUT)
        .build()
    else {
        return false;
    };
    let response = client
        .get(crate::lcu::api_url(endpoint, "/"))
        .header(
            reqwest::header::AUTHORIZATION,
            crate::lcu::authorization(endpoint),
        )
        .send()
        .await;
    response.is_ok()
}

/// Coarse session health: an active saved account whose snapshot opens, and —
/// when Riot Client is running — whether the live login is the same account.
/// The output never names the account.
async fn check_session(context: &DoctorContext) -> DoctorCheck {
    if context.config.accounts.is_empty() {
        return DoctorCheck::warn(
            CHECK_SESSION,
            "No accounts are saved yet. Add an account in Swapper to get started.",
        );
    }
    let Some(active) = context.active_account() else {
        return DoctorCheck::warn(
            CHECK_SESSION,
            "No account is active. Choose one in the account list or add a new account.",
        );
    };
    let snapshot_id = active.vault_id;
    let snapshot_ok = tokio::task::spawn_blocking(move || vault::read_snapshot(snapshot_id).is_ok())
        .await
        .unwrap_or(false);
    if !snapshot_ok {
        return DoctorCheck::error(
            CHECK_SESSION,
            "The active account's saved session could not be opened. Switch to it once; if that fails, remove and re-add the account.",
        );
    }
    match identity::detect_account().await {
        Detection::Identified(found) => match &active.puuid {
            None => DoctorCheck::warn(
                CHECK_SESSION,
                "Saved session is present, but this account was saved without an identity check. Switch to it once to re-verify.",
            ),
            Some(puuid) if *puuid == found.puuid => DoctorCheck::ok(
                CHECK_SESSION,
                "The active account matches the signed-in Riot Client.",
            ),
            Some(_) => DoctorCheck::warn(
                CHECK_SESSION,
                "The signed-in Riot account is not the active Swapper account. Switch accounts to bring the session back in sync.",
            ),
        },
        Detection::NotRunning => DoctorCheck::ok(
            CHECK_SESSION,
            "Saved session is present. Riot Client is closed, so it could not be verified.",
        ),
        Detection::NotReady => DoctorCheck::warn(
            CHECK_SESSION,
            "Saved session is present. Riot Client is still signing in; retry to verify it.",
        ),
        Detection::Unavailable => DoctorCheck::warn(
            CHECK_SESSION,
            "Saved session is present, but Riot Client could not be reached to verify it.",
        ),
    }
}

fn opgg_probe_url() -> String {
    format!(
        "{}{}",
        crate::runes::opgg::BASE_URL,
        crate::runes::opgg::path(
            crate::runes::opgg::DEFAULT_REGION,
            crate::runes::opgg::MODE_RANKED,
            1,
            crate::runes::opgg::POSITION_NONE,
            crate::runes::opgg::TIER_ALL,
        )
    )
}

/// Availability and latency for one stats site. A host that answers is not
/// automatically healthy: the status code is classified, and the body is
/// never read.
async fn check_provider(id: &str, url: &str) -> DoctorCheck {
    let client = provider_client();
    let started = Instant::now();
    // HEAD first: the cheapest probe. A rejected HEAD still gets one GET,
    // because some hosts refuse HEAD outright. After a timeout a GET would hit
    // the same wall, so it is skipped.
    let mut last_error = None;
    for method in [reqwest::Method::HEAD, reqwest::Method::GET] {
        match client.request(method, url).send().await {
            Ok(response) => {
                let milliseconds = started.elapsed().as_millis();
                let (status, message) = provider_answer_status(response.status().as_u16());
                let check = DoctorCheck::new(id, status, message);
                return match status {
                    DoctorStatus::Ok => check.with_detail(format!("{milliseconds} ms")),
                    _ => check,
                };
            }
            Err(error) => {
                let timed_out = error.is_timeout();
                last_error = Some(error);
                if timed_out {
                    break;
                }
            }
        }
    }
    match last_error {
        Some(error) if error.is_timeout() => DoctorCheck::warn(
            id,
            "Timed out. The site may be slow or blocked by your network; retry.",
        ),
        Some(error) if error.is_connect() => DoctorCheck::error(
            id,
            "Could not connect. Check your internet connection, then retry.",
        ),
        Some(_) => DoctorCheck::error(
            id,
            "The request failed before an answer came back. Check your internet connection, then retry.",
        ),
        None => DoctorCheck::error(id, "The request could not be sent. Retry."),
    }
}

/// Classifies a provider's HTTP answer. API roots legitimately answer 400 or
/// 404 (the u.gg endpoint is POST-only), so other client errors still count as
/// available; an outright block (403/429) or a server-side outage (5xx) does not.
fn provider_answer_status(status: u16) -> (DoctorStatus, &'static str) {
    match status {
        403 | 429 => (DoctorStatus::Warn, "Blocked or rate-limited by the site."),
        500..=599 => (DoctorStatus::Error, "The site is having problems right now."),
        _ => (DoctorStatus::Ok, "Reachable."),
    }
}

static PROVIDER_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn provider_client() -> &'static reqwest::Client {
    PROVIDER_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("Swapper/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(PROVIDER_TIMEOUT)
            .timeout(PROVIDER_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

/// The Swapper version check: the installed version plus the updater's last
/// verdict. Read-only — it never triggers a check, it reports the updater state.
fn version_check(snapshot: Option<&updater::UpdateSnapshot>, installed: &str) -> DoctorCheck {
    let Some(snapshot) = snapshot else {
        return DoctorCheck::warn(CHECK_VERSION, "Could not read the updater status. Retry.");
    };
    match snapshot.state {
        updater::UpdateStateKind::NotConfigured => DoctorCheck::ok(
            CHECK_VERSION,
            format!(
                "Swapper {installed} is installed. Update checking is not configured in this build."
            ),
        ),
        updater::UpdateStateKind::Checking => DoctorCheck::ok(
            CHECK_VERSION,
            format!("Swapper {installed} is installed. Checking for updates…"),
        ),
        updater::UpdateStateKind::UpToDate => {
            DoctorCheck::ok(CHECK_VERSION, format!("Swapper {installed} is up to date."))
        }
        updater::UpdateStateKind::Available => {
            let available = snapshot.available_version.as_deref().unwrap_or("newer");
            DoctorCheck::ok(
                CHECK_VERSION,
                format!(
                    "Swapper {installed} is installed, and version {available} is available. Use Update & Restart to install it."
                ),
            )
        }
        updater::UpdateStateKind::Installing => DoctorCheck::ok(
            CHECK_VERSION,
            "An update is installing. Swapper restarts when it finishes.",
        ),
        updater::UpdateStateKind::Failed => DoctorCheck::warn(
            CHECK_VERSION,
            "The last update check failed. Retry, or check for updates in Settings.",
        ),
    }
}

fn check_remote(summary: &crate::remote::DoctorSummary) -> DoctorCheck {    if !summary.enabled {
        return DoctorCheck::ok(
            CHECK_REMOTE,
            "Remote Control is off. Turn it on in Settings to control League from a phone.",
        );
    }
    let detail = summary
        .interface
        .as_deref()
        .map(|name| format!("Adapter: {name}"));
    if summary.lan_ready {
        return with_detail_or_none(
            DoctorCheck::ok(CHECK_REMOTE, "LAN Remote Control is ready."),
            detail,
        );
    }
    if summary.state == RemoteState::Starting {
        return with_detail_or_none(
            DoctorCheck::warn(CHECK_REMOTE, "The remote service is starting. Retry in a moment."),
            detail,
        );
    }
    let reason = summary
        .lan_message
        .as_deref()
        .or(summary.message.as_deref())
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            "LAN is unavailable. Set the active Windows network to Private, then retry.".into()
        });
    with_detail_or_none(DoctorCheck::warn(CHECK_REMOTE, reason), detail)
}

fn check_tailscale(installed: bool, running: bool) -> DoctorCheck {
    if !installed {
        return DoctorCheck::ok(
            CHECK_TAILSCALE,
            "Tailscale is not installed. It is optional and only used to reach Swapper from outside your home network.",
        );
    }
    if running {
        DoctorCheck::ok(CHECK_TAILSCALE, "Tailscale is installed and connected.")
    } else {
        DoctorCheck::warn(
            CHECK_TAILSCALE,
            "Tailscale is installed but not connected. Start Tailscale to use it for Remote Control.",
        )
    }
}

fn with_detail_or_none(mut check: DoctorCheck, detail: Option<String>) -> DoctorCheck {
    check.detail = detail;
    check
}

fn status_word(status: DoctorStatus) -> &'static str {
    match status {
        DoctorStatus::Ok => "ok",
        DoctorStatus::Warn => "warn",
        DoctorStatus::Error => "error",
    }
}

fn label(id: &str) -> &'static str {
    match id {
        CHECK_RIOT_CLIENT => "Riot Client",
        CHECK_LEAGUE_CLIENT => "League Client / LCU",
        CHECK_SESSION => "Active account session",
        CHECK_OPGG => "OP.GG",
        CHECK_LOLALYTICS => "Lolalytics",
        CHECK_PROBUILDS => "ProBuildStats / U.GG",
        CHECK_REMOTE => "LAN Remote Control",
        CHECK_TAILSCALE => "Tailscale",
        CHECK_DECEIVE => "Deceive",
        CHECK_VERSION => "Swapper version",
        _ => "Check",
    }
}

/// The plain-text report behind Copy diagnostics. Every line goes through
/// [`sanitize`], so the report is safe even if a message picked up something
/// it should never contain.
fn render_report(version: &str, checks: &[DoctorCheck]) -> String {
    let generated = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let mut report = format!("Swapper Doctor\nSwapper {version}\nGenerated {generated} (Unix time)\n");
    for check in checks {
        report.push_str(&format!(
            "\n{} [{}]\n{}\n",
            label(&check.id),
            status_word(check.status),
            sanitize(&check.message)
        ));
        if let Some(detail) = &check.detail {
            let detail = sanitize(detail);
            if !detail.is_empty() {
                report.push_str(&detail);
                report.push('\n');
            }
        }
    }
    report.push_str(
        "\nThis report never contains PUUIDs, Riot IDs, sessions, access tokens, pairing secrets, cookies or vault contents.\n",
    );
    report
}

const REDACTED: &str = "[redacted]";
const TOKEN_PUNCT: &[char] = &[
    '(', ')', '[', ']', '{', '}', '<', '>', '"', '\'', ',', '.', ':', ';', '!', '?', '*',
];

/// Redacts anything that must never appear in diagnostics. Over-redacts on
/// purpose: Swapper's own messages never need an identifier, a token or an
/// address, so anything shaped like one is dropped.
fn sanitize(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let token = word.trim_matches(TOKEN_PUNCT);
            if is_sensitive(token) {
                word.replace(token, REDACTED)
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_sensitive(token: &str) -> bool {
    // PUUIDs are UUIDs.
    if uuid::Uuid::parse_str(token).is_ok() {
        return true;
    }
    // Addresses, with or without a port ("192.168.0.10:38127").
    if looks_like_ipv4(token)
        || token
            .split_once(':')
            .is_some_and(|(host, _)| looks_like_ipv4(host))
    {
        return true;
    }
    let lower = token.to_ascii_lowercase();
    if ["token=", "password=", "secret=", "auth=", "bearer", "riot:"]
        .iter()
        .any(|key| lower.contains(*key))
    {
        return true;
    }
    // Riot-ID shape (GameName#TagLine). Swapper's messages never use '#', so
    // anything word#tag-shaped is treated as an identifier.
    if let Some((name, tag)) = token.split_once('#') {
        if name.len() >= 2
            && tag.len() >= 3
            && name.chars().all(|c| c.is_ascii_alphanumeric())
            && tag.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return true;
        }
    }
    // Long opaque blobs: base64 or hex session material, pairing credentials,
    // lockfile passwords.
    token.len() >= 24
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-'))
        && token.chars().any(|c| c.is_ascii_digit())
        && token.chars().any(|c| c.is_ascii_alphabetic())
}

fn looks_like_ipv4(token: &str) -> bool {
    let mut parts = token.split('.');
    let mut count = 0;
    for part in parts.by_ref() {
        count += 1;
        if count > 4
            || part.is_empty()
            || part.len() > 3
            || !part.chars().all(|c| c.is_ascii_digit())
            || part.parse::<u8>().is_err()
        {
            return false;
        }
    }
    count == 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_redacts_identifiers_secrets_and_addresses() {
        let sanitized = sanitize(
            "Session 123e4567-e89b-12d3-a456-426614174000 for Player#EUW1 with Basic \
             cmlvdDphYmNUT0tFTk5PUGVn and token=abcdef0123456789abcdef0123456789 seen on \
             192.168.1.42:51234 stayed readable.",
        );
        for secret in [
            "123e4567",
            "Player#EUW1",
            "cmlvdDphYmNUT0tFTk5PUGVn",
            "abcdef0123456789",
            "192.168.1.42",
        ] {
            assert!(!sanitized.contains(secret), "sanitize leaked {secret}");
        }
        assert!(sanitized.contains(REDACTED));
        assert!(sanitized.contains("stayed"));
    }

    #[test]
    fn sanitize_keeps_plain_diagnostics_text() {
        for line in [
            "Riot Client is connected.",
            "Reachable.",
            "Swapper 0.4.0 is installed. Update checking is not configured in this build.",
            "Swapper 0.4.0 is installed, and version 0.5.0 is available. Use Update & Restart to install it.",
            "Bundled Deceive is available.",
            "File version 1.18.0",
            "LAN Remote Control is ready.",
            "Timed out. The site may be slow or blocked by your network; retry.",
            "Adapter: Wi-Fi",
        ] {
            assert_eq!(sanitize(line), line);
        }
    }

    #[test]
    fn version_check_reports_the_updater_verdict() {
        fn snapshot(state: updater::UpdateStateKind) -> updater::UpdateSnapshot {
            updater::UpdateSnapshot {
                state,
                current_version: "0.4.0".into(),
                available_version: None,
                notes: None,
                message: None,
                last_check: None,
            }
        }

        let unconfigured = version_check(
            Some(&snapshot(updater::UpdateStateKind::NotConfigured)),
            "0.4.0",
        );
        assert_eq!(unconfigured.status, DoctorStatus::Ok);
        assert!(unconfigured.message.contains("0.4.0"));
        assert!(unconfigured.message.contains("not configured in this build"));

        let up_to_date = version_check(
            Some(&snapshot(updater::UpdateStateKind::UpToDate)),
            "0.4.0",
        );
        assert_eq!(up_to_date.status, DoctorStatus::Ok);
        assert!(up_to_date.message.contains("up to date"));

        let available = version_check(
            Some(&updater::UpdateSnapshot {
                available_version: Some("0.5.0".into()),
                ..snapshot(updater::UpdateStateKind::Available)
            }),
            "0.4.0",
        );
        assert_eq!(available.status, DoctorStatus::Ok);
        assert!(available.message.contains("0.5.0"));

        let failed = version_check(Some(&snapshot(updater::UpdateStateKind::Failed)), "0.4.0");
        assert_eq!(failed.status, DoctorStatus::Warn);
        assert!(failed.message.contains("failed"));

        let missing = version_check(None, "0.4.0");
        assert_eq!(missing.status, DoctorStatus::Warn);
    }

    #[test]
    fn version_numbers_are_not_treated_as_addresses() {
        assert!(looks_like_ipv4("192.168.1.42"));
        assert!(!looks_like_ipv4("0.4.0"));
        assert!(!looks_like_ipv4("1.2.3.4.5"));
        assert!(!looks_like_ipv4("51234"));
    }

    #[test]
    fn report_omits_identifiers_secrets_and_addresses() {
        let checks = vec![
            DoctorCheck::ok(
                CHECK_SESSION,
                "Active account Player#EUW1 with puuid 123e4567-e89b-12d3-a456-426614174000 verified",
            ),
            DoctorCheck::warn(
                CHECK_REMOTE,
                "Could not listen on 192.168.0.10:38127 with credential af0bcd1234567890af0bcd1234567890a",
            ),
            DoctorCheck::ok(CHECK_VERSION, "Swapper 0.4.0 is installed.").with_detail("132 ms"),
        ];
        let report = render_report("0.4.0", &checks);
        for secret in [
            "Player#EUW1",
            "123e4567",
            "192.168.0.10",
            "af0bcd1234567890",
        ] {
            assert!(!report.contains(secret), "report leaked {secret}");
        }
        assert!(report.contains("Active account [redacted]"));
        assert!(report.contains("[ok]"));
        assert!(report.contains("[warn]"));
        assert!(report.contains("132 ms"));
    }

    #[test]
    fn provider_status_classification_follows_the_review_rules() {
        // 2xx/3xx and 4xx other than 403/429: API roots legitimately answer
        // 400/404/405 (u.gg/api is POST-only).
        for status in [200, 204, 301, 302, 400, 404, 405] {
            let (status_kind, message) = provider_answer_status(status);
            assert_eq!(status_kind, DoctorStatus::Ok, "{status} should be ok");
            assert_eq!(message, "Reachable.");
        }
        for status in [403, 429] {
            let (status_kind, message) = provider_answer_status(status);
            assert_eq!(status_kind, DoctorStatus::Warn, "{status} should warn");
            assert!(message.contains("rate-limited"));
        }
        for status in [500, 502, 503] {
            let (status_kind, message) = provider_answer_status(status);
            assert_eq!(status_kind, DoctorStatus::Error, "{status} should error");
            assert!(message.contains("having problems"));
        }
    }

    #[test]
    fn a_blocked_provider_answer_carries_no_latency_and_an_ok_one_does() {
        let blocked = DoctorCheck::new(CHECK_OPGG, DoctorStatus::Warn, "Blocked or rate-limited by the site.");
        assert!(blocked.detail.is_none());

        let healthy = DoctorCheck::new(CHECK_OPGG, DoctorStatus::Ok, "Reachable.")
            .with_detail("132 ms");
        assert_eq!(healthy.detail.as_deref(), Some("132 ms"));
    }

    #[test]
    fn every_check_id_has_a_report_label() {
        for id in [
            CHECK_RIOT_CLIENT,
            CHECK_LEAGUE_CLIENT,
            CHECK_SESSION,
            CHECK_OPGG,
            CHECK_LOLALYTICS,
            CHECK_PROBUILDS,
            CHECK_REMOTE,
            CHECK_TAILSCALE,
            CHECK_DECEIVE,
            CHECK_VERSION,
        ] {
            assert_ne!(label(id), "Check", "check {id} has no report label");
        }
    }

    #[test]
    fn checks_carry_their_backend_label() {
        let check = DoctorCheck::ok(CHECK_RIOT_CLIENT, "Riot Client is connected.");
        assert_eq!(check.id, CHECK_RIOT_CLIENT);
        assert_eq!(check.label, "Riot Client");
    }

    #[test]
    fn status_words_cover_every_status() {
        assert_eq!(status_word(DoctorStatus::Ok), "ok");
        assert_eq!(status_word(DoctorStatus::Warn), "warn");
        assert_eq!(status_word(DoctorStatus::Error), "error");
    }

    #[test]
    fn check_results_are_sanitized_before_serialization() {
        let check = DoctorCheck::ok(CHECK_VERSION, "Signed in as Player#EUW1")
            .with_detail("password=hunter2hunter2hunter2");
        let sanitized = check.sanitized();
        assert!(!sanitized.message.contains("Player#EUW1"));
        assert!(!sanitized.detail.as_deref().unwrap_or("").contains("hunter2"));
    }
}
