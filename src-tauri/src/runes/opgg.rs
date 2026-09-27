//! op.gg champion API client.
//!
//! The HTTP shape and the endpoint layout are ported from LeagueAkari (MIT,
//! https://github.com/LeagueAkari/LeagueAkari) — see
//! `src/shared/http-api-axios-helper/opgg/index.ts` and
//! `src/shared/data-adapter/champion-data/opgg.ts`. Swapper only reads the
//! rune-page part of the response.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::RuneError;

/// op.gg region slug used when the League client region is unknown.
pub const DEFAULT_REGION: &str = "euw";
/// op.gg mode slug for regular queues.
pub const MODE_RANKED: &str = "ranked";
/// op.gg mode slug for ARAM.
pub const MODE_ARAM: &str = "aram";
/// op.gg mode slug for Arena.
pub const MODE_ARENA: &str = "arena";
/// op.gg uses this position for modes that do not have one.
pub const POSITION_NONE: &str = "none";

const BASE_URL: &str = "https://lol-api-champion.op.gg";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RunePageGroup {
    #[serde(default)]
    pub id: i64,
    pub primary_page_id: i64,
    pub secondary_page_id: i64,
    #[serde(default)]
    pub play: u64,
    #[serde(default)]
    pub win: u64,
    #[serde(default)]
    pub pick_rate: f64,
    #[serde(default)]
    pub builds: Vec<RuneBuild>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuneBuild {
    #[serde(default)]
    pub id: i64,
    pub primary_page_id: i64,
    pub primary_rune_ids: Vec<i64>,
    pub secondary_page_id: i64,
    pub secondary_rune_ids: Vec<i64>,
    pub stat_mod_ids: Vec<i64>,
    #[serde(default)]
    pub play: u64,
    #[serde(default)]
    pub win: u64,
    #[serde(default)]
    pub pick_rate: f64,
}

impl RuneBuild {
    pub fn win_pct(&self) -> Option<f64> {
        (self.play > 0).then(|| (self.win as f64 / self.play as f64) * 100.0)
    }
}

#[derive(Deserialize)]
struct Response {
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    #[serde(default)]
    rune_pages: Vec<RunePageGroup>,
}

/// Parses an op.gg champion response and returns its rune-page groups.
pub fn parse(body: &str) -> Result<Vec<RunePageGroup>, RuneError> {
    let response: Response = serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read the op.gg response: {e}")))?;
    Ok(response.data.rune_pages)
}

/// Builds the op.gg champion path for the given region, mode, champion and position.
pub fn path(region: &str, mode: &str, champion_id: i64, position: &str) -> String {
    format!("/api/{region}/champions/{mode}/{champion_id}/{position}")
}

/// One preset: the most-played build of an op.gg rune-page group.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub index: usize,
    pub primary_page_id: i64,
    pub secondary_page_id: i64,
    pub keystone: i64,
    pub primary_runes: Vec<i64>,
    pub secondary_runes: Vec<i64>,
    pub shards: Vec<i64>,
    pub play: u64,
    pub win_pct: Option<f64>,
    pub pick_rate: Option<f64>,
}

impl Preset {
    fn from_build(index: usize, build: &RuneBuild) -> Option<Self> {
        let keystone = *build.primary_rune_ids.first()?;
        Some(Self {
            index,
            primary_page_id: build.primary_page_id,
            secondary_page_id: build.secondary_page_id,
            keystone,
            primary_runes: build.primary_rune_ids.iter().skip(1).copied().collect(),
            secondary_runes: build.secondary_rune_ids.clone(),
            shards: build.stat_mod_ids.clone(),
            play: build.play,
            win_pct: build.win_pct(),
            pick_rate: (build.pick_rate > 0.0).then_some(build.pick_rate * 100.0),
        })
    }
}

/// The top build of each rune-page group, most popular group first.
pub fn presets(groups: &[RunePageGroup]) -> Vec<Preset> {
    let mut ranked: Vec<&RunePageGroup> = groups.iter().filter(|g| !g.builds.is_empty()).collect();
    ranked.sort_by(|a, b| b.play.cmp(&a.play));
    ranked
        .into_iter()
        .enumerate()
        .filter_map(|(index, group)| {
            let build = group
                .builds
                .iter()
                .max_by(|a, b| {
                    a.play
                        .cmp(&b.play)
                        .then_with(|| a.win.cmp(&b.win))
                })?;
            Preset::from_build(index, build)
        })
        .collect()
}

pub struct OpggClient {
    client: reqwest::Client,
    base_url: String,
}

impl OpggClient {
    pub fn new() -> Result<Self, RuneError> {
        let client = reqwest::Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| RuneError::unavailable(format!("Could not create the op.gg client: {e}")))?;
        Ok(Self {
            client,
            base_url: BASE_URL.to_string(),
        })
    }

    pub fn url(&self, region: &str, mode: &str, champion_id: i64, position: &str) -> String {
        format!("{}{}", self.base_url, path(region, mode, champion_id, position))
    }

    pub async fn rune_pages(
        &self,
        region: &str,
        mode: &str,
        champion_id: i64,
        position: &str,
    ) -> Result<Vec<RunePageGroup>, RuneError> {
        let response = self
            .client
            .get(self.url(region, mode, champion_id, position))
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| RuneError::unavailable(format!("op.gg request failed: {e}")))?;
        if !response.status().is_success() {
            return Err(RuneError::unavailable(format!(
                "op.gg returned HTTP {}",
                response.status().as_u16()
            )));
        }
        let body = response
            .text()
            .await
            .map_err(|e| RuneError::unavailable(format!("op.gg response was unreadable: {e}")))?;
        parse(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/opgg-ahri-mid-ranked.json");

    #[test]
    fn parses_rune_pages_from_a_saved_response() {
        let groups = parse(FIXTURE).expect("fixture should parse");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].primary_page_id, 8100);
        assert_eq!(groups[0].secondary_page_id, 8200);
        assert_eq!(groups[0].builds.len(), 3);
        assert_eq!(groups[0].builds[0].stat_mod_ids, vec![5005, 5008, 5001]);
    }

    #[test]
    fn rejects_a_response_without_rune_pages() {
        assert!(parse("{}").is_err());
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"data":{}}"#).unwrap().is_empty());
    }

    #[test]
    fn chooses_the_top_build_per_group_in_popularity_order() {
        let groups = parse(FIXTURE).unwrap();
        let presets = presets(&groups);
        assert_eq!(presets.len(), 3);
        // Most popular group first.
        assert_eq!(presets[0].primary_page_id, 8100);
        assert_eq!(presets[1].primary_page_id, 8200);
        // Top build of the first group is the 8058-play build.
        assert_eq!(presets[0].keystone, 8112);
        assert_eq!(presets[0].play, 8058);
        assert_eq!(presets[0].primary_runes, vec![8139, 8140, 8106]);
        assert_eq!(presets[0].secondary_runes, vec![8210, 8226]);
        assert_eq!(presets[0].shards, vec![5005, 5008, 5001]);
        let win = presets[0].win_pct.unwrap();
        assert!((win - 50.67).abs() < 0.1, "win pct was {win}");
        assert!(presets[0].pick_rate.unwrap() > 60.0);
        // Second preset uses the Sorcery keystone.
        assert_eq!(presets[1].keystone, 8992);
        assert_eq!(presets[2].keystone, 8214);
    }

    #[test]
    fn builds_the_endpoint_path_with_the_league_position() {
        assert_eq!(
            path("euw", MODE_RANKED, 103, "mid"),
            "/api/euw/champions/ranked/103/mid"
        );
        assert_eq!(
            path("na", MODE_ARAM, 103, POSITION_NONE),
            "/api/na/champions/aram/103/none"
        );
    }
}
