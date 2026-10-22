//! op.gg champion API client.
//!
//! The HTTP shape and the endpoint layout are ported from LeagueAkari (MIT,
//! https://github.com/LeagueAkari/LeagueAkari) — see
//! `src/shared/http-api-axios-helper/opgg/index.ts` and
//! `src/shared/data-adapter/champion-data/opgg.ts`. Swapper only reads the
//! rune-page part of the response.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::patch::{blend_choices, PatchInfo, Rate};
use super::provider::ProviderError;

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

pub(crate) const BASE_URL: &str = "https://lol-api-champion.op.gg";
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
    /// Patch-blended weight and win rate; `None` on a plain current-patch group.
    #[serde(default)]
    pub blend: Option<Rate>,
}

impl RunePageGroup {
    /// What orders the group: its games, or the patch-blended weight.
    pub fn weight(&self) -> f64 {
        self.blend.map_or(self.play as f64, |rate| rate.weight)
    }
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
    /// Patch-blended weight and win rate; `None` on a plain current-patch build.
    #[serde(default)]
    pub blend: Option<Rate>,
}

/// The runes and shards that identify one build.
type BuildKey = (i64, i64, Vec<i64>, Vec<i64>, Vec<i64>);

impl RuneBuild {
    /// The current patch's own win rate.
    fn raw_win_pct(&self) -> Option<f64> {
        (self.play > 0).then(|| (self.win as f64 / self.play as f64) * 100.0)
    }

    /// The win rate, blended with the previous patch's when there is one.
    pub fn win_pct(&self) -> Option<f64> {
        self.blend.map(|rate| rate.win_pct).or_else(|| self.raw_win_pct())
    }

    /// What orders the build: its games, or the patch-blended weight.
    pub fn weight(&self) -> f64 {
        self.blend.map_or(self.play as f64, |rate| rate.weight)
    }

    fn key(&self) -> BuildKey {
        (
            self.primary_page_id,
            self.secondary_page_id,
            self.primary_rune_ids.clone(),
            self.secondary_rune_ids.clone(),
            self.stat_mod_ids.clone(),
        )
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
    /// Patch-blended weight and win rate; `None` on a plain current-patch pair.
    #[serde(default)]
    pub blend: Option<Rate>,
}

impl SpellStats {
    fn weight(&self) -> f64 {
        self.blend.map_or(self.play as f64, |rate| rate.weight)
    }
}

/// The parts of an op.gg champion response Swapper reads.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChampionData {
    pub rune_pages: Vec<RunePageGroup>,
    pub summoner_spells: Vec<SpellStats>,
    /// Which patch this is, and whether the previous one was blended in.
    #[serde(default)]
    pub patch: Option<PatchInfo>,
}

/// A previous-patch-only option is carried over only with at least this share
/// (percent) of the table's weight, so a stale rare page cannot crowd the list.
const MIN_PRIOR_SHARE_PCT: f64 = 2.0;

/// Whether a previous-patch-only option has enough of `rates`' total weight to
/// be carried over.
fn keeps<K>(rates: &[(K, Rate)]) -> impl Fn(Option<&Rate>) -> bool {
    let total: f64 = rates.iter().map(|(_, rate)| rate.weight).sum();
    move |rate| rate.is_some_and(|rate| rate.weight * 100.0 >= total * MIN_PRIOR_SHARE_PCT)
}

/// Blends the previous patch into op.gg's data: rune builds, page groups and
/// spell pairs get a weight and win rate from [`blend_choices`]. Their `play`
/// stays the current patch's games; only ordering and win rates change.
pub fn blend_champion_data(
    mut now: ChampionData,
    prev: Option<&ChampionData>,
    info: PatchInfo,
) -> ChampionData {
    now.patch = Some(info);
    let Some(prev) = prev else {
        return now;
    };
    let rate_of = |play: u64, win: u64| (play > 0).then(|| win as f64 / play as f64 * 100.0);
    let builds = |data: &ChampionData| -> Vec<(BuildKey, u64, Option<f64>)> {
        data.rune_pages
            .iter()
            .flat_map(|group| &group.builds)
            .map(|build| (build.key(), build.play, build.raw_win_pct()))
            .collect()
    };
    let group_key = |group: &RunePageGroup| (group.primary_page_id, group.secondary_page_id);
    let groups = |data: &ChampionData| -> Vec<((i64, i64), u64, Option<f64>)> {
        data.rune_pages
            .iter()
            .map(|group| (group_key(group), group.play, rate_of(group.play, group.win)))
            .collect()
    };
    let spells = |data: &ChampionData| -> Vec<(Vec<i64>, u64, Option<f64>)> {
        data.summoner_spells
            .iter()
            .map(|spell| (spell.ids.clone(), spell.play, rate_of(spell.play, spell.win)))
            .collect()
    };

    let build_rates = blend_choices(&builds(&now), &builds(prev));
    let keep_build = keeps(&build_rates);
    let build_rates: HashMap<BuildKey, Rate> = build_rates.into_iter().collect();
    let group_rates = blend_choices(&groups(&now), &groups(prev));
    let keep_group = keeps(&group_rates);
    let group_rates: HashMap<(i64, i64), Rate> = group_rates.into_iter().collect();
    let carried = |build: &RuneBuild| RuneBuild {
        play: 0,
        win: 0,
        pick_rate: 0.0,
        blend: build_rates.get(&build.key()).copied(),
        ..build.clone()
    };

    for group in &mut now.rune_pages {
        group.blend = group_rates.get(&group_key(group)).copied();
        for build in &mut group.builds {
            build.blend = build_rates.get(&build.key()).copied();
        }
    }
    for old in &prev.rune_pages {
        let missing: Vec<RuneBuild> = old
            .builds
            .iter()
            .filter(|build| keep_build(build_rates.get(&build.key())))
            .filter(|build| {
                !now.rune_pages
                    .iter()
                    .any(|group| group.builds.iter().any(|other| other.key() == build.key()))
            })
            .map(carried)
            .collect();
        match now.rune_pages.iter_mut().find(|group| group_key(group) == group_key(old)) {
            Some(group) => group.builds.extend(missing),
            None if keep_group(group_rates.get(&group_key(old))) && !missing.is_empty() => {
                now.rune_pages.push(RunePageGroup {
                    play: 0,
                    win: 0,
                    pick_rate: 0.0,
                    blend: group_rates.get(&group_key(old)).copied(),
                    builds: missing,
                    ..old.clone()
                });
            }
            None => {}
        }
    }

    let spell_rates = blend_choices(&spells(&now), &spells(prev));
    let keep_spell = keeps(&spell_rates);
    let spell_rates: HashMap<Vec<i64>, Rate> = spell_rates.into_iter().collect();
    for spell in &mut now.summoner_spells {
        spell.blend = spell_rates.get(&spell.ids).copied();
    }
    let carried: Vec<SpellStats> = prev
        .summoner_spells
        .iter()
        .filter(|old| !now.summoner_spells.iter().any(|spell| spell.ids == old.ids))
        .filter(|old| keep_spell(spell_rates.get(&old.ids)))
        .map(|old| SpellStats {
            play: 0,
            win: 0,
            pick_rate: 0.0,
            blend: spell_rates.get(&old.ids).copied(),
            ..old.clone()
        })
        .collect();
    now.summoner_spells.extend(carried);
    now
}

impl ChampionData {
    /// The most-played spell pair, or `None` when op.gg reported none. op.gg
    /// already ranks the list by popularity, but the maximum is taken so the
    /// order is not assumed.
    pub fn top_spell_pair(&self) -> Option<[i64; 2]> {
        self.summoner_spells
            .iter()
            .filter(|entry| entry.ids.len() >= 2)
            .max_by(|left, right| left.weight().total_cmp(&right.weight()))
            .map(|entry| [entry.ids[0], entry.ids[1]])
    }
}

/// Parses an op.gg champion response into the rune pages and spell pairs.
pub fn parse_data(body: &str) -> Result<ChampionData, ProviderError> {
    let response: Response = serde_json::from_str(body).map_err(|_| ProviderError::Format)?;
    Ok(ChampionData {
        rune_pages: response.data.rune_pages,
        summoner_spells: response.data.summoner_spells,
        patch: None,
    })
}

/// Parses an op.gg champion response and returns its rune-page groups.
#[cfg(test)]
pub fn parse(body: &str) -> Result<Vec<RunePageGroup>, ProviderError> {
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
    ranked.sort_by(|a, b| b.weight().total_cmp(&a.weight()));
    ranked
        .into_iter()
        .enumerate()
        .filter_map(|(index, group)| {
            let build = group
                .builds
                .iter()
                .max_by(|a, b| {
                    a.weight()
                        .total_cmp(&b.weight())
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
    pub fn new() -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| ProviderError::Unavailable)?;
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

    /// [`Self::fetch`] with its outcome recorded for Swapper Doctor.
    pub async fn champion_data(
        &self,
        region: &str,
        mode: &str,
        champion_id: i64,
        position: &str,
        tier: &str,
        version: Option<&str>,
    ) -> Result<ChampionData, ProviderError> {
        let found = self
            .fetch(region, mode, champion_id, position, tier, version)
            .await;
        super::health::record(super::health::Source::Opgg, "champion data", &found);
        found
    }

    async fn fetch(
        &self,
        region: &str,
        mode: &str,
        champion_id: i64,
        position: &str,
        tier: &str,
        version: Option<&str>,
    ) -> Result<ChampionData, ProviderError> {
        let mut url = self.url(region, mode, champion_id, position, tier);
        if let Some(version) = version {
            url.push_str(&format!("&version={version}"));
        }
        let response = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| ProviderError::from_reqwest(&e))?;
        if !response.status().is_success() {
            return Err(ProviderError::from_status(response.status()));
        }
        let body = response
            .text()
            .await
            .map_err(|_| ProviderError::Unavailable)?;
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
    fn a_malformed_body_is_classified_as_a_format_change() {
        assert_eq!(parse("not json").unwrap_err(), ProviderError::Format);
        assert_eq!(parse("{}").unwrap_err(), ProviderError::Format);
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

    fn info() -> PatchInfo {
        PatchInfo { patch: "16.20".into(), previous: Some("16.19".into()) }
    }

    /// The fixture with every play and win scaled to `plays` per build.
    fn with_plays(data: &ChampionData, plays: u64) -> ChampionData {
        let mut data = data.clone();
        for group in &mut data.rune_pages {
            group.play = plays;
            group.win = plays / 2;
            for build in &mut group.builds {
                build.win = (build.win_pct().unwrap_or(50.0) / 100.0 * plays as f64) as u64;
                build.play = plays;
            }
        }
        data
    }

    #[test]
    fn day_zero_rune_pages_follow_the_previous_patch_order_and_rates() {
        let prev = parse_data(FIXTURE).unwrap();
        let now = with_plays(&prev, 0);
        let blended = blend_champion_data(now, Some(&prev), info());
        assert_eq!(presets(&blended.rune_pages)[0].keystone, presets(&prev.rune_pages)[0].keystone);
        let top = &presets(&blended.rune_pages)[0];
        assert_eq!(top.play, 0, "plays shown are the current patch's");
        let expected = presets(&prev.rune_pages)[0].win_pct.unwrap();
        assert!((top.win_pct.unwrap() - expected).abs() < 1e-9);
    }

    #[test]
    fn a_current_only_rune_page_outranks_the_previous_patchs_top_page() {
        let prev = parse_data(FIXTURE).unwrap();
        let mut now = with_plays(&prev, 50);
        // The current patch has switched keystone: a build only it has, 20,000 plays.
        let mut fresh = now.rune_pages[0].builds[0].clone();
        fresh.primary_rune_ids = vec![8010, 9111, 9104, 8299];
        fresh.play = 20_000;
        fresh.win = 10_400;
        now.rune_pages[0].builds.push(fresh);
        now.rune_pages[0].play += 20_000;
        let blended = blend_champion_data(now, Some(&prev), info());
        assert_eq!(presets(&blended.rune_pages)[0].keystone, 8010);
    }

    #[test]
    fn the_rune_statistics_follow_the_blended_weights() {
        let prev = parse_data(FIXTURE).unwrap();
        let blended = blend_champion_data(with_plays(&prev, 0), Some(&prev), info());
        let stats = crate::runes::stats::aggregate(&blended.rune_pages);
        let plain = crate::runes::stats::aggregate(&prev.rune_pages);
        let blended_pct = stats.get(8112).unwrap().win_pct().unwrap();
        assert!(blended_pct > 0.0 && blended_pct < 100.0);
        assert_eq!(stats.get(8112).unwrap().play, 0);
        assert!(plain.get(8112).unwrap().play > 0);
    }

    #[test]
    fn a_failed_or_missing_previous_patch_leaves_the_current_data_alone() {
        let now = parse_data(FIXTURE).unwrap();
        let blended = blend_champion_data(now.clone(), None, PatchInfo { previous: None, ..info() });
        assert_eq!(presets(&blended.rune_pages), presets(&now.rune_pages));
        assert!(blended.patch.unwrap().previous.is_none());
    }
}
