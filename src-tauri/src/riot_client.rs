use std::fs::OpenOptions;
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use serde::Deserialize;
use uuid::Uuid;

use crate::windows::process::SnapshotError;

const USERINFO_PATH: &str = "/rso-auth/v1/authorization/userinfo";
const LAUNCH_LEAGUE_PATH: &str = "/product-launcher/v1/products/league_of_legends/patchlines/live";
/// How long a switch waits for Riot sign-in, patch readiness, and League startup.
const LAUNCH_WAIT: Duration = Duration::from_secs(60);
const LAUNCH_POLL: Duration = Duration::from_secs(2);

struct Endpoint {
    port: u16,
    password: String,
}

pub enum RiotProbe {
    Identified(RiotIdentity),
    NotRunning,
    NotReady,
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RiotIdentity {
    pub puuid: String,
    pub game_name: String,
    pub tag_line: String,
    pub platform: Option<String>,
    pub profile_icon_id: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserInfoEnvelope {
    user_info: String,
}

fn parse_userinfo(body: &str) -> Option<RiotIdentity> {
    let envelope: UserInfoEnvelope = serde_json::from_str(body).ok()?;
    let value: serde_json::Value = serde_json::from_str(&envelope.user_info).ok()?;
    let puuid = Uuid::parse_str(value.get("sub")?.as_str()?.trim()).ok()?;
    let game_name = value.pointer("/acct/game_name")?.as_str()?.trim();
    let tag_line = value.pointer("/acct/tag_line")?.as_str()?.trim();
    if game_name.is_empty() || tag_line.is_empty() {
        return None;
    }
    Some(RiotIdentity {
        puuid: puuid.to_string(),
        game_name: game_name.to_owned(),
        tag_line: tag_line.to_owned(),
        platform: value
            .pointer("/lol/cpid")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        profile_icon_id: value
            .pointer("/lol_account/profile_icon")
            .and_then(|v| v.as_i64()),
    })
}

fn parse_lockfile(body: &str) -> Option<(u32, Endpoint)> {
    let parts: Vec<&str> = body.lines().next()?.trim().split(':').collect();
    let [name, pid, port, password, protocol] = parts.as_slice() else {
        return None;
    };
    let pid: u32 = pid.parse().ok()?;
    let port: u16 = port.parse().ok()?;
    if !name.eq_ignore_ascii_case("Riot Client")
        || pid == 0
        || port == 0
        || password.is_empty()
        || !protocol.eq_ignore_ascii_case("https")
    {
        return None;
    }
    Some((
        pid,
        Endpoint {
            port,
            password: (*password).to_owned(),
        },
    ))
}

/// Reads the Riot Client lockfile and checks that the PID it names really is
/// the Riot Client service.
///
/// `Ok(None)` means there is no usable lockfile (nothing running); `Err`
/// means the process table could not be read, which callers must report as
/// "cannot tell" instead of "not running".
fn discover() -> Result<Option<Endpoint>, SnapshotError> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return Ok(None);
    };
    let path = PathBuf::from(local_app_data).join("Riot Games/Riot Client/Config/lockfile");
    // Riot keeps the lockfile open. Share read/write/delete so we can inspect it.
    let Ok(mut file) = OpenOptions::new().read(true).share_mode(0x7).open(path) else {
        return Ok(None);
    };
    let mut body = String::new();
    if file.read_to_string(&mut body).is_err() {
        return Ok(None);
    }
    let Some((pid, endpoint)) = parse_lockfile(&body) else {
        return Ok(None);
    };
    // One snapshot answers the identity question without refreshing the
    // whole process table the way `System::new_all` used to.
    let image = crate::windows::process::image_for_pid(pid)?;
    Ok(image
        .filter(|name| name.eq_ignore_ascii_case("RiotClientServices.exe"))
        .map(|_| endpoint))
}

/// A request to the local Riot Client API, signed with the lockfile password.
fn request(endpoint: &Endpoint, method: reqwest::Method, path: &str) -> Option<reqwest::RequestBuilder> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
        .ok()?;
    let auth =
        base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", endpoint.password));
    Some(
        client
            .request(method, format!("https://127.0.0.1:{}{path}", endpoint.port))
            .header(reqwest::header::AUTHORIZATION, format!("Basic {auth}")),
    )
}

pub async fn detect() -> RiotProbe {
    let endpoint = match tokio::task::spawn_blocking(discover).await {
        Ok(Ok(Some(endpoint))) => endpoint,
        Ok(Ok(None)) => return RiotProbe::NotRunning,
        // The process table could not be read: report "unavailable" rather
        // than claiming the client is gone.
        Ok(Err(_)) | Err(_) => return RiotProbe::Unavailable,
    };
    let Some(request) = request(&endpoint, reqwest::Method::GET, USERINFO_PATH) else {
        return RiotProbe::Unavailable;
    };
    let response = request.send().await;
    let response = match response {
        Ok(response) => response,
        Err(_) => return RiotProbe::Unavailable,
    };
    if matches!(response.status().as_u16(), 401 | 403 | 404) {
        return RiotProbe::NotReady;
    }
    if !response.status().is_success() {
        return RiotProbe::Unavailable;
    }
    match response.text().await {
        Ok(body) => parse_userinfo(&body)
            .map(RiotProbe::Identified)
            .unwrap_or(RiotProbe::NotReady),
        Err(_) => RiotProbe::Unavailable,
    }
}

/// Riot Client ignores `--launch-product` ("Auto-launch is disabled" in its log),
/// so after a switch wait for it to sign in, then ask it to start League.
/// Confirms League actually starts; an accepted POST alone is not success.
/// Superseded launches stop silently; all other failures reach the caller.
pub async fn launch_league_when_signed_in(cancelled: impl Fn() -> bool) -> Result<(), String> {
    wait_for_league(&cancelled, LAUNCH_WAIT, LAUNCH_POLL, || {
        poll_launch(&cancelled)
    }).await
}

enum LaunchPoll {
    Waiting,
    Running,
}

async fn wait_for_league<F, Fut>(
    cancelled: &impl Fn() -> bool,
    wait: Duration,
    interval: Duration,
    mut poll: F,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<LaunchPoll, String>>,
{
    let started = std::time::Instant::now();
    let mut last_refusal = None;
    while started.elapsed() < wait && !cancelled() {
        let observation = poll().await;
        if cancelled() {
            return Ok(());
        }
        match observation {
            Ok(LaunchPoll::Running) => return Ok(()),
            Ok(LaunchPoll::Waiting) => {}
            // Riot can refuse while its agreement and patch status are still loading.
            Err(reason) => last_refusal = Some(reason),
        }
        tokio::time::sleep(interval.min(wait.saturating_sub(started.elapsed()))).await;
    }
    if cancelled() {
        return Ok(());
    }
    let reason = last_refusal.map(|reason| format!(" Last Riot Client response: {reason}")).unwrap_or_default();
    Err(format!("League did not start.{reason} Open Riot Client to finish any updates or sign-in prompts, then press Play again."))
}

async fn poll_launch(cancelled: &impl Fn() -> bool) -> Result<LaunchPoll, String> {
    let league_up = tokio::task::spawn_blocking(|| {
        crate::windows::process::any_running(&["LeagueClient.exe", "LeagueClientUx.exe"])
    })
    .await
    .map_err(|error| format!("Could not check League startup: {error}"))?
    .map_err(|error| format!("Could not check League startup: {error:?}"))?;
    if league_up {
        return Ok(LaunchPoll::Running);
    }
    if cancelled() || !matches!(detect().await, RiotProbe::Identified(_)) {
        return Ok(LaunchPoll::Waiting);
    }
    let endpoint = tokio::task::spawn_blocking(discover)
        .await
        .map_err(|error| format!("Could not find Riot Client: {error}"))?
        .map_err(|error| format!("Could not find Riot Client: {error:?}"))?;
    // Discovery can outlive the account action that started this worker.
    if cancelled() {
        return Ok(LaunchPoll::Waiting);
    }
    let Some(request) = endpoint.and_then(|e| request(&e, reqwest::Method::POST, LAUNCH_LEAGUE_PATH)) else {
        return Ok(LaunchPoll::Waiting);
    };
    let response = request.send().await
        .map_err(|_| "Riot Client did not answer the League launch request.".to_string())?;
    launch_response(response.status().as_u16())?;
    Ok(LaunchPoll::Waiting)
}

fn launch_response(status: u16) -> Result<(), String> {
    match status {
        200..=299 => Ok(()),
        464 => Err("Riot Client reported that League's agreement status is not ready (HTTP 464).".into()),
        424 => Err("Riot Client reported that League is not up to date (HTTP 424).".into()),
        _ => Err(format!("Riot Client refused to launch League (HTTP {status}).")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riot_launch_refusals_describe_status_without_assuming_user_action() {
        assert!(launch_response(200).is_ok());
        assert!(launch_response(202).is_ok());
        assert!(launch_response(464).unwrap_err().contains("not ready (HTTP 464)"));
        assert!(launch_response(424).unwrap_err().contains("not up to date (HTTP 424)"));
        assert!(launch_response(503).unwrap_err().contains("HTTP 503"));
    }

    #[tokio::test]
    async fn transient_launch_refusals_retry_until_league_runs() {
        let mut statuses = [464, 424, 503, 202].into_iter();
        let mut polls = 0;
        wait_for_league(&|| false, Duration::from_secs(1), Duration::ZERO, || {
            polls += 1;
            std::future::ready(match statuses.next() {
                Some(status) => launch_response(status).map(|()| LaunchPoll::Waiting),
                None => Ok(LaunchPoll::Running),
            })
        }).await.expect("Startup refusals must not stop a launch that later succeeds");
        assert_eq!(polls, 5);
    }

    #[tokio::test]
    async fn persistent_refusal_reports_last_reason_only_at_deadline() {
        let mut polls = 0;
        let wait = Duration::from_millis(20);
        let started = std::time::Instant::now();
        let error = wait_for_league(&|| false, wait, Duration::ZERO, || {
            polls += 1;
            std::future::ready(launch_response(if polls == 1 { 464 } else { 424 })
                .map(|()| LaunchPoll::Waiting))
        }).await.unwrap_err();
        assert!(polls > 1, "An early refusal must be retried");
        assert!(started.elapsed() >= wait, "Only the deadline may end retries");
        assert!(error.contains("424"), "Report the last refusal: {error}");
        assert!(!error.contains("464"), "Do not report an obsolete refusal: {error}");
        assert!(error.contains("Play again"));
    }

    #[tokio::test]
    async fn accepted_request_does_not_erase_last_refusal_before_deadline() {
        let mut statuses = [464, 424].into_iter();
        let error = wait_for_league(&|| false, Duration::from_millis(20), Duration::ZERO, || {
            std::future::ready(launch_response(statuses.next().unwrap_or(202))
                .map(|()| LaunchPoll::Waiting))
        }).await.unwrap_err();
        assert!(error.contains("424"), "An accepted POST does not confirm startup: {error}");
        assert!(!error.contains("464"));
    }

    #[tokio::test]
    async fn accepted_launch_keeps_retrying_until_league_runs() {
        let mut polls = 0;
        let mut requests = 0;
        wait_for_league(&|| false, Duration::from_secs(1), Duration::ZERO, || {
            polls += 1;
            let result = if polls < 3 {
                requests += 1;
                launch_response(202).map(|()| LaunchPoll::Waiting)
            } else {
                Ok(LaunchPoll::Running)
            };
            std::future::ready(result)
        }).await.unwrap();
        assert_eq!(polls, 3, "An accepted POST is not a running League client");
        assert_eq!(requests, 2, "Re-POST while League is not running");
    }

    #[tokio::test]
    async fn launch_deadline_reports_failure() {
        let error = wait_for_league(&|| false, Duration::ZERO, Duration::ZERO, || {
            std::future::ready(Ok(LaunchPoll::Waiting))
        }).await.expect_err("A launch that never starts must report an error");
        assert!(error.contains("Play again"));
    }

    #[tokio::test]
    async fn switch_during_launch_cancels_stale_result_and_next_play_works() {
        let guard = crate::SwitchGuard::new();
        let first = guard.acquire().unwrap();
        let old_epoch = guard.epoch();
        drop(first);
        let mut switch = None;
        let result = wait_for_league(&|| guard.epoch() != old_epoch,
            Duration::from_secs(1), Duration::ZERO, || {
                switch = Some(guard.acquire().unwrap());
                std::future::ready(Err("Old Riot request failed during the switch".into()))
            }).await;
        assert!(result.is_ok(), "A superseded launch must not report a stale failure");
        drop(switch);
        let play = guard.acquire().expect("The next Play must acquire the guard");
        let new_epoch = guard.epoch();
        drop(play);
        let mut requests = 0;
        let mut polls = 0;
        wait_for_league(&|| guard.epoch() != new_epoch,
            Duration::from_secs(1), Duration::ZERO, || {
                polls += 1;
                let result = if polls > 1 { LaunchPoll::Running } else {
                    requests += 1;
                    LaunchPoll::Waiting
                };
                std::future::ready(Ok(result))
            }).await.unwrap();
        assert_eq!(requests, 1);
        assert!(!guard.is_switching());
    }

    #[test]
    fn reads_signed_in_riot_identity_without_league_client() {
        let body = r#"{"userInfo":"{\"sub\":\"12345678-1234-1234-1234-123456789abc\",\"acct\":{\"game_name\":\"Player\",\"tag_line\":\"EUW\"},\"lol\":{\"cpid\":\"EUW1\"},\"lol_account\":{\"profile_icon\":42}}"}"#;
        let identity = parse_userinfo(body).unwrap();
        assert_eq!(identity.puuid, "12345678-1234-1234-1234-123456789abc");
        assert_eq!(identity.game_name, "Player");
        assert_eq!(identity.tag_line, "EUW");
        assert_eq!(identity.platform.as_deref(), Some("EUW1"));
        assert_eq!(identity.profile_icon_id, Some(42));
    }

    #[test]
    fn accepts_only_a_riot_client_https_lockfile() {
        let (pid, endpoint) = parse_lockfile("Riot Client:123:45678:secret:https\n").unwrap();
        assert_eq!(pid, 123);
        assert_eq!(endpoint.port, 45678);
        for body in [
            "LeagueClientUx:123:45678:secret:https",
            "Riot Client:0:45678:secret:https",
            "Riot Client:123:0:secret:https",
            "Riot Client:123:45678::https",
            "Riot Client:123:45678:secret:http",
        ] {
            assert!(parse_lockfile(body).is_none());
        }
    }

    #[test]
    fn incomplete_riot_identity_is_not_accepted() {
        for body in [
            r#"{"userInfo":"{}"}"#,
            r#"{"userInfo":"{\"sub\":\"old\",\"acct\":{\"game_name\":\"Player\",\"tag_line\":\"\"}}"}"#,
            r#"{"userInfo":"{\"sub\":\"not-a-puuid\",\"acct\":{\"game_name\":\"Player\",\"tag_line\":\"EUW\"}}"}"#,
            r#"{"userInfo":"not-json"}"#,
        ] {
            assert!(parse_userinfo(body).is_none());
        }
    }
}
