use std::fs::OpenOptions;
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use serde::Deserialize;
use uuid::Uuid;

use crate::vault::LaunchTarget;
use crate::windows::process::SnapshotError;

const USERINFO_PATH: &str = "/rso-auth/v1/authorization/userinfo";
/// How long a switch waits for Riot sign-in, patch readiness, and League startup.
const LAUNCH_WAIT: Duration = Duration::from_secs(60);
const LAUNCH_POLL: Duration = Duration::from_secs(2);
const LAUNCH_RETRY_GRACE: Duration = Duration::from_secs(15);
const PATCH_WAIT: Duration = Duration::from_secs(5 * 60);
/// Consecutive polls without a Riot Client (after seeing it) before giving up.
const GONE_POLLS: u32 = 3;

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
/// so after a switch wait for it to sign in, then ask it to start the target game.
/// Confirms the game actually starts; an accepted POST alone is not success.
/// A Riot Client only target launches nothing and returns at once.
/// Superseded launches stop silently; all other failures reach the caller.
pub async fn launch_when_signed_in(target: LaunchTarget, cancelled: impl Fn() -> bool) -> Result<(), String> {
    let Some(product) = target.product() else { return Ok(()) };
    let game = target.game_name();
    wait_for_launch(game, &cancelled, LAUNCH_WAIT, LAUNCH_POLL, |can_request| {
        poll_launch(target, product, &cancelled, can_request)
    }).await
}

/// Processes that show the product has started.
fn started_images(target: LaunchTarget) -> &'static [&'static str] {
    match target {
        LaunchTarget::Valorant => &["VALORANT.exe", "VALORANT-Win64-Shipping.exe"],
        _ => &["LeagueClient.exe", "LeagueClientUx.exe"],
    }
}

#[derive(Debug)]
enum LaunchPoll {
    Waiting,
    Accepted,
    Updating,
    /// Riot Client answered 423: still starting up, not a refusal.
    Busy,
    /// No Riot Client lockfile/process.
    Gone,
    Running,
}

async fn wait_for_launch<F, Fut>(
    game: &str,
    cancelled: &impl Fn() -> bool,
    wait: Duration,
    interval: Duration,
    mut poll: F,
) -> Result<(), String>
where
    F: FnMut(bool) -> Fut,
    Fut: std::future::Future<Output = Result<LaunchPoll, String>>,
{
    let started = tokio::time::Instant::now();
    let mut last_refusal = None;
    let mut deadline = wait;
    let mut retry_at = Duration::ZERO;
    let mut seen_riot = false;
    let mut gone_polls = 0;
    while started.elapsed() < deadline && !cancelled() {
        let observation = poll(started.elapsed() >= retry_at).await;
        if cancelled() {
            return Ok(());
        }
        if matches!(observation, Ok(LaunchPoll::Gone)) {
            // Riot may not have written its lockfile yet; only count a client that vanished.
            gone_polls = if seen_riot { gone_polls + 1 } else { 0 };
            if gone_polls >= GONE_POLLS {
                return Err("Riot Client closed. Open it to finish signing in.".into());
            }
        } else {
            seen_riot = true;
            gone_polls = 0;
        }
        match observation {
            Ok(LaunchPoll::Running) => return Ok(()),
            Ok(LaunchPoll::Waiting | LaunchPoll::Gone) => {}
            Ok(LaunchPoll::Busy) => {
                // Still starting (HTTP 423): keep waiting while it stays busy, up to the patch cap.
                deadline = deadline.max((started.elapsed() + wait).min(PATCH_WAIT));
                last_refusal = Some("Riot Client is still starting.".into());
            }
            Ok(LaunchPoll::Accepted) => retry_at = started.elapsed() + LAUNCH_RETRY_GRACE,
            Ok(LaunchPoll::Updating) => {
                // The patch budget is measured from launch start, never reset by another 424.
                deadline = deadline.max(PATCH_WAIT);
                last_refusal = Some(format!("{game} is updating."));
            }
            // Riot can refuse while its agreement and patch status are still loading.
            Err(reason) => last_refusal = Some(reason),
        }
        tokio::time::sleep(interval.min(deadline.saturating_sub(started.elapsed()))).await;
    }
    if cancelled() {
        return Ok(());
    }
    if let Some(reason) = &last_refusal { eprintln!("{game} launch failed: {reason}"); }
    Err(format!("{game} didn't start. Open Riot Client to finish any update or sign-in."))
}

async fn poll_launch(target: LaunchTarget, product: &str, cancelled: &impl Fn() -> bool, can_request: bool) -> Result<LaunchPoll, String> {
    let game = target.game_name();
    let game_up = tokio::task::spawn_blocking(move || {
        crate::windows::process::any_running(started_images(target))
    })
    .await
    .map_err(|error| { eprintln!("{game} startup check failed: {error}"); format!("Couldn't check if {game} started.") })?
    .map_err(|error| { eprintln!("{game} startup check failed: {error:?}"); format!("Couldn't check if {game} started.") })?;
    if game_up {
        return Ok(LaunchPoll::Running);
    }
    if !can_request || cancelled() {
        return Ok(LaunchPoll::Waiting);
    }
    match detect().await {
        RiotProbe::Identified(_) => {}
        RiotProbe::NotRunning => return Ok(LaunchPoll::Gone),
        _ => return Ok(LaunchPoll::Waiting),
    }
    let endpoint = tokio::task::spawn_blocking(discover)
        .await
        .map_err(|error| { eprintln!("Riot Client discovery failed: {error}"); "Riot Client wasn't found. Set its path in Settings.".to_string() })?
        .map_err(|error| { eprintln!("Riot Client discovery failed: {error:?}"); "Riot Client wasn't found. Set its path in Settings.".to_string() })?;
    // Discovery can outlive the account action that started this worker.
    if cancelled() {
        return Ok(LaunchPoll::Waiting);
    }
    let path = format!("/product-launcher/v1/products/{product}/patchlines/live");
    let Some(request) = endpoint.and_then(|e| request(&e, reqwest::Method::POST, &path)) else {
        return Ok(LaunchPoll::Waiting);
    };
    let response = request.send().await
        .map_err(|_| "Riot Client didn't answer. Try again.".to_string())?;
    launch_response(game, response.status().as_u16())
}

fn launch_response(game: &str, status: u16) -> Result<LaunchPoll, String> {
    match status {
        200..=299 => Ok(LaunchPoll::Accepted),
        464 => Err(format!("{game} isn't ready yet.")),
        424 => Ok(LaunchPoll::Updating),
        423 => Ok(LaunchPoll::Busy),
        _ => { eprintln!("Riot Client refused {game} launch: HTTP {status}"); Err("Riot Client refused the launch.".into()) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riot_launch_refusals_describe_status_without_assuming_user_action() {
        assert!(launch_response("League", 200).is_ok());
        assert!(launch_response("League", 202).is_ok());
        assert_eq!(launch_response("League", 464).unwrap_err(), "League isn't ready yet.");
        assert!(matches!(launch_response("League", 424), Ok(LaunchPoll::Updating)));
        assert!(matches!(launch_response("League", 423), Ok(LaunchPoll::Busy)));
        assert_eq!(launch_response("League", 503).unwrap_err(), "Riot Client refused the launch.");
        assert_eq!(launch_response("VALORANT", 464).unwrap_err(), "VALORANT isn't ready yet.");
    }

    #[tokio::test(start_paused = true)]
    async fn transient_launch_refusals_retry_until_league_runs() {
        let mut statuses = [464, 424, 503, 202].into_iter();
        let mut polls = 0;
        wait_for_launch("League", &|| false, Duration::from_secs(1), Duration::ZERO, |_| {
            polls += 1;
            std::future::ready(match statuses.next() {
                Some(status) => launch_response("League", status),
                None => Ok(LaunchPoll::Running),
            })
        }).await.expect("Startup refusals must not stop a launch that later succeeds");
        assert_eq!(polls, 5);
    }

    #[tokio::test(start_paused = true)]
    async fn persistent_refusal_reports_last_reason_only_at_deadline() {
        let mut polls = 0;
        let wait = LAUNCH_WAIT;
        let started = tokio::time::Instant::now();
        let error = wait_for_launch("League", &|| false, wait, LAUNCH_POLL, |_| {
            polls += 1;
            std::future::ready(launch_response("League", if polls == 1 { 464 } else { 424 }))
        }).await.unwrap_err();
        assert!(polls > 1, "An early refusal must be retried");
        assert_eq!(started.elapsed(), PATCH_WAIT, "Repeated 424s must not reset the five-minute cap");
        assert_eq!(error, "League didn't start. Open Riot Client to finish any update or sign-in.");
    }

    #[tokio::test(start_paused = true)]
    async fn accepted_request_does_not_erase_last_refusal_before_deadline() {
        let mut statuses = [464, 424].into_iter();
        let started = tokio::time::Instant::now();
        let error = wait_for_launch("League", &|| false, LAUNCH_WAIT, LAUNCH_POLL, |can_request| {
            std::future::ready(if can_request {
                launch_response("League", statuses.next().unwrap_or(202))
            } else {
                Ok(LaunchPoll::Waiting)
            })
        }).await.unwrap_err();
        assert_eq!(started.elapsed(), PATCH_WAIT);
        assert_eq!(error, "League didn't start. Open Riot Client to finish any update or sign-in.");
    }

    #[tokio::test(start_paused = true)]
    async fn accepted_launch_is_not_resent_within_grace() {
        let started = tokio::time::Instant::now();
        let mut requests = Vec::new();
        wait_for_launch("League", &|| false, LAUNCH_WAIT, LAUNCH_POLL, |can_request| {
            let elapsed = started.elapsed();
            let result = if elapsed >= Duration::from_secs(20) {
                LaunchPoll::Running
            } else if can_request {
                requests.push(elapsed);
                LaunchPoll::Accepted
            } else {
                LaunchPoll::Waiting
            };
            std::future::ready(Ok(result))
        }).await.unwrap();
        assert_eq!(requests.len(), 2, "Only resend after the grace expires");
        assert!(requests.windows(2).all(|pair| pair[1] - pair[0] >= LAUNCH_RETRY_GRACE));
    }

    #[tokio::test(start_paused = true)]
    async fn patch_refusal_extends_deadline_past_normal_launch_wait() {
        let started = tokio::time::Instant::now();
        let mut polls = 0;
        wait_for_launch("League", &|| false, LAUNCH_WAIT, LAUNCH_POLL, |_| {
            polls += 1;
            std::future::ready(if polls == 1 {
                launch_response("League", 424)
            } else if started.elapsed() > LAUNCH_WAIT + Duration::from_secs(10) {
                Ok(LaunchPoll::Running)
            } else {
                Ok(LaunchPoll::Waiting)
            })
        }).await.expect("An update may outlast the normal 60-second wait");
        assert!(started.elapsed() > LAUNCH_WAIT);
        assert!(started.elapsed() < PATCH_WAIT);
    }

    #[tokio::test(start_paused = true)]
    async fn riot_busy_past_sixty_seconds_is_not_a_failure() {
        let started = tokio::time::Instant::now();
        wait_for_launch("League", &|| false, LAUNCH_WAIT, LAUNCH_POLL, |_| {
            std::future::ready(if started.elapsed() > Duration::from_secs(64) {
                Ok(LaunchPoll::Running)
            } else {
                launch_response("League", 423)
            })
        }).await.expect("HTTP 423 for over a minute is still starting");
        assert!(started.elapsed() > LAUNCH_WAIT);
    }

    #[tokio::test(start_paused = true)]
    async fn riot_busy_forever_still_stops_at_the_cap() {
        let started = tokio::time::Instant::now();
        wait_for_launch("League", &|| false, LAUNCH_WAIT, LAUNCH_POLL, |_| std::future::ready(launch_response("League", 423)))
            .await.unwrap_err();
        assert_eq!(started.elapsed(), PATCH_WAIT);
    }

    #[tokio::test(start_paused = true)]
    async fn closing_riot_client_stops_the_wait_early() {
        let started = tokio::time::Instant::now();
        let mut polls = 0;
        let error = wait_for_launch("League", &|| false, LAUNCH_WAIT, LAUNCH_POLL, |_| {
            polls += 1;
            std::future::ready(Ok(if polls < 3 { LaunchPoll::Waiting } else { LaunchPoll::Gone }))
        }).await.unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(15));
        assert!(error.starts_with("Riot Client closed"));
    }

    #[tokio::test(start_paused = true)]
    async fn riot_not_started_yet_is_not_treated_as_closed() {
        let mut polls = 0;
        wait_for_launch("League", &|| false, LAUNCH_WAIT, LAUNCH_POLL, |_| {
            polls += 1;
            std::future::ready(Ok(if polls < 6 { LaunchPoll::Gone } else { LaunchPoll::Running }))
        }).await.expect("a lockfile that has not appeared yet must not fail the wait");
    }

    #[tokio::test(start_paused = true)]
    async fn launch_deadline_reports_failure() {
        let error = wait_for_launch("League", &|| false, Duration::ZERO, Duration::ZERO, |_| {
            std::future::ready(Ok(LaunchPoll::Waiting))
        }).await.expect_err("A launch that never starts must report an error");
        assert_eq!(error, "League didn't start. Open Riot Client to finish any update or sign-in.");
    }

    #[tokio::test(start_paused = true)]
    async fn launch_without_patch_refusal_keeps_sixty_second_deadline() {
        let started = tokio::time::Instant::now();
        let error = wait_for_launch("League", &|| false, LAUNCH_WAIT, LAUNCH_POLL, |_| {
            std::future::ready(launch_response("League", 503))
        }).await.unwrap_err();
        assert_eq!(started.elapsed(), LAUNCH_WAIT);
        assert_eq!(error, "League didn't start. Open Riot Client to finish any update or sign-in.");
    }

    #[tokio::test(start_paused = true)]
    async fn switch_during_launch_cancels_stale_result_and_next_play_works() {
        let guard = crate::SwitchGuard::new();
        let first = guard.acquire().unwrap();
        let old_epoch = guard.epoch();
        drop(first);
        let mut switch = None;
        let result = wait_for_launch("League", &|| guard.epoch() != old_epoch,
            Duration::from_secs(1), Duration::ZERO, |_| {
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
        wait_for_launch("League", &|| guard.epoch() != new_epoch,
            Duration::from_secs(1), Duration::ZERO, |_| {
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
