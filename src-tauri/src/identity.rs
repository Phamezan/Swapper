use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::lcu::{self, LcuEndpoint};
use crate::riot_client::{self, RiotProbe};
use crate::vault::{self, Account};

const SUMMONER_PATH: &str = "/lol-summoner/v1/current-summoner";
const PLATFORM_PATH: &str = "/lol-platform-config/v1/namespaces/LoginDataPacket/platformId";
const ICON_PATH: &str = "/lol-game-data/assets/v1/profile-icons";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_ICON_BYTES: usize = 256 * 1024;

/// The signed-in Riot account as read from the local Riot Client.
///
/// `puuid` is the canonical identity. The Riot ID is display-only and can
/// change at any time, and `platform` comes from Riot's League account
/// data — never from the Riot ID tagline.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedIdentity {
    pub puuid: String,
    pub game_name: String,
    pub tag_line: String,
    pub platform: Option<String>,
    pub region: Option<String>,
    pub profile_icon_id: Option<i64>,
    pub icon_data_url: Option<String>,
}

impl DetectedIdentity {
    pub fn riot_id(&self) -> String {
        format!("{}#{}", self.game_name, self.tag_line)
    }
}

/// One probe against Riot Client, before it is matched to saved accounts.
#[derive(Clone, Debug)]
pub enum Detection {
    Identified(DetectedIdentity),
    /// The Riot Client is not running (or has no lockfile yet).
    NotRunning,
    /// The Riot Client is reachable but no Riot account is signed in yet.
    NotReady,
    /// The client was found but its API could not be read.
    Unavailable,
}

/// Serializable detection result for the frontend.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum DetectOutcome {
    Identified {
        identity: DetectedIdentity,
        /// Saved account whose PUUID matches the live identity, if any.
        #[serde(rename = "savedAccountId")]
        saved_account_id: Option<Uuid>,
    },
    NotRunning,
    NotReady,
    Unavailable,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentSummoner {
    #[serde(default)]
    puuid: String,
    #[serde(default)]
    game_name: String,
    #[serde(default)]
    tag_line: String,
    #[serde(default)]
    profile_icon_id: Option<i64>,
}

/// Reads the signed-in Riot account from Riot Client. League Client is optional
/// and supplies a profile image only when it belongs to the same PUUID.
pub async fn detect() -> Detection {
    let riot = match riot_client::detect().await {
        RiotProbe::Identified(identity) => identity,
        RiotProbe::NotRunning => return Detection::NotRunning,
        RiotProbe::NotReady => return Detection::NotReady,
        RiotProbe::Unavailable => return Detection::Unavailable,
    };
    let platform = riot.platform.as_deref().and_then(normalize_platform);
    let mut identity = DetectedIdentity {
        puuid: riot.puuid,
        game_name: riot.game_name,
        tag_line: riot.tag_line,
        region: platform.as_deref().map(region_label),
        platform,
        profile_icon_id: riot.profile_icon_id.filter(|id| *id >= 0),
        icon_data_url: None,
    };
    // Profile icon enrichment is cosmetic: an unreadable process table just
    // skips it instead of changing what identity is reported.
    let league_running = tokio::task::spawn_blocking(lcu::league_process_running)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(false);
    if league_running {
        if let Detection::Identified(league) = detect_league().await {
            if league.puuid == identity.puuid {
                if league.profile_icon_id.is_some() {
                    identity.profile_icon_id = league.profile_icon_id;
                }
                identity.icon_data_url = league.icon_data_url;
            }
        }
    }
    Detection::Identified(identity)
}

async fn detect_league() -> Detection {
    let endpoint = match tokio::task::spawn_blocking(lcu::discover).await {
        Ok(Some(endpoint)) => endpoint,
        Ok(None) => return Detection::NotRunning,
        Err(_) => return Detection::Unavailable,
    };
    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(2))
        .timeout(REQUEST_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(_) => return Detection::Unavailable,
    };
    let auth = lcu::authorization(&endpoint);
    let summoner =
        match fetch_json::<CurrentSummoner>(&client, &auth, &endpoint, SUMMONER_PATH).await {
            Ok(Some(summoner)) => summoner,
            // The client exists but could not hand over an identity: it is
            // either still booting / signed out, or the lockfile is stale.
            Ok(None) | Err(_) => return after_failed_fetch().await,
        };
    let puuid = summoner.puuid.trim().to_string();
    let game_name = summoner.game_name.trim().to_string();
    let tag_line = summoner.tag_line.trim().to_string();
    if puuid.is_empty() || game_name.is_empty() || tag_line.is_empty() {
        return Detection::NotReady;
    }
    let profile_icon_id = summoner.profile_icon_id.filter(|id| *id >= 0);
    let platform = fetch_platform(&client, &auth, &endpoint).await;
    let region = platform.as_deref().map(region_label);
    let icon_data_url = match profile_icon_id {
        Some(id) => fetch_icon(&client, &auth, &endpoint, id).await,
        None => None,
    };
    Detection::Identified(DetectedIdentity {
        puuid,
        game_name,
        tag_line,
        platform,
        region,
        profile_icon_id,
        icon_data_url,
    })
}

/// The endpoint answered but no identity came back. Distinguish a client that
/// is still running (keep waiting) from a stale lockfile (client is gone).
async fn after_failed_fetch() -> Detection {
    // When the process table cannot be read, keep waiting (NotReady) rather
    // than declaring the client gone.
    let league_running = tokio::task::spawn_blocking(lcu::league_process_running)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(true);
    if league_running {
        Detection::NotReady
    } else {
        Detection::NotRunning
    }
}

/// Only a known matching PUUID can update a saved account's metadata.
fn can_update_from_detection(account: &Account, identity: &DetectedIdentity) -> bool {
    account.puuid.as_deref() == Some(identity.puuid.as_str())
}

/// Turns a probe into the outcome the frontend sees, refreshing the active
/// account's presentation metadata only when its PUUID is already known.
pub fn outcome_for(detection: &Detection, config: &mut vault::Config) -> Result<DetectOutcome, String> {
    let identity = match detection {
        Detection::Identified(identity) => identity,
        Detection::NotRunning => return Ok(DetectOutcome::NotRunning),
        Detection::NotReady => return Ok(DetectOutcome::NotReady),
        Detection::Unavailable => return Ok(DetectOutcome::Unavailable),
    };
    let mut saved_account_id = config
        .accounts
        .iter()
        .find(|a| a.puuid.as_deref() == Some(identity.puuid.as_str()))
        .map(|a| a.id);
    let mut dirty = false;
    if let Some(active_id) = config.active_id {
        if let Some(active) = config.accounts.iter_mut().find(|a| a.id == active_id) {
            if can_update_from_detection(active, identity) {
                dirty |= apply_identity(active, identity);
                saved_account_id = Some(active.id);
            }
        }
    }
    if dirty {
        vault::save(config)?;
    }
    Ok(DetectOutcome::Identified {
        identity: identity.clone(),
        saved_account_id,
    })
}

/// Refreshes an account's presentation metadata from a verified live identity.
///
/// The PUUID is only ever adopted, never replaced by Riot ID text, and the
/// optional nickname is never touched — except that a legacy account's custom
/// name is migrated into `nickname` the first time real Riot identity arrives,
/// so the user's label is not silently hidden behind the Riot ID.
///
/// Returns whether anything changed.
pub fn apply_identity(account: &mut Account, identity: &DetectedIdentity) -> bool {
    let mut changed = false;
    if account.game_name.is_none() && account.nickname.is_none() {
        let legacy = account.name.trim();
        if !legacy.is_empty() && !legacy.eq_ignore_ascii_case(&identity.riot_id()) {
            account.nickname = Some(legacy.to_string());
            changed = true;
        }
    }
    if account.puuid.as_deref() != Some(identity.puuid.as_str()) {
        account.puuid = Some(identity.puuid.clone());
        changed = true;
    }
    if account.game_name.as_deref() != Some(identity.game_name.as_str()) {
        account.game_name = Some(identity.game_name.clone());
        changed = true;
    }
    if account.tag_line.as_deref() != Some(identity.tag_line.as_str()) {
        account.tag_line = Some(identity.tag_line.clone());
        changed = true;
    }
    if let Some(platform) = &identity.platform {
        if account.platform.as_ref() != Some(platform) {
            account.platform = Some(platform.clone());
            changed = true;
        }
    }
    if let Some(region) = &identity.region {
        if account.region.as_ref() != Some(region) {
            account.region = Some(region.clone());
            changed = true;
        }
    }
    let profile_icon_changed = identity
        .profile_icon_id
        .is_some_and(|profile_icon_id| account.profile_icon_id != Some(profile_icon_id));
    if let Some(profile_icon_id) = identity.profile_icon_id {
        if account.profile_icon_id != Some(profile_icon_id) {
            account.profile_icon_id = Some(profile_icon_id);
            changed = true;
        }
    }
    if let Some(icon_data_url) = identity.icon_data_url.as_deref() {
        // Presentation cache failures must not make identity detection or account
        // switching fail. The letter avatar remains a safe fallback.
        let _ = vault::save_profile_icon(account.id, icon_data_url);
    } else if profile_icon_changed {
        // Do not keep showing an icon we know no longer matches the account.
        let _ = vault::remove_profile_icon(account.id);
    }
    changed
}

async fn fetch_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    auth: &str,
    endpoint: &LcuEndpoint,
    path: &str,
) -> Result<Option<T>, reqwest::Error> {
    let response = client
        .get(lcu::api_url(endpoint, path))
        .header(reqwest::header::AUTHORIZATION, auth)
        .send()
        .await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    response.json().await.map(Some)
}

async fn fetch_platform(
    client: &reqwest::Client,
    auth: &str,
    endpoint: &LcuEndpoint,
) -> Option<String> {
    let response = client
        .get(lcu::api_url(endpoint, PLATFORM_PATH))
        .header(reqwest::header::AUTHORIZATION, auth)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().await.ok()?;
    parse_platform(&body).and_then(|value| normalize_platform(&value))
}

async fn fetch_icon(
    client: &reqwest::Client,
    auth: &str,
    endpoint: &LcuEndpoint,
    id: i64,
) -> Option<String> {
    for extension in ["jpg", "png"] {
        let path = format!("{ICON_PATH}/{id}.{extension}");
        let Ok(response) = client
            .get(lcu::api_url(endpoint, &path))
            .header(reqwest::header::AUTHORIZATION, auth)
            .send()
            .await
        else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(bytes) = response.bytes().await else {
            continue;
        };
        if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
            continue;
        }
        let mime = if extension == "jpg" {
            "image/jpeg"
        } else {
            "image/png"
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Some(format!("data:{mime};base64,{encoded}"));
    }
    None
}

/// Pulls the platform id out of a platform-config response body.
pub fn parse_platform(body: &str) -> Option<String> {
    match serde_json::from_str::<serde_json::Value>(body.trim()) {
        Ok(serde_json::Value::String(value)) => nonempty(&value),
        Ok(serde_json::Value::Object(mut map)) => map
            .remove("platformId")
            .or_else(|| map.remove("value"))
            .and_then(|value| value.as_str().and_then(nonempty)),
        Ok(_) => None,
        Err(_) => nonempty(body),
    }
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('"').trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Accepts only platform-shaped values such as `EUW1` or `NA1`.
pub fn normalize_platform(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('"').trim().to_uppercase();
    if value.is_empty() || value.len() > 16 {
        return None;
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return None;
    }
    Some(value)
}

/// Maps a League platform id to its friendly region label.
///
/// The tagline of a Riot ID is user-controlled and non-authoritative, so the
/// region shown in Swapper is derived only from the League platform id.
pub fn region_label(platform: &str) -> String {
    let label = match platform {
        "BR1" => "BR",
        "EUN1" => "EUNE",
        "EUW1" => "EUW",
        "JP1" => "JP",
        "KR" => "KR",
        "LA1" => "LAN",
        "LA2" => "LAS",
        "NA1" => "NA",
        "OC1" => "OCE",
        "PBE1" => "PBE",
        "PH2" => "PH",
        "SG2" => "SG",
        "TH2" => "TH",
        "TR1" => "TR",
        "TW2" => "TW",
        "VN2" => "VN",
        other => other,
    };
    label.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> DetectedIdentity {
        DetectedIdentity {
            puuid: "PUUID-A".into(),
            game_name: "Example".into(),
            tag_line: "ABC".into(),
            platform: Some("EUW1".into()),
            region: Some("EUW".into()),
            profile_icon_id: Some(4561),
            icon_data_url: None,
        }
    }

    fn account(name: &str, puuid: Option<&str>) -> Account {
        Account {
            id: Uuid::new_v4(),
            name: name.into(),
            puuid: puuid.map(str::to_string),
            game_name: None,
            tag_line: None,
            platform: None,
            region: None,
            profile_icon_id: None,
            nickname: None,
            vault_id: Uuid::new_v4(),
        }
    }

    #[test]
    fn maps_platform_ids_to_friendly_regions() {
        assert_eq!(region_label("EUW1"), "EUW");
        assert_eq!(region_label("NA1"), "NA");
        assert_eq!(region_label("EUN1"), "EUNE");
        assert_eq!(region_label("KR"), "KR");
        assert_eq!(region_label("LA2"), "LAS");
        assert_eq!(region_label("XX9"), "XX9");
    }

    #[test]
    fn parses_platform_from_config_bodies() {
        assert_eq!(parse_platform("\"EUW1\"").as_deref(), Some("EUW1"));
        assert_eq!(parse_platform("EUW1").as_deref(), Some("EUW1"));
        assert_eq!(
            parse_platform("{\"platformId\":\"NA1\"}").as_deref(),
            Some("NA1")
        );
        assert_eq!(
            parse_platform("{\"value\":\"EUN1\"}").as_deref(),
            Some("EUN1")
        );
        assert_eq!(
            parse_platform("{\"errorCode\":\"PLATFORM_CONFIG_NOT_READY\"}"),
            None
        );
        assert_eq!(parse_platform("null"), None);
    }

    #[test]
    fn rejects_platform_values_that_are_not_platform_ids() {
        assert_eq!(normalize_platform("euw1").as_deref(), Some("EUW1"));
        assert_eq!(normalize_platform("EUW 1"), None);
        assert_eq!(normalize_platform(""), None);
        assert_eq!(normalize_platform("Not a platform"), None);
        assert_eq!(normalize_platform("\"\""), None);
    }

    #[test]
    fn taglines_never_influence_the_region() {
        assert_ne!(region_label("EUW1"), "666");
        assert_ne!(region_label("EUW1"), "NA");
        assert_eq!(region_label("NA1"), "NA");
    }

    #[test]
    fn refreshes_riot_id_for_the_same_puuid_without_touching_nickname() {
        let mut account = account("OldName#OLD", Some("PUUID-A"));
        account.nickname = Some("Main".into());
        account.game_name = Some("OldName".into());
        account.tag_line = Some("OLD".into());
        account.platform = Some("EUW1".into());
        account.region = Some("EUW".into());

        let changed = apply_identity(&mut account, &identity());

        assert!(changed);
        assert_eq!(account.puuid.as_deref(), Some("PUUID-A"));
        assert_eq!(account.riot_id().as_deref(), Some("Example#ABC"));
        assert_eq!(account.nickname.as_deref(), Some("Main"));
        assert_eq!(account.display_name(), "Main");
        assert_eq!(account.region.as_deref(), Some("EUW"));
        assert_eq!(account.profile_icon_id, Some(4561));
    }

    #[test]
    fn adopts_identity_without_reporting_changes_when_nothing_differs() {
        let mut account = account("Example#ABC", Some("PUUID-A"));
        account.game_name = Some("Example".into());
        account.tag_line = Some("ABC".into());
        account.platform = Some("EUW1".into());
        account.region = Some("EUW".into());
        account.profile_icon_id = Some(4561);

        assert!(!apply_identity(&mut account, &identity()));
    }

    #[test]
    fn riot_identity_without_optional_league_details_preserves_saved_details() {
        let mut account = account("Example#ABC", Some("PUUID-A"));
        account.game_name = Some("Example".into());
        account.tag_line = Some("ABC".into());
        account.platform = Some("EUW1".into());
        account.region = Some("EUW".into());
        account.profile_icon_id = Some(4561);
        let mut riot = identity();
        riot.platform = None;
        riot.region = None;
        riot.profile_icon_id = None;

        assert!(!apply_identity(&mut account, &riot));
        assert_eq!(account.platform.as_deref(), Some("EUW1"));
        assert_eq!(account.region.as_deref(), Some("EUW"));
        assert_eq!(account.profile_icon_id, Some(4561));
    }

    #[test]
    fn migrates_a_legacy_custom_name_to_the_nickname_on_first_identity() {
        let mut account = account("Main", None);

        apply_identity(&mut account, &identity());

        assert_eq!(account.nickname.as_deref(), Some("Main"));
        assert_eq!(account.puuid.as_deref(), Some("PUUID-A"));
        assert_eq!(account.display_name(), "Main");
        assert_eq!(account.riot_id().as_deref(), Some("Example#ABC"));
    }

    #[test]
    fn observing_an_unknown_login_does_not_relabel_a_legacy_account() {
        let legacy = account("Main", None);
        let legacy_id = legacy.id;
        let mut config = vault::Config {
            accounts: vec![legacy],
            active_id: Some(legacy_id),
            ..vault::Config::default()
        };

        let result = outcome_for(&Detection::Identified(identity()), &mut config).unwrap();

        assert!(matches!(result, DetectOutcome::Identified { saved_account_id: None, .. }));
        assert!(config.accounts[0].puuid.is_none());
        assert_eq!(config.accounts[0].display_name(), "Main");
    }

    #[test]
    fn an_unsaved_login_cannot_prove_ownership_of_a_legacy_vault() {
        let legacy = account("Main", None);
        assert!(!can_update_from_detection(&legacy, &identity()));
    }

    #[test]
    fn detection_result_serializes_saved_account_id_for_frontend() {
        let result = DetectOutcome::Identified {
            identity: identity(),
            saved_account_id: None,
        };
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value.get("savedAccountId"), Some(&serde_json::Value::Null));

        let saved_id = Uuid::new_v4();
        let result = DetectOutcome::Identified {
            identity: identity(),
            saved_account_id: Some(saved_id),
        };
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(
            value.get("savedAccountId").and_then(|id| id.as_str()),
            Some(saved_id.to_string().as_str())
        );
    }
}
