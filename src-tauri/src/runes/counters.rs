//! Per-champion matchup tables from u.gg, with lolalytics as fallback.
//!
//! The fallback page holds the same Qwik state as the build page. Its `counters` list has
//! one row per opposing champion: `vsWr` is THIS champion's win rate into that
//! opponent, `n` the games, `allWr` the opponent's overall win rate. The page's
//! `d1` equals `vsWr - allWr` on every row of a real page (checked); `d2` is
//! `d1` minus a per-page constant and is not used.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::data::{keystone_fetches, keystone_gate};
use super::lolalytics::{self, as_i64, as_u64, number, resolve, LolalyticsClient, Qwik};
use super::patch::{self, blend_rate, PatchInfo};
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
    /// Which patch this is, and whether the previous one was blended in.
    #[serde(default)]
    pub patch: Option<PatchInfo>,
}

/// Blends the previous patch's table into the current one: each matchup's win
/// rates are blended per [`blend_rate`] and the games stay the current patch's.
/// Matchups only the previous patch has come last with 0 games, so they sort as
/// low-sample rows.
pub fn blend_counters(now: Counters, prev: Option<&Counters>, info: PatchInfo) -> Counters {
    let Some(prev) = prev else {
        return Counters { patch: Some(info), ..now };
    };
    let mut rows: Vec<CounterRow> = now
        .rows
        .iter()
        .map(|row| match prev.rows.iter().find(|other| other.champion_id == row.champion_id) {
            Some(other) => CounterRow {
                vs_win_pct: blend_rate(row.vs_win_pct, row.games, other.vs_win_pct),
                all_win_pct: blend_rate(row.all_win_pct, row.games, other.all_win_pct),
                ..row.clone()
            },
            None => row.clone(),
        })
        .collect();
    rows.extend(
        prev.rows
            .iter()
            .filter(|other| !now.rows.iter().any(|row| row.champion_id == other.champion_id))
            .map(|other| CounterRow { games: 0, ..other.clone() }),
    );
    Counters {
        rows,
        patch: Some(info),
        ..now
    }
}

/// The counters page. `lane` is omitted to get the champion's usual lane.
pub fn counters_url(champion_slug: &str, lane: Option<&str>, tier: &str, patch: Option<&str>) -> String {
    let tier = lolalytics::tier_slug(tier);
    lolalytics::patched(
        match lane {
            Some(lane) => format!(
                "{}/{champion_slug}/counters/?lane={lane}&tier={tier}",
                lolalytics::BASE_URL
            ),
            None => format!("{}/{champion_slug}/counters/?tier={tier}", lolalytics::BASE_URL),
        },
        patch,
    )
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
        patch: None,
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
        patch: Option<&str>,
    ) -> Result<Option<Counters>, ProviderError> {
        let body = self
            .page(counters_url(champion_slug, lane, tier, patch))
            .await?;
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
    super::ugg::prefer(
        super::ugg::counters(champion_id, lane, tier).await,
        load_lolalytics(champion_slug, champion_id, lane, tier),
    ).await
}

async fn load_lolalytics(
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
    match client.counters(champion_slug, champion_id, lane, tier, None).await {
        Ok(Some(found)) => {
            let found = patch::blended(
                found,
                &key,
                last_good(),
                |previous| {
                    let client = &client;
                    async move {
                        client
                            .counters(champion_slug, champion_id, lane, tier, Some(&previous))
                            .await
                    }
                },
                blend_counters,
            )
            .await;
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
            counters_url("graves", Some("jungle"), "emerald_plus", None),
            "https://lolalytics.com/lol/graves/counters/?lane=jungle&tier=emerald_plus"
        );
        assert_eq!(
            counters_url("graves", None, "bogus", None),
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

    fn row(champion_id: i64, vs: f64, games: u64, all: f64) -> CounterRow {
        CounterRow { champion_id, vs_win_pct: vs, games, all_win_pct: all }
    }

    fn info() -> PatchInfo {
        PatchInfo { patch: "16.20".into(), previous: Some("16.19".into()) }
    }

    #[test]
    fn day_zero_counters_take_the_previous_patch_rates_and_keep_zero_games() {
        let now = parse_counters(GRAVES, 104).unwrap();
        // Day 0: the current page has the matchups but no games yet.
        let empty = Counters {
            rows: now.rows.iter().map(|r| CounterRow { games: 0, ..r.clone() }).collect(),
            ..now
        };
        let prev = Counters { rows: vec![row(48, 51.0, 9000, 49.0), row(120, 53.0, 9000, 49.0)], ..empty.clone() };
        let blended = blend_counters(empty, Some(&prev), info());
        let first = blended.rows.iter().find(|r| r.champion_id == 48).unwrap();
        assert!((first.vs_win_pct - 51.0).abs() < 1e-9);
        assert_eq!(first.games, 0, "displayed games are the current patch's");
        assert!(blended.patch.unwrap().previous.is_some());
    }

    #[test]
    fn a_large_current_counters_sample_dominates_and_prev_only_rows_come_last() {
        let now = Counters { lane: "jungle".into(), rows: vec![row(48, 58.0, 5000, 49.0)], patch: None };
        let prev = Counters { rows: vec![row(48, 52.0, 8000, 49.0), row(7, 47.0, 3000, 50.0)], ..now.clone() };
        let blended = blend_counters(now, Some(&prev), info());
        assert!((blended.rows[0].vs_win_pct - 58.0).abs() < 0.5, "{}", blended.rows[0].vs_win_pct);
        assert_eq!(blended.rows[0].games, 5000);
        assert_eq!((blended.rows[1].champion_id, blended.rows[1].games), (7, 0));
        assert!((blended.rows[1].vs_win_pct - 47.0).abs() < 1e-9);
    }

    #[test]
    fn without_a_previous_patch_the_counters_are_the_current_ones() {
        let now = parse_counters(GRAVES, 104).unwrap();
        let blended = blend_counters(now.clone(), None, PatchInfo { previous: None, ..info() });
        assert_eq!(blended.rows, now.rows);
        assert_eq!(blended.patch.unwrap().previous, None);
    }

    #[test]
    fn the_patch_is_a_query_parameter_and_the_prior_cache_key_carries_it() {
        assert_eq!(
            counters_url("yone", Some("middle"), "emerald_plus", Some("16.19")),
            "https://lolalytics.com/lol/yone/counters/?lane=middle&tier=emerald_plus&patch=16.19"
        );
        let base = counters_cache_key("yone", Some("middle"), "emerald_plus");
        assert_ne!(patch::prior_key(&base, "16.19"), patch::prior_key(&base, "16.18"));
        assert_ne!(patch::prior_key(&base, "16.19"), base);
    }
}
