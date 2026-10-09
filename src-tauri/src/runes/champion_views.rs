//! View models for the Champion tab beyond matchups: the lane tier list and a
//! champion's overview with its most common build. Shared by the desktop
//! flyout and the phone remote.

use std::collections::HashMap;

use super::data;
use super::items;
use super::lolalytics;
use super::overview::{self, Damage, Overview};
use super::provider::Sourced;
use super::tierlist::{self, TierList};
use super::view::{self, KeystoneBuildView};

// ---------------------------------------------------------------------------
// Tier list
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierRowView {
    pub champion_id: i64,
    pub name: String,
    /// lolalytics' tier label, "S+" to "D-".
    pub tier: String,
    pub win_pct: f64,
    pub pick_pct: f64,
    pub ban_pct: f64,
    pub games: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierListView {
    pub rows: Vec<TierRowView>,
    /// True when no list could be loaded.
    pub unavailable: bool,
    pub stale: bool,
    pub updated_at: Option<i64>,
}

/// The tier list for a Swapper role (`mid`, `adc`, ...) and rank bracket.
pub async fn tier_list_view(position: &str, tier: &str) -> TierListView {
    let found = match lolalytics::lane(position) {
        Some(lane) => tierlist::load(lane, tier).await.ok().flatten(),
        None => None,
    };
    let names = data::champion_names().await.unwrap_or_default();
    build_tier_list(found, &names)
}

fn build_tier_list(found: Option<Sourced<TierList>>, names: &HashMap<i64, String>) -> TierListView {
    let Some(sourced) = found else {
        return TierListView {
            rows: Vec::new(),
            unavailable: true,
            stale: false,
            updated_at: None,
        };
    };
    let rows = sourced
        .value
        .rows
        .iter()
        // A champion the client does not know yet has no name to show.
        .filter_map(|row| {
            Some(TierRowView {
                champion_id: row.champion_id,
                name: names.get(&row.champion_id)?.clone(),
                tier: row.label().to_string(),
                win_pct: row.win_pct,
                pick_pct: row.pick_pct,
                ban_pct: row.ban_pct,
                games: row.games,
            })
        })
        .collect();
    TierListView {
        rows,
        unavailable: false,
        stale: sourced.stale,
        updated_at: sourced.fetched_at,
    }
}

// ---------------------------------------------------------------------------
// Overview and build summary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewStatsView {
    pub tier: String,
    pub win_pct: f64,
    pub avg_win_pct: f64,
    /// `win_pct` minus the champion's average win rate.
    pub delta: f64,
    pub pick_pct: f64,
    pub ban_pct: f64,
    pub games: u64,
    pub rank: u32,
    pub rank_total: u32,
    pub patch: String,
}

/// Share of damage dealt by type, in percent.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageView {
    pub physical: f64,
    pub magic: f64,
    pub true_damage: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionRunesView {
    pub keystone: i64,
    pub primary_runes: Vec<i64>,
    pub secondary_runes: Vec<i64>,
    pub shards: Vec<i64>,
}

/// What the champion probably builds. Read-only; nothing applies from it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionBuildView {
    pub runes: Option<ChampionRunesView>,
    pub spells: Vec<i64>,
    pub items: KeystoneBuildView,
    /// Ability order for levels 1-15.
    pub skill_order: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionOverviewView {
    pub champion_id: i64,
    /// The Swapper role the page is for, or empty if unknown.
    pub position: String,
    pub stats: Option<OverviewStatsView>,
    pub damage: Option<DamageView>,
    pub build: Option<ChampionBuildView>,
    /// True when no overview could be loaded.
    pub unavailable: bool,
}

/// The overview for a champion. `position` is a Swapper role; without a valid
/// one lolalytics picks the champion's usual lane.
pub async fn champion_overview_view(
    champion_id: i64,
    position: Option<&str>,
    tier: &str,
) -> ChampionOverviewView {
    let names = data::champion_names().await.unwrap_or_default();
    let lane = position.and_then(lolalytics::lane);
    let found = match names.get(&champion_id).and_then(|name| lolalytics::champion_slug(name)) {
        Some(slug) if champion_id > 0 => overview::load(&slug, champion_id, lane, tier)
            .await
            .ok()
            .flatten(),
        _ => None,
    };
    let item_names = match &found {
        Some(sourced) if sourced.value.build.is_some() => items::names().await,
        _ => Default::default(),
    };
    build_overview(champion_id, found, &item_names)
}

fn percent_split(damage: &Damage) -> DamageView {
    let total = damage.physical + damage.magic + damage.true_damage;
    DamageView {
        physical: damage.physical / total * 100.0,
        magic: damage.magic / total * 100.0,
        true_damage: damage.true_damage / total * 100.0,
    }
}

fn build_overview(
    champion_id: i64,
    found: Option<Sourced<Overview>>,
    item_names: &HashMap<i64, String>,
) -> ChampionOverviewView {
    let Some(sourced) = found else {
        return ChampionOverviewView {
            champion_id,
            position: String::new(),
            stats: None,
            damage: None,
            build: None,
            unavailable: true,
        };
    };
    let Sourced {
        value: overview,
        stale,
        fetched_at,
        ..
    } = sourced;
    let stats = &overview.stats;
    let build = overview.build.map(|build| ChampionBuildView {
        runes: overview.runes.map(|runes| ChampionRunesView {
            keystone: runes.keystone,
            primary_runes: runes.primary_runes,
            secondary_runes: runes.secondary_runes,
            shards: runes.shards,
        }),
        spells: overview.spells.map(|pair| pair.to_vec()).unwrap_or_default(),
        items: view::keystone_build_view(build, stale, fetched_at, item_names),
        skill_order: overview.skill_order,
    });
    ChampionOverviewView {
        champion_id,
        position: super::counters_view::position_of(&overview.lane).to_string(),
        stats: Some(OverviewStatsView {
            tier: stats.tier.clone(),
            win_pct: stats.win_pct,
            avg_win_pct: stats.avg_win_pct,
            delta: stats.win_pct - stats.avg_win_pct,
            pick_pct: stats.pick_pct,
            ban_pct: stats.ban_pct,
            games: stats.games,
            rank: stats.rank,
            rank_total: stats.rank_total,
            patch: stats.patch.clone(),
        }),
        damage: overview.damage.as_ref().map(percent_split),
        build,
        unavailable: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runes::lolalytics::KeystoneBuild;
    use crate::runes::matchup::MatchupRunes;
    use crate::runes::overview::OverviewStats;
    use crate::runes::provider::Provider;
    use crate::runes::tierlist::TierRow;

    fn row(id: i64, tier: u8) -> TierRow {
        TierRow {
            champion_id: id,
            tier,
            win_pct: 52.0,
            pick_pct: 5.0,
            ban_pct: 3.0,
            games: 9000,
        }
    }

    #[test]
    fn tier_rows_get_names_and_labels_and_unknown_champions_are_skipped() {
        let names = HashMap::from([(31, "Cho'Gath".to_string())]);
        let list = TierList {
            rows: vec![row(31, 1), row(9999, 2)],
        };
        let view = build_tier_list(Some(Sourced::fresh(list, Provider::Lolalytics)), &names);
        assert_eq!(view.rows.len(), 1);
        assert_eq!((view.rows[0].name.as_str(), view.rows[0].tier.as_str()), ("Cho'Gath", "S+"));
        assert!(build_tier_list(None, &names).unavailable);
    }

    fn overview() -> Overview {
        Overview {
            lane: "bottom".into(),
            stats: OverviewStats {
                tier: "A".into(),
                win_pct: 52.0,
                avg_win_pct: 50.5,
                pick_pct: 8.0,
                ban_pct: 2.0,
                games: 1000,
                rank: 7,
                rank_total: 40,
                patch: "16.20".into(),
            },
            damage: Some(Damage {
                physical: 75.0,
                magic: 25.0,
                true_damage: 0.0,
            }),
            build: Some(KeystoneBuild {
                items: vec![1, 2, 3],
                ..KeystoneBuild::default()
            }),
            runes: Some(MatchupRunes {
                keystone: 8112,
                ..MatchupRunes::default()
            }),
            spells: Some([4, 14]),
            skill_order: Some("QWEQQRQWQWRWWEE".into()),
        }
    }

    #[test]
    fn the_overview_carries_delta_damage_shares_and_the_build() {
        let found = Some(Sourced::fresh(overview(), Provider::Lolalytics));
        let view = build_overview(22, found, &HashMap::new());
        assert_eq!(view.position, "adc");
        let stats = view.stats.expect("stats");
        assert!((stats.delta - 1.5).abs() < 1e-9);
        assert_eq!((stats.rank, stats.rank_total), (7, 40));
        let damage = view.damage.expect("damage");
        assert!((damage.physical - 75.0).abs() < 1e-9 && damage.true_damage == 0.0);
        let build = view.build.expect("build");
        assert_eq!(build.spells, vec![4, 14]);
        assert_eq!(build.items.items.len(), 3);
        assert_eq!(build.runes.expect("runes").keystone, 8112);
    }

    #[test]
    fn a_missing_overview_is_unavailable() {
        let view = build_overview(22, None, &HashMap::new());
        assert!(view.unavailable && view.stats.is_none() && view.build.is_none());
    }
}
