//! Swapper's own daily tier-list history. u.gg keeps no day-by-day numbers, so
//! once a day per lane and rank bracket the shown list is saved as a compact
//! snapshot, and a week-old one gives each champion's win-rate change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::tierlist::TierRow;

/// Snapshots kept per lane and bracket (one per day).
const MAX_SNAPSHOTS: usize = 10;
/// The comparison snapshot is the oldest one this many days old...
const MIN_AGE_DAYS: i64 = 3;
/// ...up to this many.
const MAX_AGE_DAYS: i64 = 8;
const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    #[serde(rename = "w")]
    win_pct: f64,
    #[serde(rename = "g")]
    games: u64,
    #[serde(rename = "t")]
    tier: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Snapshot {
    /// Days since the Unix epoch (UTC).
    day: i64,
    patch: String,
    rows: HashMap<i64, Entry>,
}

/// Records today's snapshot (unless `stale`) and returns each champion's win
/// rate minus its week-old one, for champions with at least `floor` games both
/// now and then. Empty without a usable same-patch snapshot.
pub(super) fn week_deltas(lane: &str, tier: &str, patch: &str, rows: &[TierRow], floor: u64, stale: bool) -> HashMap<i64, f64> {
    let Some(path) = history_path(lane, tier) else {
        return HashMap::new();
    };
    let today = chrono::Utc::now().timestamp().div_euclid(SECONDS_PER_DAY);
    let mut history = load(&path);
    if !stale && !rows.is_empty() && !history.iter().any(|snap| snap.day == today) {
        history = record(history, snapshot(today, patch, rows));
        if let Ok(bytes) = serde_json::to_vec(&history) {
            super::cache::write_atomic(&path, &bytes);
        }
    }
    deltas(&history, today, patch, rows, floor)
}

/// `tier-history/<lane>-<bracket>.json` under the data root. `None` for a
/// bracket that is not a plain slug, since it comes from the UI.
fn history_path(lane: &str, tier: &str) -> Option<PathBuf> {
    let slug = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if !slug(lane) || !slug(tier) {
        return None;
    }
    let root = crate::vault::data_root().ok()?;
    Some(root.join("tier-history").join(format!("{lane}-{}.json", tier.to_ascii_lowercase())))
}

/// A missing or corrupt file is an empty history.
fn load(path: &Path) -> Vec<Snapshot> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn snapshot(day: i64, patch: &str, rows: &[TierRow]) -> Snapshot {
    Snapshot {
        day,
        patch: patch.to_string(),
        rows: rows
            .iter()
            .map(|row| (row.champion_id, Entry { win_pct: row.win_pct, games: row.games, tier: row.tier }))
            .collect(),
    }
}

/// Adds `snap`, replacing a same-day one, and keeps the newest [`MAX_SNAPSHOTS`].
fn record(mut history: Vec<Snapshot>, snap: Snapshot) -> Vec<Snapshot> {
    history.retain(|old| old.day != snap.day);
    history.push(snap);
    history.sort_by_key(|old| old.day);
    let excess = history.len().saturating_sub(MAX_SNAPSHOTS);
    history.drain(..excess);
    history
}

/// The oldest same-patch snapshot [`MIN_AGE_DAYS`]..=[`MAX_AGE_DAYS`] old.
fn baseline<'a>(history: &'a [Snapshot], today: i64, patch: &str) -> Option<&'a Snapshot> {
    history
        .iter()
        .filter(|snap| (MIN_AGE_DAYS..=MAX_AGE_DAYS).contains(&(today - snap.day)) && snap.patch == patch)
        .min_by_key(|snap| snap.day)
}

fn deltas(history: &[Snapshot], today: i64, patch: &str, rows: &[TierRow], floor: u64) -> HashMap<i64, f64> {
    let Some(base) = baseline(history, today, patch) else {
        return HashMap::new();
    };
    rows.iter()
        .filter(|row| row.games >= floor)
        .filter_map(|row| {
            let then = base.rows.get(&row.champion_id).filter(|then| then.games >= floor)?;
            Some((row.champion_id, row.win_pct - then.win_pct))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, win_pct: f64, games: u64) -> TierRow {
        TierRow { champion_id: id, win_pct, games, ..TierRow::default() }
    }

    fn snap(day: i64, patch: &str, rows: &[TierRow]) -> Snapshot {
        snapshot(day, patch, rows)
    }

    #[test]
    fn history_keeps_one_snapshot_a_day_and_the_newest_ten() {
        let mut history = Vec::new();
        for day in 0..15 {
            history = record(history, snap(day, "16.22", &[row(1, 50.0, 900)]));
        }
        history = record(history, snap(14, "16.22", &[row(1, 51.0, 900)]));
        assert_eq!(history.len(), MAX_SNAPSHOTS);
        assert_eq!((history[0].day, history[9].day), (5, 14));
        assert_eq!(history[9].rows[&1].win_pct, 51.0);
    }

    #[test]
    fn the_baseline_is_the_oldest_same_patch_snapshot_three_to_eight_days_old() {
        let history = vec![snap(1, "16.22", &[]), snap(2, "16.21", &[]), snap(4, "16.22", &[]), snap(8, "16.22", &[])];
        // Day 10: day 2 (8 days) is the previous patch, so day 4 (6 days) is used.
        assert_eq!(baseline(&history, 10, "16.22").map(|s| s.day), Some(4));
        // Day 11: nothing between 3 and 8 days old on this patch but day 4 and day 8.
        assert_eq!(baseline(&history, 11, "16.22").map(|s| s.day), Some(4));
        // Under three days of data: nothing.
        assert_eq!(baseline(&[snap(8, "16.22", &[])], 10, "16.22"), None);
        assert_eq!(baseline(&history, 10, "16.23"), None);
    }

    #[test]
    fn deltas_need_enough_games_now_and_then() {
        let history = vec![snap(3, "16.22", &[row(1, 50.0, 900), row(2, 50.0, 100), row(3, 52.0, 900)])];
        let now = [row(1, 52.1, 900), row(2, 53.0, 900), row(3, 50.0, 100), row(4, 55.0, 900)];
        let found = deltas(&history, 10, "16.22", &now, 500);
        assert_eq!(found.len(), 1);
        assert!((found[&1] - 2.1).abs() < 1e-9);
    }

    #[test]
    fn corrupt_or_missing_files_are_an_empty_history_and_files_round_trip() {
        let dir = std::env::temp_dir().join(format!("swapper-tier-history-{}", uuid::Uuid::new_v4()));
        let path = dir.join("top-emerald_plus.json");
        assert!(load(&path).is_empty());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        assert!(load(&path).is_empty());
        let history = vec![snap(3, "16.22", &[row(1, 50.0, 900)])];
        super::super::cache::write_atomic(&path, &serde_json::to_vec(&history).unwrap());
        assert_eq!(load(&path), history);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn brackets_from_the_ui_must_be_plain_slugs() {
        assert!(history_path("top", "../evil").is_none());
        assert!(history_path("top", "").is_none());
    }
}
