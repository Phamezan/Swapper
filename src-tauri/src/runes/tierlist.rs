//! The lolalytics tier list for one lane and rank bracket.
//!
//! The Qwik state holds one dictionary keyed by champion id (the same ids as
//! the page's `champId` slug map, checked for all 173 champions of a real page)
//! whose values are row objects `{rank, tier, wr, pr, br, games, ...}`. A row
//! with `rank` 0 is not ranked in this lane.
//!
//! lolalytics' own `rank`/`tier` are ignored: it ranks mostly by win rate, while
//! this list wants "strong and popular". Each kept row is scored from its win
//! rate — shrunk toward the lane average so a low-sample situational pick cannot
//! top the list — blended with its pick rate, then sorted best-first and labelled
//! with our own `S+`..`D` tiers. Ban rate is deliberately unused: it is
//! champion-wide (all roles) and inflates a champion in a lane it is not banned
//! for.

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
/// Games of shrinkage weight pulling each win rate toward the lane average, so
/// a low-sample (situational/counterpick) win rate cannot top the list. One of
/// the calibration knobs.
pub const PRIOR_GAMES: f64 = 3000.0;
/// Win-rate weight in the blended score. One of the calibration knobs.
pub const WR_WEIGHT: f64 = 0.55;
/// Pick-rate weight in the blended score. One of the calibration knobs.
pub const POP_WEIGHT: f64 = 0.45;
/// Score floor for each tier, best first. A score at or above the first floor is
/// `S+`; below every floor is `D`. Calibrated against the live emerald_plus
/// middle list (97 kept rows): S+ is the top few so 1.5 would leave it empty,
/// and -0.45 splits the large middle more evenly between C and D.
const TIER_THRESHOLDS: [(&str, f64); 5] =
    [("S+", 1.3), ("S", 1.0), ("A", 0.5), ("B", 0.0), ("C", -0.45)];
/// Our tier labels, indexed by the numeric `tier`.
const TIER_LABELS: [&str; 6] = ["S+", "S", "A", "B", "C", "D"];
const LAST_GOOD_MAX_ENTRIES: usize = 32;
const LAST_GOOD_MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TierRow {
    pub champion_id: i64,
    /// Our tier index into [`TIER_LABELS`] (0 = S+, 5 = D).
    pub tier: u8,
    pub win_pct: f64,
    pub pick_pct: f64,
    pub ban_pct: f64,
    pub games: u64,
    /// The blended score; higher is better. Sorts the list. Missing from lists
    /// cached before it existed.
    #[serde(default)]
    pub score: f64,
}

impl TierRow {
    /// The label for this row's tier.
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

/// `v2`: rows carry our blended score and six-label tiers; v1 entries persisted
/// on disk hold lolalytics' 1-15 tiers and must not be served.
pub fn tierlist_cache_key(lane: &str, tier: &str) -> String {
    format!("tierlist-v2|{lane}|{}", lolalytics::tier_slug(tier))
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
    score_rows(&mut rows);
    Some(TierList { rows })
}

/// Scores every kept row from its win rate (shrunk toward the lane average) and
/// its pick rate, sorts best-first and labels each with its tier. Fewer than
/// two rows cannot be normalized, so they keep a zero score and their labels
/// come from that.
fn score_rows(rows: &mut [TierRow]) {
    if rows.len() < 2 {
        for row in rows.iter_mut() {
            row.score = 0.0;
            row.tier = tier_index(0.0);
        }
        return;
    }
    let total_games: f64 = rows.iter().map(|row| row.games as f64).sum();
    let lane_avg = if total_games > 0.0 {
        rows.iter().map(|row| row.win_pct * row.games as f64).sum::<f64>() / total_games
    } else {
        rows.iter().map(|row| row.win_pct).sum::<f64>() / rows.len() as f64
    };
    let swr: Vec<f64> = rows
        .iter()
        .map(|row| {
            let games = row.games as f64;
            (row.win_pct * games + lane_avg * PRIOR_GAMES) / (games + PRIOR_GAMES)
        })
        .collect();
    // Pick rate is a percentage; a non-positive one would make `ln` undefined,
    // so clamp to 0.01% (a z-scored outlier with no meaningful effect).
    let pres: Vec<f64> = rows.iter().map(|row| row.pick_pct.max(0.01).ln()).collect();
    let z_wr = zscores(&swr);
    let z_pr = zscores(&pres);
    for (index, row) in rows.iter_mut().enumerate() {
        row.score = WR_WEIGHT * z_wr[index] + POP_WEIGHT * z_pr[index];
    }
    rows.sort_by(|left, right| right.score.total_cmp(&left.score));
    for row in rows.iter_mut() {
        row.tier = tier_index(row.score);
    }
}

/// Population z-scores. All zeros when the spread is zero, so a flat list does
/// not produce NaN.
fn zscores(values: &[f64]) -> Vec<f64> {
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let variance = values.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / count;
    let stdev = variance.sqrt();
    if stdev <= 0.0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|value| (value - mean) / stdev).collect()
}

/// The tier index for a score: the first label whose floor it reaches, or the
/// last label (`D`) below every floor.
fn tier_index(score: f64) -> u8 {
    TIER_THRESHOLDS
        .iter()
        .position(|(_, floor)| score >= *floor)
        .unwrap_or(TIER_LABELS.len() - 1) as u8
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
    let games = row.get("games").and_then(as_u64)?;
    if champion_id <= 0 || rank == 0 || games < min_games {
        return None;
    }
    Some(TierRow {
        champion_id,
        tier: 0,
        win_pct: row.get("wr").and_then(number)?,
        pick_pct: row.get("pr").and_then(number)?,
        ban_pct: row.get("br").and_then(number)?,
        games,
        score: 0.0,
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

    /// The production parser for the default games floor.
    fn parse_tierlist(html: &str) -> Option<TierList> {
        parse_tierlist_with_min(html, MIN_TIER_GAMES)
    }

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
    /// Cho'Gath (31), Master Yi (11) and Wukong (62), plus rows to be dropped.
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
    fn scores_and_labels_the_kept_rows_best_first() {
        let list = parse_tierlist(&page()).expect("the page should parse");
        let ids: Vec<i64> = list.rows.iter().map(|row| row.champion_id).collect();
        // The blend puts popular, strong Wukong first, then Cho'Gath (higher win
        // rate but lower pick rate) and Master Yi last.
        assert_eq!(ids, vec![62, 31, 11]);
        assert!(list.rows[0].score > list.rows[1].score);
        assert!(list.rows[1].score > list.rows[2].score);
        assert_eq!(list.rows[0].label(), "S");
        // A three-row list spreads the scores wide, so the tail lands in C/D.
        assert!(["A", "B", "C", "D"].contains(&list.rows[1].label()));
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
    fn score_thresholds_map_to_the_six_tier_labels() {
        let label = |tier| TierRow { tier, ..TierRow::default() }.label();
        assert_eq!(
            (label(0), label(1), label(2), label(3), label(4), label(5), label(99)),
            ("S+", "S", "A", "B", "C", "D", "")
        );
        assert_eq!(
            (
                tier_index(2.0),
                tier_index(1.3),
                tier_index(1.0),
                tier_index(0.5),
                tier_index(0.0),
                tier_index(-0.45),
                tier_index(-1.0),
            ),
            (0, 0, 1, 2, 3, 4, 5)
        );
    }

    #[test]
    fn a_strong_low_sample_champion_ranks_below_a_popular_one_and_ban_rate_is_ignored() {
        let row = |id, win_pct, pick_pct, ban_pct, games| TierRow {
            champion_id: id,
            win_pct,
            pick_pct,
            ban_pct,
            games,
            ..TierRow::default()
        };
        // L1 has the highest win rate on a small sample; P1 is slightly lower
        // but far more popular, and P2/P3 are average filler.
        let mut rows = vec![
            row(1, 56.0, 0.5, 30.0, 600),
            row(2, 54.0, 8.0, 0.5, 60_000),
            row(3, 51.0, 6.0, 0.5, 40_000),
            row(4, 50.0, 5.0, 0.5, 30_000),
        ];
        score_rows(&mut rows);
        let ids: Vec<i64> = rows.iter().map(|row| row.champion_id).collect();
        // (a) the popular champ outranks the strong-but-rare one.
        assert!(ids.iter().position(|id| *id == 2) < ids.iter().position(|id| *id == 1));
        // (c) every row gets one of our labels, ordered best-first.
        let labels: Vec<&str> = rows.iter().map(TierRow::label).collect();
        assert!(labels.iter().all(|label| ["S+", "S", "A", "B", "C", "D"].contains(label)));
        assert!(rows.windows(2).all(|pair| pair[0].tier <= pair[1].tier));

        // (b) ban rate is not an input: flipping every ban rate leaves the
        // scores and order untouched.
        let before: Vec<(i64, f64)> = rows.iter().map(|row| (row.champion_id, row.score)).collect();
        let mut flipped = vec![
            row(1, 56.0, 0.5, 0.5, 600),
            row(2, 54.0, 8.0, 30.0, 60_000),
            row(3, 51.0, 6.0, 30.0, 40_000),
            row(4, 50.0, 5.0, 30.0, 30_000),
        ];
        score_rows(&mut flipped);
        let after: Vec<(i64, f64)> = flipped.iter().map(|row| (row.champion_id, row.score)).collect();
        assert_eq!(before, after);
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

    /// Calibrates the thresholds against the real list. Run with
    /// `cargo test --lib -- --ignored --nocapture prints_the_live`.
    #[tokio::test]
    #[ignore = "network: fetches the live emerald_plus middle tier list"]
    async fn prints_the_live_emerald_plus_middle_scoring() {
        let client = LolalyticsClient::new().expect("a client");
        let list = client
            .tierlist("middle", "emerald_plus")
            .await
            .expect("a fetch")
            .expect("a tier list");
        println!("kept rows: {}", list.rows.len());
        for (rank, row) in list.rows.iter().take(20).enumerate() {
            println!(
                "#{:<2} id={:<4} score={:>6.3} tier={:<2} wr={:>5.2} pr={:>5.2} games={}",
                rank + 1,
                row.champion_id,
                row.score,
                row.label(),
                row.win_pct,
                row.pick_pct,
                row.games
            );
        }
        let mut counts = std::collections::BTreeMap::new();
        for row in &list.rows {
            *counts.entry(row.label()).or_insert(0usize) += 1;
        }
        println!("tier counts: {counts:?}");
    }
}
