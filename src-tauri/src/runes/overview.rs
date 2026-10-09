//! A champion's overview from its lolalytics build page: tier, win/pick/ban
//! rates, lane rank, patch, damage split, and the most common build.
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
    pub runes: Option<MatchupRunes>,
    pub spells: Option<[i64; 2]>,
    pub skill_order: Option<String>,
}

/// The champion's build page. `lane` is omitted to get its usual lane.
pub fn overview_url(champion_slug: &str, lane: Option<&str>, tier: &str) -> String {
    let tier = lolalytics::tier_slug(tier);
    match lane {
        Some(lane) => format!(
            "{}/{champion_slug}/build/?lane={lane}&tier={tier}",
            lolalytics::BASE_URL
        ),
        None => format!("{}/{champion_slug}/build/?tier={tier}", lolalytics::BASE_URL),
    }
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
        rank: page.get("rank").and_then(as_u64)? as u32,
        rank_total: page.get("rankTotal").and_then(as_u64)? as u32,
        patch: page.get("patch")?.as_str()?.to_string(),
    };
    let details = pick_details(objs);
    Some(Overview {
        lane: page.get("lane")?.as_str()?.to_string(),
        stats,
        damage: page.get("damage").and_then(damage),
        build: lolalytics::parse_build(html),
        runes: details.runes,
        spells: details.spells,
        skill_order: details.skill_order,
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
    ) -> Result<Option<Overview>, ProviderError> {
        let body = self.page(overview_url(champion_slug, lane, tier)).await?;
        Ok(parse_overview(&body, champion_id))
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
    let key = overview_cache_key(champion_slug, lane, tier);
    if let Some(cached) = cached(&key) {
        return Ok(cached);
    }
    let gate = keystone_gate(&key);
    let _guard = gate.lock().await;
    if let Some(cached) = cached(&key) {
        return Ok(cached);
    }
    let _permit = keystone_fetches()
        .acquire()
        .await
        .expect("the fetch semaphore is never closed");
    let Ok(client) = LolalyticsClient::new() else {
        return Ok(stale(&key));
    };
    match client.overview(champion_slug, champion_id, lane, tier).await {
        Ok(Some(found)) => {
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
            overview_url("ahri", Some("middle"), "emerald_plus"),
            "https://lolalytics.com/lol/ahri/build/?lane=middle&tier=emerald_plus"
        );
        assert_eq!(
            overview_url("ahri", None, "bogus"),
            "https://lolalytics.com/lol/ahri/build/?tier=emerald_plus"
        );
        assert_ne!(
            overview_cache_key("ahri", None, "emerald_plus"),
            overview_cache_key("ahri", Some("middle"), "emerald_plus")
        );
    }
}
