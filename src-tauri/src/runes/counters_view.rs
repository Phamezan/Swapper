//! View model for the Champion tab: who beats a champion and who it beats,
//! shared by the desktop flyout and the phone remote.

use std::collections::HashMap;

use super::counters::{self, CounterRow, Counters};
use super::data;
use super::lolalytics;
use super::matchup::MIN_MATCHUP_GAMES;
use super::provider::Sourced;

/// Cards per strip.
const LIST_LEN: usize = 15;

/// One opposing champion, from the searched champion's point of view.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterRowView {
    pub champion_id: i64,
    pub name: String,
    /// The searched champion's win rate into this opponent.
    pub win_pct: f64,
    /// `win_pct` minus the opponent's overall win rate.
    pub delta: f64,
    pub games: u64,
    /// Under [`MIN_MATCHUP_GAMES`]: shown greyed out, below the rest.
    pub low_sample: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionCountersView {
    pub champion_id: i64,
    pub champion_name: String,
    /// The Swapper role the table is for ("mid", "adc"), or empty if unknown.
    pub position: String,
    /// Toughest matchups: the champion's lowest win rate into each opponent first.
    pub best_into: Vec<CounterRowView>,
    /// Easiest matchups: the champion's highest win rate into each opponent first.
    pub beats: Vec<CounterRowView>,
    /// True when no table could be loaded.
    pub unavailable: bool,
    pub stale: bool,
    pub updated_at: Option<i64>,
}

/// One champion for the search box.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionOption {
    pub id: i64,
    pub name: String,
}

/// Champion ids at or above this belong to other game modes: the client lists
/// duplicates such as "Annie" as 60001, while real champions end near 950.
const MAX_CHAMPION_ID: i64 = 10_000;

fn is_selectable_champion(id: i64, name: &str) -> bool {
    (1..MAX_CHAMPION_ID).contains(&id) && !name.trim().is_empty()
}

/// Every champion by name, for the Champion tab search box.
pub async fn champion_list() -> Vec<ChampionOption> {
    let mut list: Vec<ChampionOption> = data::champion_names()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|(id, name)| is_selectable_champion(*id, name))
        .map(|(id, name)| ChampionOption { id, name })
        .collect();
    list.sort_by(|left, right| left.name.cmp(&right.name));
    list
}

/// The matchup table for a champion. `position` is a Swapper role; without a
/// valid one lolalytics picks the champion's usual lane.
pub async fn champion_counters_view(
    champion_id: i64,
    position: Option<&str>,
    tier: &str,
) -> ChampionCountersView {
    let names = data::champion_names().await.unwrap_or_default();
    let champion_name = names.get(&champion_id).cloned().unwrap_or_default();
    let lane = position.and_then(lolalytics::lane);
    let found = match lolalytics::champion_slug(&champion_name) {
        Some(slug) if champion_id > 0 => counters::load(&slug, champion_id, lane, tier)
            .await
            .ok()
            .flatten(),
        _ => None,
    };
    build_view(champion_id, champion_name, found, &names)
}

pub(super) fn position_of(lane: &str) -> &'static str {
    match lane {
        "top" => "top",
        "jungle" => "jungle",
        "middle" => "mid",
        "bottom" => "adc",
        "support" => "support",
        _ => "",
    }
}

fn build_view(
    champion_id: i64,
    champion_name: String,
    found: Option<Sourced<Counters>>,
    names: &HashMap<i64, String>,
) -> ChampionCountersView {
    let Some(sourced) = found else {
        return ChampionCountersView {
            champion_id,
            champion_name,
            position: String::new(),
            best_into: Vec::new(),
            beats: Vec::new(),
            unavailable: true,
            stale: false,
            updated_at: None,
        };
    };
    let rows: Vec<CounterRowView> = sourced
        .value
        .rows
        .iter()
        .map(|row| row_view(row, names))
        .collect();
    let mut best_into = ranked(&rows, |left, right| left.win_pct.total_cmp(&right.win_pct));
    best_into.truncate(LIST_LEN);
    let mut beats = ranked(&rows, |left, right| right.win_pct.total_cmp(&left.win_pct));
    beats.retain(|row| !best_into.iter().any(|other| other.champion_id == row.champion_id));
    beats.truncate(LIST_LEN);
    ChampionCountersView {
        champion_id,
        champion_name,
        position: position_of(&sourced.value.lane).to_string(),
        best_into,
        beats,
        unavailable: false,
        stale: sourced.stale,
        updated_at: sourced.fetched_at,
    }
}

fn row_view(row: &CounterRow, names: &HashMap<i64, String>) -> CounterRowView {
    CounterRowView {
        champion_id: row.champion_id,
        name: names
            .get(&row.champion_id)
            .cloned()
            .unwrap_or_else(|| format!("Champion {}", row.champion_id)),
        win_pct: row.vs_win_pct,
        delta: row.vs_win_pct - row.all_win_pct,
        games: row.games,
        low_sample: row.games < MIN_MATCHUP_GAMES,
    }
}

/// Rows ordered by `order`, with low-sample rows after all the others.
fn ranked(
    rows: &[CounterRowView],
    order: impl Fn(&CounterRowView, &CounterRowView) -> std::cmp::Ordering,
) -> Vec<CounterRowView> {
    let mut sorted = rows.to_vec();
    sorted.sort_by(|left, right| {
        left.low_sample
            .cmp(&right.low_sample)
            .then_with(|| order(left, right))
    });
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runes::provider::Provider;

    fn row(id: i64, vs: f64, games: u64) -> CounterRow {
        CounterRow {
            champion_id: id,
            vs_win_pct: vs,
            games,
            all_win_pct: 50.0,
        }
    }

    fn view(rows: Vec<CounterRow>) -> ChampionCountersView {
        let counters = Counters {
            lane: "middle".into(),
            rows,
        };
        build_view(
            1,
            "Annie".into(),
            Some(Sourced::fresh(counters, Provider::Lolalytics)),
            &HashMap::new(),
        )
    }

    #[test]
    fn best_into_is_lowest_win_rate_first_with_the_enemy_perspective() {
        let view = view(vec![row(2, 55.0, 500), row(3, 45.0, 500), row(4, 48.0, 500)]);
        assert_eq!(view.position, "mid");
        let ids: Vec<i64> = view.best_into.iter().map(|r| r.champion_id).collect();
        assert_eq!(ids, vec![3, 4, 2]);
        let top = &view.best_into[0];
        assert!((top.win_pct - 45.0).abs() < 1e-9);
        assert!((top.delta - -5.0).abs() < 1e-9);
    }

    #[test]
    fn beats_is_highest_win_rate_first_and_not_repeated() {
        let rows: Vec<CounterRow> = (2..=25).map(|id| row(id, 40.0 + id as f64, 500)).collect();
        let view = view(rows);
        assert_eq!(view.beats[0].champion_id, 25);
        assert_eq!(view.best_into.len(), LIST_LEN);
        assert!(view
            .beats
            .iter()
            .all(|b| view.best_into.iter().all(|c| c.champion_id != b.champion_id)));
    }

    #[test]
    fn low_samples_are_flagged_and_sorted_below_the_rest() {
        let view = view(vec![row(2, 30.0, 99), row(3, 45.0, 100), row(4, 49.0, 5000)]);
        let ids: Vec<i64> = view.best_into.iter().map(|r| r.champion_id).collect();
        assert_eq!(ids, vec![3, 4, 2]);
        assert!(view.best_into[2].low_sample);
        assert!(!view.best_into[0].low_sample);
    }

    #[test]
    fn a_missing_table_is_unavailable() {
        let view = build_view(1, "Annie".into(), None, &HashMap::new());
        assert!(view.unavailable && view.best_into.is_empty() && view.beats.is_empty());
    }

    #[test]
    fn champion_search_skips_other_mode_duplicates() {
        assert!(is_selectable_champion(103, "Ahri"));
        assert!(is_selectable_champion(950, "Naafiri"));
        assert!(!is_selectable_champion(60103, "Ahri"));
        assert!(!is_selectable_champion(-1, "None"));
        assert!(!is_selectable_champion(5, " "));
    }
}
