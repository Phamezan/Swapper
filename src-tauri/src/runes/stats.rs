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
    /// The current patch's plays and wins, which are what is shown.
    pub play: u64,
    pub win: u64,
    /// The patch-blended plays and wins (equal to the above on a plain page);
    /// they set the win rate and the pick share.
    pub weight: f64,
    pub weighted_win: f64,
}

impl RuneStat {
    pub fn win_pct(&self) -> Option<f64> {
        if self.weight > 0.0 {
            return Some(self.weighted_win / self.weight * 100.0);
        }
        (self.play > 0).then(|| (self.win as f64 / self.play as f64) * 100.0)
    }
}

/// Per-rune plays and wins across every build of every rune-page group.
#[derive(Debug, Default)]
pub struct Aggregate {
    pub per_rune: HashMap<i64, RuneStat>,
    /// Stat shards per row (by position in the build's shard array), because
    /// the same shard id sits in two rows and would be counted twice by id.
    pub per_shard_row: Vec<HashMap<i64, RuneStat>>,
}

impl Aggregate {
    pub fn get(&self, id: i64) -> Option<RuneStat> {
        self.per_rune.get(&id).copied()
    }

    /// The stats of one shard row (0 Offense, 1 Flex, 2 Defense), usable with
    /// [`Aggregate::get`] and [`Aggregate::pick_pct`].
    pub fn shard_row(&self, row: usize) -> Aggregate {
        Aggregate {
            per_rune: self.per_shard_row.get(row).cloned().unwrap_or_default(),
            per_shard_row: Vec::new(),
        }
    }

    /// Pick% of a rune within the row that contains it. Runes that never
    /// appear in a build contribute no plays, so the row total is only the
    /// plays of the row's observed runes.
    pub fn pick_pct(&self, id: i64, row: &[i64]) -> Option<f64> {
        let row_weight: f64 = row
            .iter()
            .filter_map(|candidate| self.per_rune.get(candidate))
            .map(|stat| stat.weight)
            .sum();
        if row_weight <= 0.0 {
            return None;
        }
        let weight = self.per_rune.get(&id)?.weight;
        Some((weight / row_weight) * 100.0)
    }
}

/// Sums plays and wins for every rune and stat shard used across the builds.
pub fn aggregate(groups: &[RunePageGroup]) -> Aggregate {
    let mut result = Aggregate::default();
    for group in groups {
        for build in &group.builds {
            let add = |entry: &mut RuneStat| {
                entry.play += build.play;
                entry.win += build.win;
                entry.weight += build.weight();
                entry.weighted_win += build.weight() * build.win_pct().unwrap_or(0.0) / 100.0;
            };
            for id in build.primary_rune_ids.iter().chain(&build.secondary_rune_ids) {
                add(result.per_rune.entry(*id).or_default());
            }
            if result.per_shard_row.len() < build.stat_mod_ids.len() {
                result.per_shard_row.resize_with(build.stat_mod_ids.len(), HashMap::new);
            }
            for (row, id) in build.stat_mod_ids.iter().enumerate() {
                add(result.per_shard_row[row].entry(*id).or_default());
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
        assert_eq!(stats.shard_row(2).get(5001).unwrap().play, 8058 + 985 + 640);
    }

    #[test]
    fn a_shard_id_repeated_across_rows_is_counted_once_per_row() {
        let group: RunePageGroup = serde_json::from_str(
            r#"{"primary_page_id":8100,"secondary_page_id":8200,"builds":[
                {"primary_page_id":8100,"primary_rune_ids":[],"secondary_page_id":8200,
                 "secondary_rune_ids":[],"stat_mod_ids":[5008,5008,5001],"play":217,"win":110}]}"#,
        )
        .unwrap();
        let stats = aggregate(&[group]);
        // Adaptive Force is in Offense and Flex: 217 plays in each row, not 434.
        assert_eq!(stats.shard_row(0).get(5008).unwrap().play, 217);
        assert_eq!(stats.shard_row(1).get(5008).unwrap().play, 217);
        assert_eq!(stats.shard_row(2).get(5001).unwrap().play, 217);
        assert!(stats.shard_row(0).get(5001).is_none());
        let flex = stats.shard_row(1);
        assert_eq!(flex.pick_pct(5008, &[5008, 5010, 5001]), Some(100.0));
    }

    #[test]
    fn win_pct_is_none_without_plays() {
        assert_eq!(RuneStat::default().win_pct(), None);
        assert_eq!(RuneStat { play: 10, win: 4, ..RuneStat::default() }.win_pct(), Some(40.0));
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
