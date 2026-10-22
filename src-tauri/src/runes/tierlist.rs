//! A lane tier list scored from u.gg stats, with lolalytics as fallback.
//!
//! The fallback's Qwik state holds one dictionary keyed by champion id (the same ids as
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
use super::patch::{self, blend_rate, PatchInfo};
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
/// Pick-rate weight in the blended score; win rate gets the rest. One of the
/// calibration knobs.
pub const POP_WEIGHT: f64 = 0.45;
/// u.gg tier lists exclude lanes accounting for less than 5% of a champion's games.
pub const MIN_ROLE_SHARE: f64 = 0.05;
/// Our tier labels, indexed by the numeric `tier`.
const TIER_LABELS: [&str; 6] = ["S+", "S", "A", "B", "C", "D"];
/// Cumulative percentages for S+, S, A, B and C; remaining rows are D.
/// Boundaries round up, and tied scores share their first row's better tier.
const TIER_CUMULATIVE_PERCENT: [usize; 5] = [3, 10, 30, 60, 85];
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
        tier_label(self.tier)
    }
}

/// The label for a tier index (0 = S+, 5 = D).
pub fn tier_label(tier: u8) -> &'static str {
    TIER_LABELS.get(tier as usize).copied().unwrap_or("")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TierList {
    /// Rows ordered by blended score, best first.
    pub rows: Vec<TierRow>,
    /// Which patch this is, and whether the previous one was blended in.
    #[serde(default)]
    pub patch: Option<PatchInfo>,
}

/// Blends the previous patch's rows into the current ones (win, pick and ban
/// rates per [`blend_rate`]; games stay the current patch's), keeps the rows
/// that clear the games floor on either patch, then scores and labels them.
/// A row only the previous patch has comes in with 0 games. Both inputs are
/// unfiltered, unscored page rows.
pub fn blend_tierlist(now: TierList, prev: Option<&TierList>, tier: &str, info: PatchInfo) -> TierList {
    let floor = min_games(tier);
    let mut rows: Vec<TierRow> = now
        .rows
        .iter()
        .filter_map(|row| {
            let other = prev.and_then(|prev| prev.rows.iter().find(|o| o.champion_id == row.champion_id));
            match other {
                Some(other) if row.games >= floor || other.games >= floor => Some(TierRow {
                    win_pct: blend_rate(row.win_pct, row.games, other.win_pct),
                    pick_pct: blend_rate(row.pick_pct, row.games, other.pick_pct),
                    ban_pct: blend_rate(row.ban_pct, row.games, other.ban_pct),
                    ..row.clone()
                }),
                Some(_) => None,
                None => (row.games >= floor).then(|| row.clone()),
            }
        })
        .collect();
    if let Some(prev) = prev {
        rows.extend(
            prev.rows
                .iter()
                .filter(|other| other.games >= floor)
                .filter(|other| !now.rows.iter().any(|row| row.champion_id == other.champion_id))
                .map(|other| TierRow { games: 0, ..other.clone() }),
        );
    }
    score_rows(&mut rows);
    TierList { rows, patch: Some(info) }
}

pub fn tierlist_url(lane: &str, tier: &str, patch: Option<&str>) -> String {
    lolalytics::patched(
        format!(
            "{}/tierlist/?lane={lane}&tier={}",
            lolalytics::BASE_URL,
            lolalytics::tier_slug(tier)
        ),
        patch,
    )
}

/// `v2`: rows carry our blended score and six-label tiers; v1 entries persisted
/// on disk hold lolalytics' 1-15 tiers and must not be served.
pub fn tierlist_cache_key(lane: &str, tier: &str) -> String {
    format!("tierlist-v2|{lane}|{}", lolalytics::tier_slug(tier))
}

/// Parses a tier list page, dropping rows below `min_games`. `None` when no
/// champion dictionary is found.
#[cfg(test)]
fn parse_tierlist_with_min(html: &str, min_games: u64) -> Option<TierList> {
    let mut rows = parse_rows(html, min_games)?;
    score_rows(&mut rows);
    Some(TierList { rows, patch: None })
}

/// The unscored rows of a tier list page with at least `min_games` games.
pub(super) fn parse_rows(html: &str, min_games: u64) -> Option<Vec<TierRow>> {
    let state: Qwik = serde_json::from_str(lolalytics::qwik_json(html)?).ok()?;
    let objs = &state.objs;
    let table = objs.iter().find(|entry| is_row_table(entry, objs))?;
    Some(
        table
            .as_object()?
            .iter()
            .filter_map(|(key, row)| tier_row(key.parse().ok()?, &resolve(row, objs, 0), min_games))
            .collect(),
    )
}

/// Scores every kept row from its win rate (shrunk toward the lane average) and
/// its pick rate, sorts best-first and labels each with its tier.
fn score_rows(rows: &mut [TierRow]) {
    let stats: Vec<(f64, f64, u64)> = rows.iter().map(|row| (row.win_pct, row.pick_pct, row.games)).collect();
    for (row, score) in rows.iter_mut().zip(scores(&stats, POP_WEIGHT)) {
        row.score = score;
    }
    rows.sort_by(|left, right| right.score.total_cmp(&left.score));
    label_rows(rows);
}

/// Also relabel persisted results from before percentile tiers without
/// changing their already sorted order, scores, statistics or freshness.
fn label_rows(rows: &mut [TierRow]) {
    let tiers = percentile_tiers(rows.iter().map(|row| row.score));
    for (row, tier) in rows.iter_mut().zip(tiers) {
        row.tier = tier;
    }
}

/// One score per `(win %, pick %, games)`: the win rate shrunk toward the
/// game-weighted average, z-scored and blended with the z-scored log pick
/// rate (`pop_weight`, the rest goes to win rate). Fewer than two rows cannot
/// be normalized, so they all score zero.
pub(super) fn scores(rows: &[(f64, f64, u64)], pop_weight: f64) -> Vec<f64> {
    if rows.len() < 2 {
        return vec![0.0; rows.len()];
    }
    let total_games: f64 = rows.iter().map(|row| row.2 as f64).sum();
    let lane_avg = if total_games > 0.0 {
        rows.iter().map(|row| row.0 * row.2 as f64).sum::<f64>() / total_games
    } else {
        rows.iter().map(|row| row.0).sum::<f64>() / rows.len() as f64
    };
    let swr: Vec<f64> = rows
        .iter()
        .map(|&(win, _, games)| {
            let games = games as f64;
            (win * games + lane_avg * PRIOR_GAMES) / (games + PRIOR_GAMES)
        })
        .collect();
    // Pick rate is a percentage; a non-positive one would make `ln` undefined,
    // so clamp to 0.01% (a z-scored outlier with no meaningful effect).
    let pres: Vec<f64> = rows.iter().map(|row| row.1.max(0.01).ln()).collect();
    zscores(&swr)
        .into_iter()
        .zip(zscores(&pres))
        .map(|(wr, pr)| (1.0 - pop_weight) * wr + pop_weight * pr)
        .collect()
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

/// Tiers for scores already sorted best-first. A tied run uses its first
/// position, so ties crossing a percentile boundary keep the better tier.
pub(super) fn percentile_tiers(scores: impl ExactSizeIterator<Item = f64>) -> Vec<u8> {
    let n = scores.len();
    let mut first = 0;
    let mut previous = None;
    scores
        .enumerate()
        .map(|(index, score)| {
            if previous != Some(score) {
                first = index;
                previous = Some(score);
            }
            TIER_CUMULATIVE_PERCENT
                .iter()
                .position(|percent| first < (n * percent).div_ceil(100))
                .unwrap_or(TIER_LABELS.len() - 1) as u8
        })
        .collect()
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
    /// Fetches and parses one lane's tier list: every ranked row, unfiltered and
    /// unscored, so [`blend_tierlist`] can apply the games floor after blending.
    pub async fn tierlist(
        &self,
        lane: &str,
        tier: &str,
        patch: Option<&str>,
    ) -> Result<Option<TierList>, ProviderError> {
        let body = self.page(tierlist_url(lane, tier, patch)).await?;
        let rows = lolalytics::checked(&body, "tier list page", true, parse_rows(&body, 0));
        Ok(rows.map(|rows| TierList { rows, patch: None }))
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
    let mut found = super::ugg::prefer(super::ugg::tier_list(lane, tier).await, load_lolalytics(lane, tier)).await?;
    if let Some(found) = found.as_mut() {
        label_rows(&mut found.value.rows);
    }
    Ok(found)
}

pub(super) async fn load_lolalytics(lane: &str, tier: &str) -> Result<Option<Sourced<TierList>>, RuneError> {
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
    match client.tierlist(lane, tier, None).await {
        Ok(Some(found)) => {
            let found = patch::blended(
                found,
                &key,
                last_good(),
                |previous| {
                    let client = &client;
                    async move { client.tierlist(lane, tier, Some(&previous)).await }
                },
                |now, prev, info| blend_tierlist(now, prev, tier, info),
            )
            .await;
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
        assert_eq!(list.rows.iter().map(TierRow::label).collect::<Vec<_>>(),vec!["S+","B","C"]);
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
    fn percentile_boundaries_map_to_the_six_tier_labels() {
        let label = |tier| TierRow { tier, ..TierRow::default() }.label();
        assert_eq!(
            (label(0), label(1), label(2), label(3), label(4), label(5), label(99)),
            ("S+", "S", "A", "B", "C", "D", "")
        );
        for (n,expected) in [(87,[3,6,18,26,21,13]),(45,[2,3,9,13,12,6])] {
            let mut counts = [0usize;6];
            for tier in percentile_tiers((0..n).map(|i|(n-i) as f64)) { counts[tier as usize] += 1; }
            assert_eq!(counts,expected,"n={n}");
        }
    }

    #[test]
    fn equal_scores_share_the_better_percentile_tier() {
        let mut scores: Vec<f64> = (0..100).map(|i|(100-i) as f64).collect();
        for boundary in [3,10,30,60,85] { scores[boundary] = scores[boundary-1]; }
        let tiers = percentile_tiers(scores.into_iter());
        for (boundary,expected) in [(3,0),(10,1),(30,2),(60,3),(85,4)] {
            assert_eq!(tiers[boundary],expected);
            assert_eq!(tiers[boundary],tiers[boundary-1]);
        }
        assert_eq!(percentile_tiers(std::iter::repeat_n(0.0,87)),vec![0;87]);
    }

    #[test]
    fn empty_and_single_row_lists_are_safe() {
        assert!(percentile_tiers(std::iter::empty()).is_empty());
        assert_eq!(percentile_tiers(std::iter::once(0.0)),vec![0]);
        assert_eq!(percentile_tiers(std::iter::once(-10.0)),vec![0]);
    }

    #[test]
    fn relabeling_cached_rows_keeps_scores_order_and_stats() {
        let mut rows = vec![raw(3,53.0,5.0,1000),raw(2,52.0,4.0,2000),raw(1,51.0,3.0,3000)];
        for (row,score) in rows.iter_mut().zip([2.0,1.0,-10.0]) {
            row.score = score;
            row.tier = 5;
        }
        let before = rows.clone();
        label_rows(&mut rows);
        assert_eq!(rows.iter().map(TierRow::label).collect::<Vec<_>>(),vec!["S+","B","C"]);
        for (mut old,new) in before.into_iter().zip(rows) {
            old.tier = new.tier;
            assert_eq!(old,new);
        }
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
            tierlist_url("jungle", "emerald_plus", None),
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
            .tierlist("middle", "emerald_plus", None)
            .await
            .expect("a fetch")
            .expect("a tier list");
        let list = blend_tierlist(list, None, "emerald_plus", PatchInfo::default());
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

    fn raw(id: i64, win: f64, pick: f64, games: u64) -> TierRow {
        TierRow { champion_id: id, tier: 0, win_pct: win, pick_pct: pick, ban_pct: 1.0, games, score: 0.0 }
    }

    fn info() -> PatchInfo {
        PatchInfo { patch: "16.20".into(), previous: Some("16.19".into()) }
    }

    #[test]
    fn day_zero_tier_list_is_the_previous_patch_with_zero_current_games() {
        let now = TierList { rows: (1..=4).map(|id| raw(id, 50.0, 5.0, 0)).collect(), patch: None };
        let prev = TierList {
            rows: vec![raw(1, 54.0, 9.0, 30_000), raw(2, 50.0, 5.0, 30_000), raw(3, 47.0, 2.0, 30_000), raw(4, 52.0, 6.0, 30_000)],
            patch: None,
        };
        let list = blend_tierlist(now, Some(&prev), "emerald_plus", info());
        assert_eq!(list.rows.len(), 4, "rows clear the floor on the previous patch");
        assert_eq!(list.rows[0].champion_id, 1);
        assert!((list.rows[0].win_pct - 54.0).abs() < 1e-9);
        assert!(list.rows.iter().all(|row| row.games == 0));
    }

    #[test]
    fn without_a_previous_patch_the_floor_applies_to_current_games_only() {
        let now = TierList { rows: (1..=3).map(|id| raw(id, 50.0, 5.0, if id == 1 { 100 } else { 900 })).collect(), patch: None };
        let list = blend_tierlist(now, None, "emerald_plus", PatchInfo { previous: None, ..info() });
        assert_eq!(list.rows.len(), 2);
        assert!(list.patch.unwrap().previous.is_none());
    }

    #[test]
    fn a_large_current_tier_sample_dominates_the_blended_rate() {
        let now = TierList { rows: vec![raw(1, 51.0, 5.0, 20_000), raw(2, 50.0, 5.0, 20_000)], patch: None };
        let prev = TierList { rows: vec![raw(1, 55.0, 5.0, 20_000)], patch: None };
        let list = blend_tierlist(now, Some(&prev), "emerald_plus", info());
        let row = list.rows.iter().find(|row| row.champion_id == 1).unwrap();
        assert!((row.win_pct - 51.0).abs() < 0.5);
    }
}
