//! View model for the lane matchup build, shared by the desktop flyout and the
//! phone remote.

use std::collections::HashMap;

use super::data;
use super::items;
use super::lolalytics;
use super::matchup::Matchup;
use super::perks::PerkCatalog;
use super::provider::Sourced;
use super::view::{self, KeystoneBuildView, PresetView};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchupStatsView {
    pub win_pct: f64,
    pub avg_win_pct: f64,
    /// Matchup win rate minus the champion's average.
    pub delta: f64,
    pub games: u64,
    pub patch: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchupView {
    pub enemy_champion_id: i64,
    pub enemy_name: String,
    /// `None` when lolalytics had no usable page for this matchup.
    pub stats: Option<MatchupStatsView>,
    /// True when the generic build should be shown: the sample is below
    /// [`super::matchup::MIN_MATCHUP_GAMES`] or the matchup could not be loaded.
    pub fallback: bool,
    /// The matchup's rune page and spells, as a selectable preset.
    pub preset: Option<PresetView>,
    /// Items and skill order for the matchup.
    pub build: Option<KeystoneBuildView>,
    /// True when this is the last successful result served after its TTL
    /// because a fetch failed; auto-apply must not trust it.
    pub stale: bool,
}

/// The matchup build for the local champion against `enemy_id` in `position`.
/// `None` only when the matchup cannot be asked for at all (no lane, or a
/// champion name is unknown), so the UI shows nothing.
pub async fn matchup_view(
    champion_id: i64,
    enemy_id: i64,
    position: &str,
    tier: &str,
) -> Option<MatchupView> {
    if champion_id <= 0 || enemy_id <= 0 {
        return None;
    }
    let lane = lolalytics::lane(position)?;
    let names = data::champion_names().await.ok()?;
    let slug = lolalytics::champion_slug(names.get(&champion_id)?)?;
    let enemy_name = names.get(&enemy_id)?.clone();
    let enemy_slug = lolalytics::champion_slug(&enemy_name)?;
    let found = data::matchup(&slug, &enemy_slug, champion_id, enemy_id, lane, tier)
        .await
        .ok()
        .flatten();
    let catalog = data::catalog().await.ok();
    let item_names = items::names().await;
    Some(build_view(
        enemy_id,
        enemy_name,
        found,
        catalog.as_ref(),
        &item_names,
    ))
}

fn build_view(
    enemy_id: i64,
    enemy_name: String,
    found: Option<Sourced<Matchup>>,
    catalog: Option<&PerkCatalog>,
    item_names: &HashMap<i64, String>,
) -> MatchupView {
    let Some(sourced) = found else {
        return fallback_view(enemy_id, enemy_name, None);
    };
    let Sourced {
        value: matchup,
        stale,
        fetched_at,
        ..
    } = sourced;
    let stats = MatchupStatsView {
        win_pct: matchup.stats.win_pct,
        avg_win_pct: matchup.stats.avg_win_pct,
        delta: matchup.stats.delta(),
        games: matchup.stats.games,
        patch: matchup.stats.patch.clone(),
    };
    if !matchup.stats.is_reliable() {
        return fallback_view(enemy_id, enemy_name, Some(stats));
    }
    let preset = catalog
        .and_then(|catalog| matchup.selection(catalog))
        .map(|selection| PresetView {
            index: 0,
            title: format!("vs {enemy_name}"),
            primary_page_id: selection.primary_page_id,
            secondary_page_id: selection.secondary_page_id,
            keystone: selection.keystone,
            primary_runes: selection.primary_runes,
            secondary_runes: selection.secondary_runes,
            shards: selection.shards,
            win_pct: Some(stats.win_pct),
            play: stats.games,
            spells: matchup.spells.map(|pair| pair.to_vec()).unwrap_or_default(),
        });
    let mut build = view::keystone_build_view(matchup.build, stale, fetched_at, item_names);
    build.skill_order = matchup.skill_order;
    MatchupView {
        enemy_champion_id: enemy_id,
        enemy_name,
        stats: Some(stats),
        fallback: false,
        preset,
        build: Some(build),
        stale,
    }
}

fn fallback_view(
    enemy_id: i64,
    enemy_name: String,
    stats: Option<MatchupStatsView>,
) -> MatchupView {
    MatchupView {
        enemy_champion_id: enemy_id,
        enemy_name,
        stats,
        fallback: true,
        preset: None,
        build: None,
        stale: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runes::lolalytics::KeystoneBuild;
    use crate::runes::provider::Provider;
    use crate::runes::matchup::{MatchupRunes, MatchupStats};

    fn matchup(games: u64) -> Sourced<Matchup> {
        Sourced::fresh(
            Matchup {
                stats: MatchupStats {
                    win_pct: 52.59,
                    avg_win_pct: 51.88,
                    games,
                    patch: "16.20".into(),
                },
                build: KeystoneBuild {
                    items: vec![3118, 3020, 4645],
                    ..KeystoneBuild::default()
                },
                runes: Some(MatchupRunes {
                    keystone: 8112,
                    ..MatchupRunes::default()
                }),
                spells: Some([4, 14]),
                skill_order: Some("WQEQQRQWQWRWWEE".into()),
            },
            super::super::provider::Provider::Lolalytics,
        )
    }

    #[test]
    fn a_large_enough_sample_offers_the_matchup_build() {
        let view = build_view(238, "Zed".into(), Some(matchup(270)), None, &HashMap::new());
        assert!(!view.fallback);
        let build = view.build.expect("a reliable matchup has a build");
        assert_eq!(build.skill_order.as_deref(), Some("WQEQQRQWQWRWWEE"));
        assert_eq!(build.items.len(), 3);
        assert_eq!(view.stats.expect("stats").games, 270);
        // Without a rune catalog there is no preset, but the build still shows.
        assert!(view.preset.is_none());
    }

    #[test]
    fn data_served_after_a_failed_fetch_is_marked_stale() {
        let cached = Sourced::stale(matchup(270).value, Provider::Lolalytics, 1);
        let view = build_view(238, "Zed".into(), Some(cached), None, &HashMap::new());
        assert!(view.stale);
        let view = build_view(238, "Zed".into(), Some(matchup(270)), None, &HashMap::new());
        assert!(!view.stale);
    }

    #[test]
    fn a_low_sample_keeps_the_stats_but_falls_back_to_the_generic_build() {
        let view = build_view(238, "Zed".into(), Some(matchup(99)), None, &HashMap::new());
        assert!(view.fallback);
        assert!(view.build.is_none() && view.preset.is_none());
        assert_eq!(view.stats.expect("stats").games, 99);
    }

    #[test]
    fn a_missing_matchup_falls_back_without_stats() {
        let view = build_view(238, "Zed".into(), None, None, &HashMap::new());
        assert!(view.fallback);
        assert!(view.stats.is_none() && view.build.is_none());
    }
}
