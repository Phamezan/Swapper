//! The lolalytics tier list for one lane and rank bracket.
//!
//! The Qwik state holds one dictionary keyed by champion id (the same ids as
//! the page's `champId` slug map, checked for all 173 champions of a real page)
//! whose values are row objects `{rank, tier, wr, pr, br, games, ...}`. `tier`
//! is 1-15 and indexes the page's own label list
//! `["", "S+", "S", "S-", "A+", "A", "A-", "B+", "B", "B-", "C+", "C", "C-",
//! "D+", "D", "D-"]`. A row with `rank` 0 is not ranked in this lane.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::cache::{self, LastGoodCache};
use super::data::{keystone_fetches, keystone_gate};
use super::lolalytics::{self, as_u64, number, resolve, LolalyticsClient, Qwik};
use super::provider::{DataKind, Provider, ProviderError, Sourced};
use super::RuneError;

use std::sync::OnceLock;

/// Rows with fewer games are too noisy to rank.
pub const MIN_TIER_GAMES: u64 = 500;
/// Challenger's pool is a few dozen players, so its champions accrue a few
/// hundred games each rather than thousands; the default floor would drop
/// every row. The page still ranks them, so a lower floor keeps the list.
const MIN_CHALLENGER_GAMES: u64 = 100;

/// The games floor for one rank bracket. The smallest bracket needs a lower
/// one, since its whole sample is a fraction of the others'.
fn min_games(tier: &str) -> u64 {
    match tier.trim().to_ascii_lowercase().as_str() {
        "challenger" => MIN_CHALLENGER_GAMES,
        _ => MIN_TIER_GAMES,
    }
}
/// The page's tier labels, indexed by the numeric `tier`.
const TIER_LABELS: [&str; 16] = [
    "", "S+", "S", "S-", "A+", "A", "A-", "B+", "B", "B-", "C+", "C", "C-", "D+", "D", "D-",
];
const LAST_GOOD_MAX_ENTRIES: usize = 32;
const LAST_GOOD_MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TierRow {
    pub champion_id: i64,
    /// 1 (S+) to 15 (D-).
    pub tier: u8,
    pub win_pct: f64,
    pub pick_pct: f64,
    pub ban_pct: f64,
    pub games: u64,
}

impl TierRow {
    /// The label lolalytics shows for this row's tier.
    pub fn label(&self) -> &'static str {
        TIER_LABELS.get(self.tier as usize).copied().unwrap_or("")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TierList {
    /// Rows ordered by tier, then win rate (best first).
    pub rows: Vec<TierRow>,
}

pub fn tierlist_url(lane: &str, tier: &str) -> String {
    format!(
        "{}/tierlist/?lane={lane}&tier={}",
        lolalytics::BASE_URL,
        lolalytics::tier_slug(tier)
    )
}

pub fn tierlist_cache_key(lane: &str, tier: &str) -> String {
    format!("tierlist|{lane}|{}", lolalytics::tier_slug(tier))
}

/// Parses a tier list page. `None` when no champion dictionary is found.
pub fn parse_tierlist(html: &str) -> Option<TierList> {
    parse_tierlist_with_min(html, MIN_TIER_GAMES)
}

/// Parses a tier list page, dropping rows below `min_games`. `None` when no
/// champion dictionary is found.
fn parse_tierlist_with_min(html: &str, min_games: u64) -> Option<TierList> {
    let state: Qwik = serde_json::from_str(lolalytics::qwik_json(html)?).ok()?;
    let objs = &state.objs;
    let table = objs.iter().find(|entry| is_row_table(entry, objs))?;
    let mut rows: Vec<TierRow> = table
        .as_object()?
        .iter()
        .filter_map(|(key, row)| tier_row(key.parse().ok()?, &resolve(row, objs, 0), min_games))
        .collect();
    rows.sort_by(|left, right| {
        left.tier
            .cmp(&right.tier)
            .then_with(|| right.win_pct.total_cmp(&left.win_pct))
    });
    Some(TierList { rows })
}

/// The champion dictionary: many entries whose values are row objects.
fn is_row_table(entry: &Value, objs: &[Value]) -> bool {
    let Some(map) = entry.as_object() else {
        return false;
    };
    let is_row = |value: &Value| {
        let row = resolve(value, objs, 0);
        row.get("rank").is_some() && row.get("pbi").is_some() && row.get("games").is_some()
    };
    map.len() >= 20 && map.values().take(3).all(is_row)
}

fn tier_row(champion_id: i64, row: &Value, min_games: u64) -> Option<TierRow> {
    let rank = row.get("rank").and_then(as_u64)?;
    let tier = row.get("tier").and_then(as_u64)?;
    let games = row.get("games").and_then(as_u64)?;
    if champion_id <= 0 || rank == 0 || !(1..=15).contains(&tier) || games < min_games {
        return None;
    }
    Some(TierRow {
        champion_id,
        tier: tier as u8,
        win_pct: row.get("wr").and_then(number)?,
        pick_pct: row.get("pr").and_then(number)?,
        ban_pct: row.get("br").and_then(number)?,
        games,
    })
}

impl LolalyticsClient {
    /// Fetches and parses one lane's tier list.
    pub async fn tierlist(&self, lane: &str, tier: &str) -> Result<Option<TierList>, ProviderError> {
        let body = self.page(tierlist_url(lane, tier)).await?;
        Ok(parse_tierlist_with_min(&body, min_games(tier)))
    }
}

static LAST_GOOD: OnceLock<LastGoodCache<TierList>> = OnceLock::new();

fn last_good() -> &'static LastGoodCache<TierList> {
    LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::TierList),
            lolalytics::SUCCESS_TTL,
            LAST_GOOD_MAX_ENTRIES,
            LAST_GOOD_MAX_BYTES,
        )
    })
}

/// One lane's tier list, cached like [`super::counters::load`].
pub async fn load(lane: &str, tier: &str) -> Result<Option<Sourced<TierList>>, RuneError> {
    let key = tierlist_cache_key(lane, tier);
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
    match client.tierlist(lane, tier).await {
        Ok(Some(found)) => {
            super::shared().tierlists.store(key.clone(), found.clone());
            last_good().store(key, found.clone());
            Ok(Some(Sourced::fresh(found, Provider::Lolalytics)))
        }
        Ok(None) => {
            super::shared().tierlists.fail(key);
            Ok(None)
        }
        Err(_) => {
            super::shared().tierlists.fail(key.clone());
            Ok(stale(&key))
        }
    }
}

fn cached(key: &str) -> Option<Option<Sourced<TierList>>> {
    match super::shared().tierlists.lookup(key) {
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

fn stale(key: &str) -> Option<Sourced<TierList>> {
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

    fn base36(mut number: usize) -> String {
        let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut out = vec![digits[number % 36]];
        while number >= 36 {
            number /= 36;
            out.push(digits[number % 36]);
        }
        out.reverse();
        String::from_utf8(out).unwrap()
    }

    /// Builds a page with `count` filler rows (ids 1..), then real values for
    /// Cho'Gath (31, tier 1), Master Yi (11, tier 2) and rows to be dropped.
    fn page() -> String {
        let mut objs = vec![String::from("{}")];
        let mut table = Vec::new();
        let rows = [
            (31, r#"{"rank":1,"tier":1,"wr":53.17,"pr":6.19,"br":7.3,"pbi":11,"games":27107}"#),
            (11, r#"{"rank":3,"tier":2,"wr":52.45,"pr":6.54,"br":15.2,"pbi":8,"games":28605}"#),
            (62, r#"{"rank":2,"tier":1,"wr":53.61,"pr":7.56,"br":5.62,"pbi":17,"games":33067}"#),
            // Not ranked in this lane, and a tiny sample.
            (7, r#"{"rank":0,"tier":0,"wr":50,"pr":0,"br":0.57,"pbi":0,"games":4}"#),
            (8, r#"{"rank":40,"tier":9,"wr":49,"pr":0.1,"br":0.1,"pbi":0,"games":120}"#),
        ];
        for id in 1..=20 {
            let (key, body) = rows
                .get(id - 1)
                .map(|(key, body)| (*key, *body))
                .unwrap_or((100 + id as i64, r#"{"rank":0,"tier":0,"wr":50,"pr":0,"br":0,"pbi":0,"games":1}"#));
            objs.push(body.to_string());
            let index = objs.len() - 1;
            table.push(format!(r#""{key}":"{}""#, base36(index)));
        }
        objs[0] = format!("{{{}}}", table.join(","));
        format!(r#"<script type="qwik/json">{{"objs":[{}]}}</script>"#, objs.join(","))
    }

    #[test]
    fn maps_rows_to_champion_ids_and_orders_by_tier_then_win_rate() {
        let list = parse_tierlist(&page()).expect("the page should parse");
        let ids: Vec<i64> = list.rows.iter().map(|row| row.champion_id).collect();
        // Tier 1: Wukong (53.61) before Cho'Gath (53.17), then tier 2 Master Yi.
        assert_eq!(ids, vec![62, 31, 11]);
        assert_eq!(list.rows[1].label(), "S+");
        assert_eq!(list.rows[2].label(), "S");
        assert_eq!(list.rows[1].games, 27107);
        assert_eq!(list.rows[1].ban_pct, 7.3);
    }

    #[test]
    fn unranked_and_tiny_sample_rows_are_dropped() {
        let list = parse_tierlist(&page()).unwrap();
        assert!(list.rows.iter().all(|row| row.champion_id != 7 && row.champion_id != 8));
        assert!(list.rows.iter().all(|row| row.games >= MIN_TIER_GAMES));
    }

    /// A page whose rows all have `games` games, one per champion id.
    fn page_with_games(games: u64) -> String {
        let mut objs = vec![String::from("{}")];
        let mut table = Vec::new();
        for id in 1..=20 {
            objs.push(format!(
                r#"{{"rank":{id},"tier":1,"wr":52.0,"pr":5.0,"br":3.0,"pbi":1,"games":{games}}}"#
            ));
            let index = objs.len() - 1;
            table.push(format!(r#""{id}":"{}""#, base36(index)));
        }
        objs[0] = format!("{{{}}}", table.join(","));
        format!(r#"<script type="qwik/json">{{"objs":[{}]}}</script>"#, objs.join(","))
    }

    #[test]
    fn the_challenger_bracket_keeps_rows_the_default_floor_drops() {
        assert!(min_games("challenger") < MIN_TIER_GAMES);
        assert_eq!(min_games("emerald_plus"), MIN_TIER_GAMES);
        assert_eq!(min_games("CHALLENGER"), MIN_CHALLENGER_GAMES);

        // Every row has 200 games: below the default floor, above Challenger's.
        let html = page_with_games(200);
        assert_eq!(parse_tierlist(&html).unwrap().rows.len(), 0);
        let challenger = parse_tierlist_with_min(&html, min_games("challenger")).unwrap();
        assert_eq!(challenger.rows.len(), 20);
    }

    #[test]
    fn tier_numbers_index_the_pages_labels() {
        let label = |tier| TierRow { tier, ..TierRow::default() }.label();
        assert_eq!((label(1), label(3), label(4), label(15), label(0), label(99)), ("S+", "S-", "A+", "D-", "", ""));
    }

    #[test]
    fn a_page_without_the_table_gives_nothing() {
        assert!(parse_tierlist("<html>Just a moment…</html>").is_none());
        assert!(parse_tierlist(r#"<script type="qwik/json">{"objs":[{"a":1}]}</script>"#).is_none());
    }

    #[test]
    fn urls_and_keys_carry_lane_and_bracket() {
        assert_eq!(
            tierlist_url("jungle", "emerald_plus"),
            "https://lolalytics.com/lol/tierlist/?lane=jungle&tier=emerald_plus"
        );
        assert_ne!(tierlist_cache_key("jungle", "all"), tierlist_cache_key("top", "all"));
        assert_ne!(tierlist_cache_key("jungle", "all"), tierlist_cache_key("jungle", "gold_plus"));
    }
}
