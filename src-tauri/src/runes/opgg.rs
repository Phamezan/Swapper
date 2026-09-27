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

/// Default rank bracket. op.gg answers with Emerald+ data when no `tier` is
/// sent, so sending this explicitly keeps the default behaviour.
pub const DEFAULT_TIER: &str = "emerald_plus";
/// op.gg's broadest bracket, used as the fallback suggestion when a narrower
/// bracket has too few games.
pub const TIER_ALL: &str = "all";

/// The rank brackets op.gg accepts on `.../champions/{mode}/{id}/{pos}?tier=`.
/// `grandmaster_plus` and `iron_plus` are rejected by the API, so they are not
/// offered.
pub const TIERS: &[(&str, &str)] = &[
    (TIER_ALL, "All ranks"),
    ("gold_plus", "Gold+"),
    ("platinum_plus", "Platinum+"),
    (DEFAULT_TIER, "Emerald+"),
    ("diamond_plus", "Diamond+"),
    ("master_plus", "Master+"),
    ("challenger", "Challenger"),
];

/// The canonical slug for a user- or settings-supplied tier name.
pub fn tier_slug(value: &str) -> Option<&'static str> {
    let value = value.trim().to_ascii_lowercase();
    TIERS
        .iter()
        .find(|(slug, _)| *slug == value)
        .map(|(slug, _)| *slug)
}

/// The human label shown for a tier slug.
pub fn tier_label(value: &str) -> Option<&'static str> {
    let slug = tier_slug(value)?;
    TIERS
        .iter()
        .find(|(candidate, _)| *candidate == slug)
        .map(|(_, label)| *label)
}

/// A known tier slug, or the default when the value is blank or unknown.
pub fn normalize_tier(value: &str) -> &'static str {
    tier_slug(value).unwrap_or(DEFAULT_TIER)
}

/// Whether op.gg honours the `tier` parameter for a mode. Ranked and ARAM do;
/// Arena does not (the arena endpoint rejects the request outright).
pub fn tier_supported(mode: &str) -> bool {
    mode != MODE_ARENA
}

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
    #[serde(default)]
    summoner_spells: Vec<SpellStats>,
    #[serde(default)]
    starter_items: Vec<ItemStats>,
    #[serde(default)]
    boots: Vec<ItemStats>,
    #[serde(default)]
    core_items: Vec<ItemStats>,
    #[serde(default)]
    last_items: Vec<ItemStats>,
}

/// One recommended summoner-spell pair, with its population.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpellStats {
    #[serde(default)]
    pub ids: Vec<i64>,
    #[serde(default)]
    pub play: u64,
    #[serde(default)]
    pub win: u64,
    #[serde(default)]
    pub pick_rate: f64,
}

/// One recommended item set (a starter, a pair of boots, a core, a single late
/// item), with its population.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ItemStats {
    #[serde(default)]
    pub ids: Vec<i64>,
    #[serde(default)]
    pub play: u64,
    #[serde(default)]
    pub win: u64,
    #[serde(default)]
    pub pick_rate: f64,
}

impl ItemStats {
    pub fn win_pct(&self) -> Option<f64> {
        (self.play > 0).then(|| (self.win as f64 / self.play as f64) * 100.0)
    }
}

/// The item build groups behind the Presets tab: starters, boots, cores, and
/// late items.
#[derive(Debug, Clone, Default)]
pub struct ItemGroups {
    pub starter_items: Vec<ItemStats>,
    pub boots: Vec<ItemStats>,
    pub core_items: Vec<ItemStats>,
    pub last_items: Vec<ItemStats>,
}

/// The parts of an op.gg champion response Swapper reads.
#[derive(Debug, Clone, Default)]
pub struct ChampionData {
    pub rune_pages: Vec<RunePageGroup>,
    pub summoner_spells: Vec<SpellStats>,
    pub items: ItemGroups,
}

impl ChampionData {
    /// The most-played spell pair, or `None` when op.gg reported none. op.gg
    /// already ranks the list by popularity, but the maximum is taken so the
    /// order is not assumed.
    pub fn top_spell_pair(&self) -> Option<[i64; 2]> {
        self.summoner_spells
            .iter()
            .filter(|entry| entry.ids.len() >= 2)
            .max_by_key(|entry| entry.play)
            .map(|entry| [entry.ids[0], entry.ids[1]])
    }
}

/// Parses an op.gg champion response into the rune pages and spell pairs.
pub fn parse_data(body: &str) -> Result<ChampionData, RuneError> {
    let response: Response = serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read the op.gg response: {e}")))?;
    Ok(ChampionData {
        rune_pages: response.data.rune_pages,
        summoner_spells: response.data.summoner_spells,
        items: ItemGroups {
            starter_items: response.data.starter_items,
            boots: response.data.boots,
            core_items: response.data.core_items,
            last_items: response.data.last_items,
        },
    })
}

/// Parses an op.gg champion response and returns its rune-page groups.
#[cfg(test)]
pub fn parse(body: &str) -> Result<Vec<RunePageGroup>, RuneError> {
    parse_data(body).map(|data| data.rune_pages)
}

/// Builds the op.gg champion path for the given region, mode, champion,
/// position and rank bracket.
pub fn path(region: &str, mode: &str, champion_id: i64, position: &str, tier: &str) -> String {
    format!(
        "/api/{region}/champions/{mode}/{champion_id}/{position}?tier={}",
        normalize_tier(tier)
    )
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

/// How many alternative cores the "more" toggle may reveal.
pub const MAX_CORE_ALTERNATIVES: usize = 2;
/// How many popular late items to show.
pub const MAX_LATE_ITEMS: usize = 4;

/// The compact item build shown on the Presets tab: only the top option for
/// each group, plus a couple of core alternatives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemBuild {
    pub starter: Option<ItemStats>,
    pub boots: Option<ItemStats>,
    pub core: Option<ItemStats>,
    /// The next most popular cores, so the UI can offer a small "more" toggle.
    pub core_alternatives: Vec<ItemStats>,
    /// The most popular single late items, at most [`MAX_LATE_ITEMS`].
    pub late: Vec<ItemStats>,
}

impl ItemBuild {
    pub fn is_empty(&self) -> bool {
        self.starter.is_none()
            && self.boots.is_none()
            && self.core.is_none()
            && self.late.is_empty()
    }
}

/// The popular options of a group, most-played first. op.gg already orders by
/// popularity, but the sort keeps the choice independent of that.
fn by_popularity(list: &[ItemStats]) -> Vec<ItemStats> {
    let mut ranked: Vec<ItemStats> = list
        .iter()
        .filter(|entry| !entry.ids.is_empty() && entry.play > 0)
        .cloned()
        .collect();
    ranked.sort_by(|a, b| b.play.cmp(&a.play));
    ranked
}

/// Picks the compact item build from op.gg's item groups: the top starter,
/// boots and core, the next cores as alternatives, and a few late items.
pub fn item_build(items: &ItemGroups) -> ItemBuild {
    let starters = by_popularity(&items.starter_items);
    let boots = by_popularity(&items.boots);
    let cores = by_popularity(&items.core_items);
    let late = by_popularity(&items.last_items);
    ItemBuild {
        starter: starters.into_iter().next(),
        boots: boots.into_iter().next(),
        core: cores.first().cloned(),
        core_alternatives: cores.into_iter().skip(1).take(MAX_CORE_ALTERNATIVES).collect(),
        late: late.into_iter().take(MAX_LATE_ITEMS).collect(),
    }
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

    pub fn url(&self, region: &str, mode: &str, champion_id: i64, position: &str, tier: &str) -> String {
        format!(
            "{}{}",
            self.base_url,
            path(region, mode, champion_id, position, tier)
        )
    }

    pub async fn champion_data(
        &self,
        region: &str,
        mode: &str,
        champion_id: i64,
        position: &str,
        tier: &str,
    ) -> Result<ChampionData, RuneError> {
        let response = self
            .client
            .get(self.url(region, mode, champion_id, position, tier))
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
        parse_data(&body)
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
            path("euw", MODE_RANKED, 103, "mid", DEFAULT_TIER),
            "/api/euw/champions/ranked/103/mid?tier=emerald_plus"
        );
        assert_eq!(
            path("na", MODE_ARAM, 103, POSITION_NONE, TIER_ALL),
            "/api/na/champions/aram/103/none?tier=all"
        );
    }

    #[test]
    fn maps_known_tiers_and_normalizes_unknown_ones() {
        assert_eq!(tier_slug("emerald_plus"), Some("emerald_plus"));
        assert_eq!(tier_slug("  ALL "), Some("all"));
        assert_eq!(tier_slug("iron_plus"), None);
        assert_eq!(tier_slug(""), None);
        // An unknown or blank value falls back to Emerald+.
        assert_eq!(normalize_tier("grandmaster_plus"), DEFAULT_TIER);
        assert_eq!(normalize_tier(""), DEFAULT_TIER);
        assert_eq!(normalize_tier("diamond_plus"), "diamond_plus");
        assert_eq!(tier_label("all"), Some("All ranks"));
        assert_eq!(tier_label("nonsense"), None);
    }

    #[test]
    fn the_path_normalizes_an_unknown_tier_to_the_default() {
        assert_eq!(
            path("euw", MODE_RANKED, 103, "mid", "iron_plus"),
            "/api/euw/champions/ranked/103/mid?tier=emerald_plus"
        );
    }

    #[test]
    fn parses_item_groups_and_picks_the_top_option_per_group() {
        let data = parse_data(FIXTURE).unwrap();
        assert_eq!(data.items.starter_items.len(), 2);
        assert_eq!(data.items.boots.len(), 2);
        assert_eq!(data.items.core_items.len(), 4);
        assert_eq!(data.items.last_items.len(), 5);
        let build = item_build(&data.items);
        assert_eq!(build.starter.as_ref().unwrap().ids, vec![1056, 2003, 2003]);
        assert_eq!(build.boots.as_ref().unwrap().ids, vec![3020]);
        assert_eq!(build.core.as_ref().unwrap().ids, vec![6653, 4645, 3157]);
        // The next two cores become alternatives; the fourth is dropped.
        let alternatives: Vec<Vec<i64>> = build
            .core_alternatives
            .iter()
            .map(|core| core.ids.clone())
            .collect();
        assert_eq!(
            alternatives,
            vec![vec![6653, 3157, 3089], vec![6653, 4645, 3135]]
        );
        // The five late items are trimmed to four, most played first.
        let late: Vec<Vec<i64>> = build.late.iter().map(|item| item.ids.clone()).collect();
        assert_eq!(late, vec![vec![3089], vec![3135], vec![3116], vec![3157]]);
        let win = build.core.as_ref().unwrap().win_pct().unwrap();
        assert!((win - 52.5).abs() < 0.1, "win pct was {win}");
        assert!(!build.is_empty());
    }

    #[test]
    fn an_empty_or_zero_play_item_group_yields_no_build() {
        let data = parse_data(r#"{"data":{"rune_pages":[]}}"#).unwrap();
        assert!(item_build(&data.items).is_empty());
        let data =
            parse_data(r#"{"data":{"core_items":[{"ids":[6653],"play":0,"win":0}]}}"#).unwrap();
        assert!(item_build(&data.items).is_empty());
    }

    #[test]
    fn picks_the_most_played_spell_pair() {
        let body = r#"{"data":{"rune_pages":[],"summoner_spells":[
            {"ids":[4,12],"play":100,"win":50,"pick_rate":0.1},
            {"ids":[4,14],"play":900,"win":500,"pick_rate":0.5},
            {"ids":[4],"play":5000,"win":2500,"pick_rate":0.3}
        ]}}"#;
        let data = parse_data(body).unwrap();
        // A truncated pair (one id) is ignored even when it is more popular.
        assert_eq!(data.top_spell_pair(), Some([4, 14]));
        assert!(parse_data(r#"{"data":{"rune_pages":[]}}"#)
            .unwrap()
            .top_spell_pair()
            .is_none());
    }

    #[test]
    fn the_tier_parameter_is_honoured_for_ranked_and_aram_only() {
        assert!(tier_supported(MODE_RANKED));
        assert!(tier_supported(MODE_ARAM));
        assert!(!tier_supported(MODE_ARENA));
    }
}
