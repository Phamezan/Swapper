//! Season-record fallback from u.gg's `fetchProfileRanks`, for players whose
//! record the League Client did not return. Best-effort like `probuilds.rs`:
//! short timeout, a bounded in-memory cache, defensive parsing. Only the viewed
//! player's Riot ID and region are sent, and only on this fallback.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use super::probuilds::{ENDPOINT, SUCCESS_TTL};
use super::provider::ProviderError;
use crate::full_app::model::record_plausible;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const CACHE_LIMIT: usize = 64;
/// u.gg's season id for the ranked season in progress (verified against the
/// client's own record). shortcut: hard-coded, an ended season just yields no
/// record; bump it each season until u.gg exposes the current id.
pub const SEASON_ID: i64 = 26;
const SOLO_QUEUE: &str = "ranked_solo_5x5";

pub const QUERY: &str = "\
query FetchProfileRanks($riotUserName: String!, $riotTagLine: String!, $regionId: String!, $seasonId: Int!) {
  fetchProfileRanks(riotUserName: $riotUserName, riotTagLine: $riotTagLine, regionId: $regionId, seasonId: $seasonId) {
    rankScores { queueType tier rank lp wins losses }
  }
}";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UggRank {
    #[serde(rename = "queueType", default)]
    pub queue: String,
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub lp: u64,
    pub wins: u64,
    pub losses: u64,
}

/// The League Client's region code (`/riotclient/region-locale`) as u.gg's region id.
pub fn region_id(client_region: &str) -> Option<&'static str> {
    Some(match client_region.trim().to_ascii_uppercase().as_str() {
        "EUW" | "EUW1" => "euw1",
        "NA" | "NA1" => "na1",
        "EUNE" | "EUN1" => "eun1",
        "KR" => "kr",
        "BR" | "BR1" => "br1",
        "LAN" | "LA1" => "la1",
        "LAS" | "LA2" => "la2",
        "OCE" | "OC1" => "oc1",
        "TR" | "TR1" => "tr1",
        "RU" => "ru",
        "JP" | "JP1" => "jp1",
        "PH" | "PH2" => "ph2",
        "SG" | "SG2" => "sg2",
        "TH" | "TH2" => "th2",
        "TW" | "TW2" => "tw2",
        "VN" | "VN2" => "vn2",
        "ME" | "ME1" => "me1",
        _ => return None,
    })
}

/// Ranked entries u.gg knows. A player u.gg has never seen is `Ok(vec![])`;
/// unranked slots come back with empty fields and are dropped.
pub fn parse(body: &str) -> Result<Vec<UggRank>, ProviderError> {
    let value: Value = serde_json::from_str(body).map_err(|_| ProviderError::Format)?;
    if value["errors"].as_array().is_some_and(|e| !e.is_empty()) {
        // A player u.gg has no record of is an answer, not an outage.
        let unknown = value["errors"][0]["message"] == "player_info_not_found";
        return if unknown { Ok(vec![]) } else { Err(ProviderError::Unavailable) };
    }
    let scores = value
        .pointer("/data/fetchProfileRanks/rankScores")
        .and_then(Value::as_array)
        .ok_or(ProviderError::Format)?;
    Ok(scores
        .iter()
        .filter_map(|s| serde_json::from_value::<UggRank>(s.clone()).ok())
        .filter(|r| !r.tier.is_empty())
        .collect())
}

/// u.gg's Solo/Duo wins and losses, taken only when its tier is the client's
/// tier and the record is believable. The client's tier and LP always win.
pub fn solo_record(ranks: &[UggRank], client_tier: &str) -> Option<(u64, u64)> {
    let solo = ranks.iter().find(|r| r.queue == SOLO_QUEUE)?;
    (solo.tier.eq_ignore_ascii_case(client_tier) && record_plausible(solo.wins, solo.losses))
        .then_some((solo.wins, solo.losses))
}

type Cache = HashMap<String, (Instant, Vec<UggRank>)>;
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn cached(key: &str, now: Instant) -> Option<Vec<UggRank>> {
    let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let (at, ranks) = cache.as_ref()?.get(key)?;
    (now.saturating_duration_since(*at) < SUCCESS_TTL).then(|| ranks.clone())
}

fn store(key: String, ranks: Vec<UggRank>, now: Instant) {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    cache.retain(|_, (at, _)| now.saturating_duration_since(*at) < SUCCESS_TTL);
    if cache.len() >= CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(key, (now, ranks));
}

pub fn body(name: &str, tag: &str, region_id: &str) -> Value {
    json!({ "query": QUERY, "variables": {
        "riotUserName": name, "riotTagLine": tag, "regionId": region_id, "seasonId": SEASON_ID } })
}

async fn fetch(name: &str, tag: &str, region_id: &str) -> Result<Vec<UggRank>, ProviderError> {
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| ProviderError::Unavailable)?;
    let response = client
        .post(ENDPOINT)
        .json(&body(name, tag, region_id))
        .send()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))?;
    if !response.status().is_success() {
        return Err(ProviderError::from_status(response.status()));
    }
    parse(&response.text().await.map_err(|_| ProviderError::Unavailable)?)
}

/// The player's u.gg Solo/Duo (wins, losses) when it agrees with `client_tier`.
/// Failures are not cached, so a later view may retry; one request per view.
pub async fn lookup_solo(
    name: &str,
    tag: &str,
    client_region: &str,
    client_tier: &str,
) -> Option<(u64, u64)> {
    let region = region_id(client_region)?;
    let key = format!("{region}|{}#{}", name.to_lowercase(), tag.to_lowercase());
    let ranks = match cached(&key, Instant::now()) {
        Some(ranks) => ranks,
        None => {
            let found = fetch(name, tag, region).await;
            super::health::record(super::health::Source::UggStats, "profile ranks", &found);
            let ranks = found.ok()?;
            store(key, ranks.clone(), Instant::now());
            ranks
        }
    };
    solo_record(&ranks, client_tier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rank(queue: &str, tier: &str, wins: u64, losses: u64) -> UggRank {
        UggRank { queue: queue.into(), tier: tier.into(), lp: 22, wins, losses }
    }

    #[test]
    fn parses_ranked_entries_and_drops_unranked_slots() {
        let body = r#"{"data":{"fetchProfileRanks":{"rankScores":[
            {"queueType":"ranked_solo_5x5","tier":"diamond","rank":"IV","lp":22,"wins":21,"losses":15},
            {"queueType":"","tier":"","rank":"","lp":0,"wins":0,"losses":0}]}}}"#;
        assert_eq!(parse(body).unwrap(), vec![rank("ranked_solo_5x5", "diamond", 21, 15)]);
    }

    #[test]
    fn an_unknown_player_is_empty_but_other_errors_and_odd_shapes_are_not() {
        let unknown = r#"{"data":{"fetchProfileRanks":null},"errors":[{"message":"player_info_not_found"}]}"#;
        assert_eq!(parse(unknown).unwrap(), vec![]);
        assert!(matches!(parse(r#"{"errors":[{"message":"boom"}]}"#), Err(ProviderError::Unavailable)));
        assert!(matches!(parse(r#"{"data":{}}"#), Err(ProviderError::Format)));
        assert!(matches!(parse("<html>"), Err(ProviderError::Format)));
    }

    #[test]
    fn maps_client_regions_to_u_gg_region_ids() {
        assert_eq!(region_id("EUW"), Some("euw1"));
        assert_eq!(region_id("na"), Some("na1"));
        assert_eq!(region_id("EUNE"), Some("eun1"));
        assert_eq!(region_id("KR"), Some("kr"));
        assert_eq!(region_id("Mars"), None);
    }

    #[test]
    fn takes_the_record_only_when_the_tier_matches() {
        let ranks = [rank("ranked_solo_5x5", "diamond", 21, 15)];
        assert_eq!(solo_record(&ranks, "DIAMOND"), Some((21, 15)));
        assert_eq!(solo_record(&ranks, "EMERALD"), None);
    }

    #[test]
    fn rejects_an_implausible_or_flex_only_record() {
        assert_eq!(solo_record(&[rank("ranked_solo_5x5", "gold", 151, 0)], "GOLD"), None);
        assert_eq!(solo_record(&[rank("ranked_flex_sr", "gold", 20, 10)], "GOLD"), None);
        assert_eq!(solo_record(&[], "GOLD"), None);
    }

    #[test]
    fn builds_the_verified_variables() {
        let body = body("N", "T", "euw1");
        assert_eq!(body["variables"]["seasonId"], SEASON_ID);
        assert_eq!(body["variables"]["regionId"], "euw1");
    }

    #[test]
    fn the_cache_reuses_for_ten_minutes_and_stays_bounded() {
        let now = Instant::now();
        store("k".into(), vec![rank("ranked_solo_5x5", "gold", 1, 1)], now);
        assert!(cached("k", now + Duration::from_secs(60)).is_some());
        assert!(cached("k", now + SUCCESS_TTL + Duration::from_secs(1)).is_none());
        for i in 0..=CACHE_LIMIT {
            store(format!("b{i}"), vec![], now);
        }
        let size = CACHE.lock().unwrap().as_ref().unwrap().len();
        assert!(size <= CACHE_LIMIT);
    }
}
