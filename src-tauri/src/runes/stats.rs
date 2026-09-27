//! Per-rune statistics approximated from op.gg builds.
//!
//! op.gg does not report stats for individual runes, so Swapper sums the
//! `play`/`win` of every build that contains a rune. Win% is wins over plays of
//! those builds. Pick% is the rune's plays over the plays of its row, which is
//! why the caller supplies the rune's row membership.

use std::collections::HashMap;

use super::opgg::RunePageGroup;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RuneStat {
    pub play: u64,
    pub win: u64,
}

impl RuneStat {
    pub fn win_pct(&self) -> Option<f64> {
        (self.play > 0).then(|| (self.win as f64 / self.play as f64) * 100.0)
    }
}

/// Per-rune plays and wins across every build of every rune-page group.
#[derive(Debug, Default)]
pub struct Aggregate {
    pub per_rune: HashMap<i64, RuneStat>,
}

impl Aggregate {
    pub fn get(&self, id: i64) -> Option<RuneStat> {
        self.per_rune.get(&id).copied()
    }

    /// Pick% of a rune within the row that contains it. Runes that never
    /// appear in a build contribute no plays, so the row total is only the
    /// plays of the row's observed runes.
    pub fn pick_pct(&self, id: i64, row: &[i64]) -> Option<f64> {
        let row_play: u64 = row
            .iter()
            .filter_map(|candidate| self.per_rune.get(candidate))
            .map(|stat| stat.play)
            .sum();
        if row_play == 0 {
            return None;
        }
        let play = self.per_rune.get(&id)?.play;
        Some((play as f64 / row_play as f64) * 100.0)
    }
}

/// Sums plays and wins for every rune and stat shard used across the builds.
pub fn aggregate(groups: &[RunePageGroup]) -> Aggregate {
    let mut result = Aggregate::default();
    for group in groups {
        for build in &group.builds {
            let mut ids: Vec<i64> = Vec::new();
            ids.extend(build.primary_rune_ids.iter().copied());
            ids.extend(build.secondary_rune_ids.iter().copied());
            ids.extend(build.stat_mod_ids.iter().copied());
            for id in ids {
                let entry = result.per_rune.entry(id).or_default();
                entry.play += build.play;
                entry.win += build.win;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runes::opgg;

    const FIXTURE: &str = include_str!("../../tests/fixtures/opgg-ahri-mid-ranked.json");

    #[test]
    fn sums_plays_and_wins_for_every_rune_in_a_build() {
        let groups = opgg::parse(FIXTURE).unwrap();
        let stats = aggregate(&groups);
        // Electrocute appears in the first three builds.
        let electrocute = stats.get(8112).unwrap();
        assert_eq!(electrocute.play, 8058 + 962 + 442);
        assert_eq!(electrocute.win, 4083 + 474 + 230);
        // A secondary rune shared across different groups still accumulates.
        let manaflow = stats.get(8210).unwrap();
        assert_eq!(manaflow.play, 8058 + 985 + 640);
        // A stat shard is aggregated too (present in the first, fourth and fifth builds).
        assert_eq!(stats.get(5001).unwrap().play, 8058 + 985 + 640);
    }

    #[test]
    fn win_pct_is_none_without_plays() {
        assert_eq!(RuneStat::default().win_pct(), None);
        assert_eq!(RuneStat { play: 10, win: 4 }.win_pct(), Some(40.0));
    }

    #[test]
    fn pick_pct_is_relative_to_the_runes_row() {
        let groups = opgg::parse(FIXTURE).unwrap();
        let stats = aggregate(&groups);
        // Keystone row (Domination): 8112 and 8992 only.
        let keystones = [8112, 8992];
        let pct = stats.pick_pct(8112, &keystones).unwrap();
        assert!((pct - (9462.0 / 10447.0) * 100.0).abs() < 0.01);
        assert!(stats.pick_pct(50099, &keystones).is_none());
    }
}
