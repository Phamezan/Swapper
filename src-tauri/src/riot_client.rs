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

pub async fn detect() -> RiotProbe {
    let endpoint = match tokio::task::spawn_blocking(discover).await {
        Ok(Ok(Some(endpoint))) => endpoint,
        Ok(Ok(None)) => return RiotProbe::NotRunning,
        // The process table could not be read: report "unavailable" rather
        // than claiming the client is gone.
        Ok(Err(_)) | Err(_) => return RiotProbe::Unavailable,
    };
    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
    {
        Ok(client) => client,
        Err(_) => return RiotProbe::Unavailable,
    };
    let auth =
        base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", endpoint.password));
    let response = client
        .get(format!(
            "https://127.0.0.1:{}{USERINFO_PATH}",
            endpoint.port
        ))
        .header(reqwest::header::AUTHORIZATION, format!("Basic {auth}"))
        .send()
        .await;
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

#[cfg(test)]
mod tests {
    use super::*;

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
