//! probuildstats / u.gg GraphQL client for pros' solo-queue builds.
//!
//! probuildstats.com is run by the same company as u.gg and serves pros' SOLO
//! QUEUE games, not pro play. The site is a React app that talks to an
//! undocumented GraphQL endpoint; the query below is the `ChampionMatchList`
//! operation taken verbatim from its `main.*.js` bundle, trimmed to the fields
//! the rune screen needs.
//!
//! Because the endpoint is undocumented it is treated as best-effort: a short
//! timeout, a per-champion cache, a short negative cache, and defensive parsing
//! that drops any match it cannot turn into a rune page instead of failing the
//! whole list.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use super::RuneError;

/// The GraphQL endpoint. The same API also answers at `https://u.gg/api`.
pub const ENDPOINT: &str = "https://u.gg/api";
/// Matches the API returns for one page.
pub const PAGE_SIZE: usize = 20;
/// A successful lookup is reused for this long, so one champion select makes a
/// single request per champion (plus explicit "load more").
pub const SUCCESS_TTL: Duration = Duration::from_secs(10 * 60);
/// A failed or unavailable lookup is remembered this long before retrying.
pub const FAILURE_TTL: Duration = Duration::from_secs(60);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

/// The verified `ChampionMatchList` operation. The argument names, the paging
/// variable `$pageNumber` (1-based) and the role values come from the bundle.
pub const QUERY: &str = "\
query ChampionMatchList($championId: Int!, $role: String, $pageNumber: Int, $isOtp: Boolean) {
  getProChampionMatchList(championId: $championId, role: $role, pageNumber: $pageNumber, isOtp: $isOtp) {
    matchList {
      matchId
      normalizedName
      proLeague
      currentTeam
      calculatedRole
      version
      win
      matchTimestamp
      totalKills
      totalDeaths
      totalAssists
      proInfo { officialName league currentTeam }
      runes { perk0 perk1 perk2 perk3 perk4 perk5 primaryStyle subStyle }
      statShards
    }
  }
}";

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProInfo {
    #[serde(default)]
    pub official_name: String,
    #[serde(default)]
    pub league: String,
    #[serde(default)]
    pub current_team: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProRunes {
    #[serde(default)]
    pub perk0: Option<i64>,
    #[serde(default)]
    pub perk1: Option<i64>,
    #[serde(default)]
    pub perk2: Option<i64>,
    #[serde(default)]
    pub perk3: Option<i64>,
    #[serde(default)]
    pub perk4: Option<i64>,
    #[serde(default)]
    pub perk5: Option<i64>,
    #[serde(default)]
    pub primary_style: Option<i64>,
    #[serde(default)]
    pub sub_style: Option<i64>,
}

/// One pro solo-queue game, as the API reports it.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProMatch {
    #[serde(default)]
    pub match_id: i64,
    #[serde(default)]
    pub normalized_name: String,
    #[serde(default)]
    pub pro_league: String,
    #[serde(default)]
    pub current_team: String,
    #[serde(default)]
    pub calculated_role: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub win: bool,
    /// Milliseconds since the Unix epoch.
    #[serde(default)]
    pub match_timestamp: i64,
    #[serde(default)]
    pub total_kills: i64,
    #[serde(default)]
    pub total_deaths: i64,
    #[serde(default)]
    pub total_assists: i64,
    #[serde(default)]
    pub pro_info: ProInfo,
    #[serde(default)]
    pub runes: ProRunes,
    #[serde(default)]
    pub stat_shards: Vec<i64>,
}

impl ProMatch {
    /// The nine-id page the match used, or `None` when any part is missing so a
    /// malformed match can be skipped.
    pub fn selection(&self) -> Option<super::RuneSelection> {
        let keystone = self.runes.perk0?;
        let primary_runes = vec![self.runes.perk1?, self.runes.perk2?, self.runes.perk3?];
        let secondary_runes = vec![self.runes.perk4?, self.runes.perk5?];
        if self.stat_shards.len() != 3 {
            return None;
        }
        Some(super::RuneSelection {
            primary_page_id: self.runes.primary_style?,
            secondary_page_id: self.runes.sub_style?,
            keystone,
            primary_runes,
            secondary_runes,
            shards: self.stat_shards.clone(),
        })
    }

    /// The name to show, preferring the pro's display name.
    pub fn display_name(&self) -> &str {
        if self.pro_info.official_name.trim().is_empty() {
            &self.normalized_name
        } else {
            &self.pro_info.official_name
        }
    }
}

/// op.gg/League position to the role value the API expects. `None` means send
/// no role, which returns recent games across every role.
pub fn role_arg(position: &str) -> Option<&'static str> {
    match position.trim().to_ascii_lowercase().as_str() {
        "top" => Some("top"),
        "jungle" => Some("jungle"),
        "mid" => Some("mid"),
        "adc" => Some("adc"),
        // The API spells support `supp`.
        "support" => Some("supp"),
        _ => None,
    }
}

/// Human label for a `calculatedRole` value from the API.
pub fn role_label(role: &str) -> &str {
    match role {
        "top" => "Top",
        "jungle" => "Jungle",
        "mid" => "Mid",
        "adc" => "Bot",
        "supp" | "support" => "Support",
        "all" => "All roles",
        other => other,
    }
}

/// Cache key for one champion, role and page.
pub fn cache_key(champion_id: i64, role: Option<&str>, page: u32) -> String {
    format!("{champion_id}|{}|{page}", role.unwrap_or("all"))
}

/// Parses a GraphQL response into the matches that map to a valid rune page.
///
/// A match list that is empty is a valid answer (no recent games), not an
/// error. Individual malformed matches and entries are dropped.
pub fn parse(body: &str) -> Result<Vec<ProMatch>, RuneError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read the pro build response: {e}")))?;
    if let Some(errors) = value.get("errors").and_then(Value::as_array) {
        if !errors.is_empty() {
            return Err(RuneError::unavailable("The pro build service returned an error."));
        }
    }
    let list = value
        .pointer("/data/getProChampionMatchList/matchList")
        .and_then(Value::as_array)
        .ok_or_else(|| RuneError::unavailable("The pro build response was missing its matches."))?;
    let mut matches = Vec::with_capacity(list.len());
    for entry in list {
        let Ok(parsed) = serde_json::from_value::<ProMatch>(entry.clone()) else {
            continue;
        };
        if parsed.match_id <= 0 || parsed.selection().is_none() {
            continue;
        }
        matches.push(parsed);
    }
    Ok(matches)
}

/// A cached lookup result.
pub enum Cached {
    Fresh(Vec<ProMatch>),
    Unavailable,
    Miss,
}

/// Successes and failures for pro build lookups, kept per champion/role/page.
#[derive(Default)]
pub struct MatchCache {
    success: HashMap<String, (Instant, Vec<ProMatch>)>,
    failures: HashMap<String, Instant>,
}

impl MatchCache {
    pub fn lookup(&self, key: &str) -> Cached {
        self.lookup_at(key, Instant::now())
    }

    pub fn lookup_at(&self, key: &str, now: Instant) -> Cached {
        if let Some((at, matches)) = self.success.get(key) {
            if now.saturating_duration_since(*at) < SUCCESS_TTL {
                return Cached::Fresh(matches.clone());
            }
        }
        if let Some(at) = self.failures.get(key) {
            if now.saturating_duration_since(*at) < FAILURE_TTL {
                return Cached::Unavailable;
            }
        }
        Cached::Miss
    }

    pub fn store(&mut self, key: String, matches: Vec<ProMatch>) {
        self.store_at(key, matches, Instant::now());
    }

    pub fn store_at(&mut self, key: String, matches: Vec<ProMatch>, now: Instant) {
        self.failures.remove(&key);
        self.success.insert(key, (now, matches));
    }

    pub fn fail(&mut self, key: String) {
        self.fail_at(key, Instant::now());
    }

    pub fn fail_at(&mut self, key: String, now: Instant) {
        self.failures.insert(key, now);
    }
}

pub struct ProBuildsClient {
    client: reqwest::Client,
    endpoint: String,
}

impl ProBuildsClient {
    pub fn new() -> Result<Self, RuneError> {
        let client = reqwest::Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| {
                RuneError::unavailable(format!("Could not create the pro builds client: {e}"))
            })?;
        Ok(Self {
            client,
            endpoint: ENDPOINT.to_string(),
        })
    }

    /// The JSON body for one `ChampionMatchList` request.
    pub fn body(champion_id: i64, role: Option<&str>, page: u32, is_otp: bool) -> Value {
        let mut variables = serde_json::Map::new();
        variables.insert("championId".into(), json!(champion_id));
        if let Some(role) = role {
            variables.insert("role".into(), json!(role));
        }
        variables.insert("pageNumber".into(), json!(page.max(1)));
        variables.insert("isOtp".into(), json!(is_otp));
        json!({ "query": QUERY, "variables": variables })
    }

    pub async fn matches(
        &self,
        champion_id: i64,
        role: Option<&str>,
        page: u32,
        is_otp: bool,
    ) -> Result<Vec<ProMatch>, RuneError> {
        let response = self
            .client
            .post(&self.endpoint)
            .json(&Self::body(champion_id, role, page, is_otp))
            .send()
            .await
            .map_err(|e| RuneError::unavailable(format!("Pro builds request failed: {e}")))?;
        if !response.status().is_success() {
            return Err(RuneError::unavailable(format!(
                "Pro builds returned HTTP {}",
                response.status().as_u16()
            )));
        }
        let body = response
            .text()
            .await
            .map_err(|e| RuneError::unavailable(format!("Pro build response was unreadable: {e}")))?;
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runes::page::{validate, CatalogIndex};
    use std::collections::HashMap;

    const FIXTURE: &str = include_str!("../../tests/fixtures/probuilds-ahri-mid.json");

    /// A catalog covering the rune trees the Ahri fixture uses (Domination
    /// 8100 and Sorcery 8200) plus the standard shard rows.
    fn catalog() -> CatalogIndex {
        let mut keystones = HashMap::new();
        keystones.insert(8112, 8100);
        keystones.insert(8124, 8100);
        keystones.insert(8214, 8200);
        let mut rows = HashMap::new();
        rows.insert(
            8100,
            vec![vec![8124, 8139], vec![8140, 8137], vec![8106, 8135]],
        );
        rows.insert(8200, vec![vec![8226, 8210], vec![8237, 8233], vec![8236]]);
        CatalogIndex {
            keystones,
            rows,
            shard_rows: vec![
                vec![5008, 5005, 5007],
                vec![5008, 5010, 5001],
                vec![5011, 5013, 5001],
            ],
        }
    }

    #[test]
    fn parses_matches_from_a_saved_response() {
        let matches = parse(FIXTURE).expect("fixture should parse");
        assert_eq!(matches.len(), PAGE_SIZE);
        let first = &matches[0];
        assert_eq!(first.match_id, 7994828910);
        assert_eq!(first.calculated_role, "mid");
        assert_eq!(first.pro_info.official_name, "TTV ROSEHAN LOL");
        assert_eq!(first.display_name(), "TTV ROSEHAN LOL");
        assert!(first.match_timestamp > 0);
        assert_eq!(first.runes.perk0, Some(8112));
    }

    #[test]
    fn maps_a_pro_match_to_a_selection_that_passes_validation() {
        let matches = parse(FIXTURE).unwrap();
        let selection = matches[0].selection().expect("fixture has a full page");
        assert_eq!(selection.primary_page_id, 8100);
        assert_eq!(selection.secondary_page_id, 8200);
        assert_eq!(selection.keystone, 8112);
        assert_eq!(selection.primary_runes, vec![8139, 8140, 8106]);
        assert_eq!(selection.secondary_runes, vec![8237, 8226]);
        assert_eq!(selection.shards, vec![5005, 5008, 5011]);
        validate(&selection, &catalog()).expect("a real pro page should validate");
    }

    #[test]
    fn keeps_the_league_order_of_the_nine_ids() {
        let selection = parse(FIXTURE).unwrap()[0].selection().unwrap();
        assert_eq!(
            selection.perk_ids(),
            vec![8112, 8139, 8140, 8106, 8237, 8226, 5005, 5008, 5011]
        );
    }

    #[test]
    fn skips_malformed_entries_without_failing_the_list() {
        let body = r#"{"data":{"getProChampionMatchList":{"matchList":[
            {"matchId":1,"runes":{"perk0":8112,"perk1":8139,"perk2":8137,"perk3":8106,"perk4":8444,"perk5":8242,"primaryStyle":8100,"subStyle":8400},"statShards":[5005,5008,5011]},
            {"matchId":0,"runes":{}},
            {"matchId":2,"runes":{"perk0":8112},"statShards":[]},
            {"matchId":3,"statShards":[]},
            {"matchId":4,"runes":{"perk0":8112,"perk1":8139,"perk2":8137,"perk3":8106,"perk4":8444,"perk5":8242,"primaryStyle":8100,"subStyle":8400},"statShards":[5005,5008]}
        ]}}}"#;
        let matches = parse(body).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].match_id, 1);
    }

    #[test]
    fn an_empty_match_list_is_a_valid_answer() {
        let body = r#"{"data":{"getProChampionMatchList":{"matchList":[]}}}"#;
        assert!(parse(body).unwrap().is_empty());
    }

    #[test]
    fn rejects_graphql_errors_and_missing_data() {
        assert!(parse(r#"{"errors":[{"message":"nope"}]}"#).is_err());
        assert!(parse(r#"{"data":{}}"#).is_err());
        assert!(parse("not json").is_err());
    }

    #[test]
    fn maps_positions_to_api_role_values() {
        assert_eq!(role_arg("top"), Some("top"));
        assert_eq!(role_arg("JUNGLE"), Some("jungle"));
        assert_eq!(role_arg("mid"), Some("mid"));
        assert_eq!(role_arg("adc"), Some("adc"));
        assert_eq!(role_arg("support"), Some("supp"));
        assert_eq!(role_arg("none"), None);
        assert_eq!(role_arg(""), None);
    }

    #[test]
    fn builds_a_graphql_body_with_only_the_needed_variables() {
        let body = ProBuildsClient::body(103, Some("mid"), 2, false);
        assert_eq!(body["variables"]["championId"], 103);
        assert_eq!(body["variables"]["role"], "mid");
        assert_eq!(body["variables"]["pageNumber"], 2);
        assert_eq!(body["variables"]["isOtp"], false);
        // No role means the API returns recent games across every role.
        let body = ProBuildsClient::body(103, None, 0, false);
        assert!(body["variables"].get("role").is_none());
        // Pages are clamped to the API's 1-based numbering.
        assert_eq!(body["variables"]["pageNumber"], 1);
    }

    #[test]
    fn caches_successes_for_ten_minutes_and_failures_for_one() {
        let now = Instant::now();
        let mut cache = MatchCache::default();
        let key = cache_key(103, Some("mid"), 1);
        assert!(matches!(cache.lookup_at(&key, now), Cached::Miss));

        cache.store_at(key.clone(), vec![ProMatch::default()], now);
        assert!(matches!(cache.lookup_at(&key, now), Cached::Fresh(_)));
        // Still fresh just under the TTL, stale after it.
        assert!(matches!(cache.lookup_at(&key, now + SUCCESS_TTL / 2), Cached::Fresh(_)));
        assert!(matches!(
            cache.lookup_at(&key, now + SUCCESS_TTL + Duration::from_secs(1)),
            Cached::Miss
        ));

        // A failed key is negative-cached for a minute.
        let failed = cache_key(103, Some("top"), 1);
        cache.fail_at(failed.clone(), now);
        assert!(matches!(cache.lookup_at(&failed, now + Duration::from_secs(30)), Cached::Unavailable));
        assert!(matches!(
            cache.lookup_at(&failed, now + FAILURE_TTL + Duration::from_secs(1)),
            Cached::Miss
        ));
    }

    #[test]
    fn a_new_success_clears_a_previous_failure() {
        let now = Instant::now();
        let mut cache = MatchCache::default();
        let key = cache_key(103, None, 1);
        cache.fail_at(key.clone(), now);
        cache.store_at(key.clone(), vec![ProMatch::default()], now);
        assert!(matches!(cache.lookup_at(&key, now), Cached::Fresh(_)));
    }

    #[test]
    fn distinguishes_role_and_page_in_the_cache_key() {
        assert_ne!(cache_key(103, Some("mid"), 1), cache_key(103, Some("mid"), 2));
        assert_ne!(cache_key(103, Some("mid"), 1), cache_key(103, Some("top"), 1));
        assert_eq!(cache_key(103, None, 1), "103|all|1");
    }
}
