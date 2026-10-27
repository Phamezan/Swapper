//! A champion's overview: u.gg win/pick/ban rates and games, with lolalytics
//! supplying damage and the most common build, and all stats when u.gg fails.
//!
//! The counters page carries none of the rank or tier fields, so the plain
//! build page (no keystone filter) is read instead. It is the same Qwik state
//! the build and matchup parsers already handle, so the build, runes, spells and
//! skill order come from the existing code paths.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::cache::{self, LastGoodCache};
use super::data::{keystone_fetches, keystone_gate};
use super::lolalytics::{self, as_i64, as_u64, number, resolve, KeystoneBuild, LolalyticsClient, Qwik};
use super::matchup::{pick_details, MatchupRunes};
use super::patch::{self, blend_rate, PatchInfo};
use super::provider::{DataKind, Provider, ProviderError, Sourced};
use super::RuneError;

use std::sync::OnceLock;

const LAST_GOOD_MAX_ENTRIES: usize = 64;
const LAST_GOOD_MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OverviewStats {
    /// lolalytics' own tier label, e.g. "S-".
    pub tier: String,
    pub win_pct: f64,
    pub avg_win_pct: f64,
    pub pick_pct: f64,
    pub ban_pct: f64,
    pub games: u64,
    pub rank: u32,
    pub rank_total: u32,
    pub patch: String,
}

/// Damage dealt by type, as the raw amounts lolalytics reports.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Damage {
    pub physical: f64,
    pub magic: f64,
    pub true_damage: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Overview {
    /// The lolalytics lane of the page, e.g. "middle".
    pub lane: String,
    pub stats: OverviewStats,
    pub damage: Option<Damage>,
    pub build: Option<KeystoneBuild>,
    /// The page's "Highest Win Build", with its own runes and spells.
    #[serde(default)]
    pub win: Option<lolalytics::WinBuild>,
    /// The wider bracket the build came from when the chosen one had too few games.
    #[serde(default)]
    pub build_tier: Option<String>,
    /// How often each rune is played here; see [`lolalytics::keystone_shares`].
    #[serde(default)]
    pub keystones: Vec<lolalytics::KeystoneShare>,
    pub runes: Option<MatchupRunes>,
    pub spells: Option<[i64; 2]>,
    pub skill_order: Option<String>,
    /// Which patch this is, and whether the previous one was blended in.
    #[serde(default)]
    pub blend: Option<PatchInfo>,
}

/// Blends the previous patch into an overview: win, average win, pick and ban
/// rates move toward the previous patch's (games, rank and tier stay current)
/// and the build merges per [`lolalytics::blend_build`].
pub fn blend_overview(now: Overview, prev: Option<&Overview>, info: PatchInfo) -> Overview {
    let mut stats = now.stats;
    if let Some(prev) = prev {
        let games = stats.games;
        stats.win_pct = blend_rate(stats.win_pct, games, prev.stats.win_pct);
        stats.avg_win_pct = blend_rate(stats.avg_win_pct, games, prev.stats.avg_win_pct);
        stats.pick_pct = blend_rate(stats.pick_pct, games, prev.stats.pick_pct);
        stats.ban_pct = blend_rate(stats.ban_pct, games, prev.stats.ban_pct);
    }
    let build = now
        .build
        .map(|build| lolalytics::blend_build(build, prev.and_then(|prev| prev.build.as_ref()), info.clone()));
    Overview {
        stats,
        build,
        blend: Some(info),
        ..now
    }
}

/// The champion's build page. `lane` is omitted to get its usual lane.
pub fn overview_url(champion_slug: &str, lane: Option<&str>, tier: &str, patch: Option<&str>) -> String {
    let tier = lolalytics::tier_slug(tier);
    lolalytics::patched(
        match lane {
            Some(lane) => format!(
                "{}/{champion_slug}/build/?lane={lane}&tier={tier}",
                lolalytics::BASE_URL
            ),
            None => format!("{}/{champion_slug}/build/?tier={tier}", lolalytics::BASE_URL),
        },
        patch,
    )
}

pub fn overview_cache_key(champion_slug: &str, lane: Option<&str>, tier: &str) -> String {
    format!(
        "{champion_slug}|overview|{}|{}",
        lane.unwrap_or("auto"),
        lolalytics::tier_slug(tier)
    )
}

/// Parses a build page's overview. `None` when the stats object is missing or
/// belongs to another champion than `champion_id`.
pub fn parse_overview(html: &str, champion_id: i64) -> Option<Overview> {
    let state: Qwik = serde_json::from_str(lolalytics::qwik_json(html)?).ok()?;
    let objs = &state.objs;
    let required = ["cid", "lane", "wr", "avgWr", "pr", "br", "rank", "rankTotal", "tier", "patch"];
    let page = objs.iter().find_map(|entry| {
        let map = entry.as_object()?;
        if !required.iter().all(|key| map.contains_key(*key)) {
            return None;
        }
        let page = resolve(entry, objs, 0);
        (page.get("cid").and_then(as_i64) == Some(champion_id)).then_some(page)
    })?;
    let stats = OverviewStats {
        tier: page.get("tier")?.as_str()?.to_string(),
        win_pct: page.get("wr").and_then(number)?,
        avg_win_pct: page.get("avgWr").and_then(number)?,
        pick_pct: page.get("pr").and_then(number)?,
        ban_pct: page.get("br").and_then(number)?,
        games: page.get("n").and_then(as_u64).unwrap_or(0),
        // Off-meta lanes are unranked: lolalytics shows "?" for rank and tier.
        rank: page.get("rank").and_then(as_u64).unwrap_or(0) as u32,
        rank_total: page.get("rankTotal").and_then(as_u64).unwrap_or(0) as u32,
        patch: page.get("patch")?.as_str()?.to_string(),
    };
    let details = pick_details(objs);
    let builds = lolalytics::parse_builds(html);
    Some(Overview {
        lane: page.get("lane")?.as_str()?.to_string(),
        stats,
        damage: page.get("damage").and_then(damage),
        build: builds.as_ref().map(|builds| builds.pick.clone()),
        win: builds.and_then(|builds| builds.win),
        build_tier: None,
        keystones: lolalytics::keystone_shares(objs),
        runes: details.runes,
        spells: details.spells,
        skill_order: details.skill_order,
        blend: None,
    })
}

fn damage(value: &Value) -> Option<Damage> {
    let found = Damage {
        physical: value.get("physical").and_then(number)?,
        magic: value.get("magic").and_then(number)?,
        true_damage: value.get("true").and_then(number)?,
    };
    (found.physical + found.magic + found.true_damage > 0.0).then_some(found)
}

impl LolalyticsClient {
    /// Fetches and parses one champion's build page. `Ok(None)` means the page
    /// loaded without an overview for that champion.
    pub async fn overview(
        &self,
        champion_slug: &str,
        champion_id: i64,
        lane: Option<&str>,
        tier: &str,
        patch: Option<&str>,
    ) -> Result<Option<Overview>, ProviderError> {
        let body = self
            .page(overview_url(champion_slug, lane, tier, patch))
            .await?;
        Ok(lolalytics::checked(&body, "champion page", false, parse_overview(&body, champion_id)))
    }
}

static LAST_GOOD: OnceLock<LastGoodCache<Overview>> = OnceLock::new();

fn last_good() -> &'static LastGoodCache<Overview> {
    LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::Overview),
            lolalytics::SUCCESS_TTL,
            LAST_GOOD_MAX_ENTRIES,
            LAST_GOOD_MAX_BYTES,
        )
    })
}

/// A champion's overview, cached like [`super::counters::load`]: `Ok(None)` for
/// no page, negative-cached failures, one request per key, and the persisted
/// last-good overview when a fetch fails.
pub async fn load(
    champion_slug: &str,
    champion_id: i64,
    lane: Option<&str>,
    tier: &str,
) -> Result<Option<Sourced<Overview>>, RuneError> {
    // The build, runes, damage and skill order still come from the same
    // lolalytics page/cache. A build outage must not hide available stats.
    let (stats, details) = match lane {
        // An explicit lane needs no u.gg answer to start lolalytics, so overlap them.
        Some(lane) => {
            let lane = lolalytics::lane(lane).unwrap_or(lane);
            tokio::join!(
                super::ugg::overview(champion_id, Some(lane), tier),
                load_lolalytics(champion_slug, champion_id, Some(lane), tier)
            )
        }
        // Without one, lolalytics needs the lane u.gg resolves.
        None => {
            let stats = super::ugg::overview(champion_id, None, tier).await;
            let lane = stats.as_ref().map(|s| s.value.lane.as_str());
            let details = load_lolalytics(champion_slug, champion_id, lane, tier).await;
            (stats, details)
        }
    };
    let merged = merge_sources(stats, details)?;
    if merged.as_ref().is_some_and(|found| found.value.build.is_some()) {
        return Ok(merged);
    }
    // An off-meta pick can have no build in a high bracket (Ezreal jungle in
    // Diamond+); borrow one from a wider bracket, keeping this bracket's stats.
    let lane = merged.as_ref().map(|found| found.value.lane.clone())
        .or_else(|| lane.map(|lane| lolalytics::lane(lane).unwrap_or(lane).to_string()));
    for wider in wider_tiers(tier) {
        let Ok(Some(found)) = load_lolalytics(champion_slug, champion_id, lane.as_deref(), wider).await else {
            continue;
        };
        if found.value.build.is_none() {
            continue;
        }
        return Ok(Some(with_wider_build(merged, found, wider)));
    }
    Ok(merged)
}

/// The brackets tried, in order, when `tier` has no build: the next wider one, then all ranks.
fn wider_tiers(tier: &str) -> Vec<&'static str> {
    const ORDER: [&str; 7] = ["challenger", "master_plus", "diamond_plus", "emerald_plus", "platinum_plus", "gold_plus", "all"];
    let at = ORDER.iter().position(|slug| *slug == lolalytics::tier_slug(tier)).unwrap_or(ORDER.len() - 1);
    let mut tiers: Vec<&'static str> = ORDER.get(at + 1).into_iter().copied().collect();
    if !tiers.contains(&"all") && at + 1 < ORDER.len() {
        tiers.push("all");
    }
    tiers
}

/// `merged`'s stats with the build, runes, spells and skill order of a wider bracket.
fn with_wider_build(merged: Option<Sourced<Overview>>, wider: Sourced<Overview>, tier: &str) -> Sourced<Overview> {
    let Some(mut merged) = merged else {
        let mut wider = wider;
        wider.value.build_tier = Some(tier.to_string());
        return wider;
    };
    merged.value.build = wider.value.build;
    merged.value.win = wider.value.win;
    merged.value.runes = wider.value.runes;
    merged.value.spells = wider.value.spells;
    merged.value.skill_order = wider.value.skill_order;
    merged.value.keystones = wider.value.keystones;
    merged.value.build_tier = Some(tier.to_string());
    merged
}

fn merge_sources(
    stats: Option<Sourced<Overview>>,
    details: Result<Option<Sourced<Overview>>, RuneError>,
) -> Result<Option<Sourced<Overview>>, RuneError> {
    if let Some(mut stats) = stats {
        if let Ok(Some(details)) = &details {
            if stats.stale && !details.stale {
                return Ok(Some(details.clone()));
            }
            stats.value.damage = details.value.damage.clone();
            stats.value.build = details.value.build.clone();
            stats.value.win = details.value.win.clone();
            stats.value.runes = details.value.runes.clone();
            stats.value.spells = details.value.spells;
            stats.value.skill_order = details.value.skill_order.clone();
            stats.value.keystones = details.value.keystones.clone();
            // Build consumers use this metadata to decide whether to trust it.
            stats.stale |= details.stale;
            stats.fetched_at = details.fetched_at;
        }
        return Ok(Some(stats));
    }
    details
}

async fn load_lolalytics(
    champion_slug: &str,
    champion_id: i64,
    lane: Option<&str>,
    tier: &str,
) -> Result<Option<Sourced<Overview>>, RuneError> {
    let key = overview_cache_key(champion_slug, lane, tier);
    if let Some(cached) = cached(&key) {
        return Ok(cached);
    }
    let gate = keystone_gate(&key);
    let _guard = gate.lock().await;
    if let Some(cached) = cached(&key) {
        return Ok(cached);
    }
    if let Some(found) = last_good().fresh(&key) {
        super::shared().overviews.store(key.clone(), found.value);
        return Ok(stale(&key));
    }
    let _permit = keystone_fetches()
        .acquire()
        .await
        .expect("the fetch semaphore is never closed");
    let Ok(client) = LolalyticsClient::new() else {
        return Ok(stale(&key));
    };
    match client.overview(champion_slug, champion_id, lane, tier, None).await {
        Ok(Some(found)) => {
            let found = patch::blended(
                found,
                &key,
                last_good(),
                |previous| {
                    let client = &client;
                    async move {
                        client
                            .overview(champion_slug, champion_id, lane, tier, Some(&previous))
                            .await
                    }
                },
                blend_overview,
            )
            .await;
            super::shared().overviews.store(key.clone(), found.clone());
            last_good().store(key, found.clone());
            Ok(Some(Sourced::fresh(found, Provider::Lolalytics)))
        }
        Ok(None) => {
            super::shared().overviews.fail(key);
            Ok(None)
        }
        Err(_) => {
            super::shared().overviews.fail(key.clone());
            Ok(stale(&key))
        }
    }
}

fn cached(key: &str) -> Option<Option<Sourced<Overview>>> {
    match super::shared().overviews.lookup(key) {
        lolalytics::Cached::Fresh(found) => Some(Some(Sourced {
            value: found,
            provider: Provider::Lolalytics,
            stale: false,
            fetched_at: None,
        })),
        lolalytics::Cached::Unavailable => Some(stale(key)),
        lolalytics::Cached::Miss => None,
    }
}

fn stale(key: &str) -> Option<Sourced<Overview>> {
    last_good().lookup(key).map(|cached| {
        if cached.fresh {
            Sourced {
                value: cached.value,
                provider: Provider::Lolalytics,
                stale: false,
                fetched_at: Some(cached.fetched_at),
            }
        } else {
            Sourced::stale(cached.value, Provider::Lolalytics, cached.fetched_at)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ugg_stats_replace_only_stats_and_missing_or_stale_stats_fall_back() {
        let details = Overview {
            lane: "middle".into(),
            stats: OverviewStats { win_pct: 53.0, games: 1234, ..Default::default() },
            build: Some(KeystoneBuild { items: vec![3118,3020,4645], ..Default::default() }),
            skill_order: Some("QWE".into()),
            ..Default::default()
        };
        let stats = Overview { lane:"middle".into(), stats:OverviewStats { win_pct:51.0,games:1000,..Default::default() },..Default::default() };
        let found = merge_sources(Some(Sourced::fresh(stats.clone(),Provider::Ugg)),Ok(Some(Sourced::fresh(details.clone(),Provider::Lolalytics)))).unwrap().unwrap();
        assert_eq!((found.value.stats.win_pct,found.value.stats.games,found.provider),(51.0,1000,Provider::Ugg));
        assert_eq!(found.value.build,details.build);
        assert_eq!(found.value.skill_order,details.skill_order);
        let found = merge_sources(None,Ok(Some(Sourced::fresh(details.clone(),Provider::Lolalytics)))).unwrap().unwrap();
        assert_eq!(found.value,details);
        let found = merge_sources(Some(Sourced::stale(stats.clone(),Provider::Ugg,0)),Ok(Some(Sourced::fresh(details.clone(),Provider::Lolalytics)))).unwrap().unwrap();
        assert_eq!(found.value,details);
        let found = merge_sources(Some(Sourced::fresh(stats,Provider::Ugg)),Ok(None)).unwrap().unwrap();
        assert_eq!(found.value.stats.win_pct,51.0);
        assert!(found.value.build.is_none());
    }

    /// A trimmed Ahri (103) build page, numbers from a real one. Index 0 is
    /// the page object, 1 the damage object, 2 the pick/win summary.
    const AHRI: &str = r#"<script type="qwik/json">{"objs":[
        {"cid":"a","lane":"b","wr":"c","avgWr":51.88,"pr":8.86,"br":"2.73","n":13281,
         "rank":10,"rankTotal":98,"tier":"S-","patch":"16.20","damage":"1"},
        {"physical":"d","magic":"e","true":"f"},
        {"pick":"3","win":"3"},
        {"items":"4","skillpriority":"6"},
        {"core":"5"},
        {"set":[3118,3020,4645]},
        {"id":"QWE","n":10,"wr":50},
        0,0,0,
        103,"middle","52.94",1737.36,18222.55,5362.05
    ]}</script>"#;

    #[test]
    fn a_missing_build_borrows_the_next_wider_bracket_then_all_ranks() {
        assert_eq!(wider_tiers("diamond_plus"), vec!["emerald_plus", "all"]);
        assert_eq!(wider_tiers("gold_plus"), vec!["all"]);
        assert!(wider_tiers("all").is_empty());
        let stats = Overview { lane: "jungle".into(), stats: OverviewStats { games: 73, ..Default::default() }, ..Default::default() };
        let wider = Overview { build: Some(KeystoneBuild { items: vec![3078, 3158, 3042], ..Default::default() }), skill_order: Some("QEW".into()), ..Default::default() };
        let merged = with_wider_build(Some(Sourced::fresh(stats, Provider::Ugg)), Sourced::fresh(wider, Provider::Lolalytics), "emerald_plus");
        assert_eq!(merged.value.stats.games, 73, "the chosen bracket's stats stay");
        assert_eq!(merged.value.build.map(|build| build.items), Some(vec![3078, 3158, 3042]));
        assert_eq!(merged.value.build_tier.as_deref(), Some("emerald_plus"));
    }

    #[test]
    fn the_highest_win_build_survives_the_ugg_stats_merge() {
        let win = lolalytics::parse_builds(include_str!("../../tests/fixtures/lolalytics-ahri-mid-8112.html"))
            .and_then(|builds| builds.win)
            .expect("the fixture has a highest win build");
        let details = Overview { lane: "middle".into(), win: Some(win.clone()), ..Default::default() };
        let stats = Overview { lane: "middle".into(), stats: OverviewStats { games: 1000, ..Default::default() }, ..Default::default() };
        let merged = merge_sources(Some(Sourced::fresh(stats, Provider::Ugg)), Ok(Some(Sourced::fresh(details, Provider::Lolalytics))))
            .unwrap()
            .unwrap();
        assert_eq!((merged.value.stats.games, merged.value.win), (1000, Some(win)));
    }

    #[test]
    fn an_unranked_off_meta_lane_still_parses() {
        // Ezreal jungle: lolalytics has games and a build but no tier-list rank.
        let page = AHRI.replace(r#""rank":10,"rankTotal":98,"tier":"S-""#, r#""rank":"?","rankTotal":"?","tier":"?""#);
        assert_ne!(page, AHRI, "the fixture still carries a rank to replace");
        let overview = parse_overview(&page, 103).expect("an unranked lane is still a page");
        assert_eq!((overview.stats.rank, overview.stats.rank_total, overview.stats.tier.as_str()), (0, 0, "?"));
        assert!(overview.build.is_some());
    }

    #[test]
    fn parses_stats_and_the_damage_split() {
        let overview = parse_overview(AHRI, 103).expect("the page should parse");
        assert_eq!(overview.lane, "middle");
        assert_eq!(overview.stats.tier, "S-");
        assert_eq!(overview.stats.win_pct, 52.94);
        assert_eq!(overview.stats.avg_win_pct, 51.88);
        assert_eq!((overview.stats.rank, overview.stats.rank_total), (10, 98));
        assert_eq!(overview.stats.ban_pct, 2.73);
        assert_eq!(overview.stats.games, 13281);
        let damage = overview.damage.expect("damage is on the page");
        assert_eq!(damage.magic, 18222.55);
        assert_eq!(damage.true_damage, 5362.05);
    }

    #[test]
    fn a_page_for_another_champion_or_without_stats_is_rejected() {
        assert!(parse_overview(AHRI, 266).is_none());
        assert!(parse_overview("<html>Just a moment…</html>", 103).is_none());
    }

    #[test]
    fn missing_damage_keeps_the_stats() {
        let html = AHRI.replace(r#","damage":"1""#, "");
        let overview = parse_overview(&html, 103).expect("stats are enough");
        assert_eq!(overview.damage, None);
    }

    #[test]
    fn urls_and_keys_carry_lane_and_bracket() {
        assert_eq!(
            overview_url("ahri", Some("middle"), "emerald_plus", None),
            "https://lolalytics.com/lol/ahri/build/?lane=middle&tier=emerald_plus"
        );
        assert_eq!(
            overview_url("ahri", None, "bogus", None),
            "https://lolalytics.com/lol/ahri/build/?tier=emerald_plus"
        );
        assert_ne!(
            overview_cache_key("ahri", None, "emerald_plus"),
            overview_cache_key("ahri", Some("middle"), "emerald_plus")
        );
    }

    #[test]
    fn the_overview_blends_rates_but_keeps_the_current_games_and_rank() {
        let now = Overview {
            stats: OverviewStats { win_pct: 50.0, avg_win_pct: 50.0, pick_pct: 5.0, ban_pct: 1.0, games: 0, rank: 3, ..Default::default() },
            ..Default::default()
        };
        let mut prev = now.clone();
        prev.stats = OverviewStats { win_pct: 53.0, avg_win_pct: 50.5, pick_pct: 8.0, ban_pct: 2.0, games: 90_000, rank: 9, ..Default::default() };
        let info = PatchInfo { patch: "16.20".into(), previous: Some("16.19".into()) };
        let blended = blend_overview(now, Some(&prev), info);
        assert!((blended.stats.win_pct - 53.0).abs() < 1e-9, "day 0 is the previous patch's rate");
        assert_eq!((blended.stats.games, blended.stats.rank), (0, 3));
        assert!(blended.blend.unwrap().previous.is_some());
    }
}
