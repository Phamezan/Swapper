//! Per-champion matchup tables from lolalytics' `/counters/` pages.
//!
//! The page holds the same Qwik state as the build page. Its `counters` list has
//! one row per opposing champion: `vsWr` is THIS champion's win rate into that
//! opponent, `n` the games, `allWr` the opponent's overall win rate. The page's
//! `d1` equals `vsWr - allWr` on every row of a real page (checked); `d2` is
//! `d1` minus a per-page constant and is not used.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::data::{keystone_fetches, keystone_gate};
use super::lolalytics::{self, as_i64, as_u64, number, resolve, LolalyticsClient, Qwik};
use super::provider::{DataKind, Provider, ProviderError, Sourced};
use super::cache::{self, LastGoodCache};
use super::RuneError;

use std::sync::OnceLock;

const LAST_GOOD_MAX_ENTRIES: usize = 128;
const LAST_GOOD_MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CounterRow {
    /// The opposing champion.
    pub champion_id: i64,
    /// The searched champion's win rate into that opponent.
    pub vs_win_pct: f64,
    pub games: u64,
    /// The opponent's overall win rate.
    pub all_win_pct: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Counters {
    /// The lolalytics lane the page was for, e.g. "middle".
    pub lane: String,
    pub rows: Vec<CounterRow>,
}

/// The counters page. `lane` is omitted to get the champion's usual lane.
pub fn counters_url(champion_slug: &str, lane: Option<&str>, tier: &str) -> String {
    let tier = lolalytics::tier_slug(tier);
    match lane {
        Some(lane) => format!(
            "{}/{champion_slug}/counters/?lane={lane}&tier={tier}",
            lolalytics::BASE_URL
        ),
        None => format!("{}/{champion_slug}/counters/?tier={tier}", lolalytics::BASE_URL),
    }
}

pub fn counters_cache_key(champion_slug: &str, lane: Option<&str>, tier: &str) -> String {
    format!(
        "{champion_slug}|counters|{}|{}",
        lane.unwrap_or("auto"),
        lolalytics::tier_slug(tier)
    )
}

/// Parses a counters page. `None` when the rows are missing or the page is for
/// a different champion than `champion_id`.
pub fn parse_counters(html: &str, champion_id: i64) -> Option<Counters> {
    let state: Qwik = serde_json::from_str(lolalytics::qwik_json(html)?).ok()?;
    let objs = &state.objs;
    let stats = objs.iter().find_map(|entry| {
        let map = entry.as_object()?;
        if !["cid", "lane", "counters"].iter().all(|key| map.contains_key(*key)) {
            return None;
        }
        let page = resolve(entry, objs, 0);
        (page.get("cid").and_then(as_i64) == Some(champion_id)).then_some(page)
    })?;
    let rows = objs.iter().find_map(|entry| {
        let list = resolve(entry.as_object()?.get("counters")?, objs, 0);
        let rows: Vec<CounterRow> = list.as_array()?.iter().filter_map(row).collect();
        (!rows.is_empty()).then_some(rows)
    })?;
    Some(Counters {
        lane: stats.get("lane")?.as_str()?.to_string(),
        rows,
    })
}

fn row(value: &Value) -> Option<CounterRow> {
    let champion_id = value.get("cid").and_then(as_i64).filter(|id| *id > 0)?;
    Some(CounterRow {
        champion_id,
        vs_win_pct: value.get("vsWr").and_then(number)?,
        games: value.get("n").and_then(as_u64)?,
        all_win_pct: value.get("allWr").and_then(number)?,
    })
}

impl LolalyticsClient {
    /// Fetches and parses one counters page. `Ok(None)` means the page loaded
    /// but had no matchup rows for that champion.
    pub async fn counters(
        &self,
        champion_slug: &str,
        champion_id: i64,
        lane: Option<&str>,
        tier: &str,
    ) -> Result<Option<Counters>, ProviderError> {
        let body = self.page(counters_url(champion_slug, lane, tier)).await?;
        Ok(parse_counters(&body, champion_id))
    }
}

static COUNTERS_LAST_GOOD: OnceLock<LastGoodCache<Counters>> = OnceLock::new();

fn last_good() -> &'static LastGoodCache<Counters> {
    COUNTERS_LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::Counters),
            lolalytics::SUCCESS_TTL,
            LAST_GOOD_MAX_ENTRIES,
            LAST_GOOD_MAX_BYTES,
        )
    })
}

/// A champion's matchup table, cached for the session like
/// [`super::data::matchup`]: `Ok(None)` means no page, failures are
/// negative-cached, concurrent misses share one request, and a failed fetch
/// serves the persisted last-good table.
pub async fn load(
    champion_slug: &str,
    champion_id: i64,
    lane: Option<&str>,
    tier: &str,
) -> Result<Option<Sourced<Counters>>, RuneError> {
    let key = counters_cache_key(champion_slug, lane, tier);
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
    match client.counters(champion_slug, champion_id, lane, tier).await {
        Ok(Some(found)) => {
            super::shared().counters.store(key.clone(), found.clone());
            last_good().store(key, found.clone());
            Ok(Some(Sourced::fresh(found, Provider::Lolalytics)))
        }
        Ok(None) => {
            super::shared().counters.fail(key);
            Ok(None)
        }
        Err(_) => {
            super::shared().counters.fail(key.clone());
            Ok(stale(&key))
        }
    }
}

fn cached(key: &str) -> Option<Option<Sourced<Counters>>> {
    match super::shared().counters.lookup(key) {
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

fn stale(key: &str) -> Option<Sourced<Counters>> {
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

    /// A trimmed Graves (104) jungle page with numbers from a real one. Index
    /// 0 is the page object, 1 the table; page-level `counters` is a different
    /// shape (strong/weak ids) and must be ignored.
    const GRAVES: &str = r#"<script type="qwik/json">{"objs":[
        {"cid":"2","lane":"3","counters":"4"},
        {"stats":"0","counters":"5"},
        104,
        "jungle",
        {"strong":[48,24],"weak":[427]},
        [{"cid":48,"vsWr":58.72,"n":109,"d1":9.43,"d2":6.96,"allWr":49.29},
         {"cid":120,"vsWr":54.66,"n":988,"d1":5.35,"d2":2.88,"allWr":49.31},
         {"cid":427,"vsWr":44.1,"n":30,"d1":-5.0,"d2":-7.5,"allWr":49.1}]
    ]}</script>"#;

    #[test]
    fn parses_rows_and_the_page_lane() {
        let counters = parse_counters(GRAVES, 104).expect("the page should parse");
        assert_eq!(counters.lane, "jungle");
        assert_eq!(counters.rows.len(), 3);
        assert_eq!(
            counters.rows[0],
            CounterRow {
                champion_id: 48,
                vs_win_pct: 58.72,
                games: 109,
                all_win_pct: 49.29,
            }
        );
        // d1 is vsWr - allWr, which is what the UI computes from the row.
        let first = &counters.rows[0];
        assert!((first.vs_win_pct - first.all_win_pct - 9.43).abs() < 0.011);
    }

    #[test]
    fn a_page_for_another_champion_or_without_rows_is_rejected() {
        assert!(parse_counters(GRAVES, 266).is_none());
        assert!(parse_counters("<html>Just a moment…</html>", 104).is_none());
        let empty = GRAVES.replace("\"counters\":\"5\"", "\"counters\":\"4\"");
        assert!(parse_counters(&empty, 104).is_none());
    }

    #[test]
    fn urls_and_keys_carry_lane_and_bracket() {
        assert_eq!(
            counters_url("graves", Some("jungle"), "emerald_plus"),
            "https://lolalytics.com/lol/graves/counters/?lane=jungle&tier=emerald_plus"
        );
        assert_eq!(
            counters_url("graves", None, "bogus"),
            "https://lolalytics.com/lol/graves/counters/?tier=emerald_plus"
        );
        assert_ne!(
            counters_cache_key("graves", None, "emerald_plus"),
            counters_cache_key("graves", Some("jungle"), "emerald_plus")
        );
        assert_ne!(
            counters_cache_key("graves", None, "emerald_plus"),
            counters_cache_key("graves", None, "diamond_plus")
        );
    }
}
