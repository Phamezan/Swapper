//! Per-keystone item builds for the preset cards.

use super::*;

/// The 6-item build people build with one preset's keystone, for a champion,
/// role and bracket. Returns `None` (so the card shows nothing) when the
/// champion cannot be mapped, the mode has no lane, or lolalytics has no build;
/// a network problem is treated the same way unless a cached build exists.
pub async fn preset_build_view(
    champion_id: i64,
    position: &str,
    tier: &str,
    keystone: i64,
    most_played: Option<&super::super::RuneSelection>,
) -> Option<PresetBuildView> {
    if champion_id <= 0 || keystone <= 0 {
        return None;
    }
    let lane = lolalytics::lane(position)?;
    let names = data::champion_names().await.ok()?;
    let slug = lolalytics::champion_slug(names.get(&champion_id)?)?;
    let keystone_page = data::keystone_build(&slug, lane, tier, keystone).await.ok().flatten();
    // A keystone page with few games has no build summaries, or no "Highest Win
    // Build"; the champion's own page (already loaded for its overview) usually has both.
    let overview = match &keystone_page {
        Some(page) if page.value.win.is_some() => None,
        _ => super::super::overview::load(&slug, champion_id, Some(lane), tier).await.ok().flatten(),
    };
    let sourced = match (keystone_page, overview) {
        (Some(mut page), overview) => {
            if let Some(overview) = overview {
                page.value.win = page.value.win.or(overview.value.win);
            }
            page
        }
        (None, Some(overview)) => super::super::provider::Sourced {
            value: lolalytics::KeystoneBuilds { pick: overview.value.build?, win: overview.value.win, pick_runes: overview.value.runes, pick_spells: overview.value.spells },
            provider: overview.provider,
            stale: overview.stale,
            fetched_at: overview.fetched_at,
        },
        (None, None) => return None,
    };
    let item_names = items::names().await;
    let catalog = data::catalog().await.ok();
    let highest_win = catalog
        .as_ref()
        .ok_or(HighestWinOmission::Unavailable)
        .and_then(|catalog| {
            highest_win_view(
                &sourced.value,
                most_played,
                catalog,
                sourced.stale,
                sourced.fetched_at,
                &item_names,
            )
        });
    Some(PresetBuildView {
        pick: keystone_build_view(
            sourced.value.pick,
            sourced.stale,
            sourced.fetched_at,
            &item_names,
        ),
        highest_win_matches_pick: matches!(highest_win, Err(HighestWinOmission::Duplicate)),
        highest_win: highest_win.ok(),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum HighestWinOmission {
    Unavailable,
    Duplicate,
}

pub(super) fn highest_win_view(
    builds: &lolalytics::KeystoneBuilds,
    most_played: Option<&super::super::RuneSelection>,
    catalog: &perks::PerkCatalog,
    stale: bool,
    updated_at: Option<i64>,
    item_names: &HashMap<i64, String>,
) -> Result<HighestWinView, HighestWinOmission> {
    let win = builds.win.as_ref().ok_or(HighestWinOmission::Unavailable)?;
    let selection = win
        .runes
        .selection(catalog)
        .ok_or(HighestWinOmission::Unavailable)?;
    super::super::page::validate(&selection, &catalog_index(catalog))
        .map_err(|_| HighestWinOmission::Unavailable)?;
    if most_played.is_some_and(|selection| win.matches_most_played(selection, &builds.pick)) {
        return Err(HighestWinOmission::Duplicate);
    }
    Ok(HighestWinView {
        preset: PresetView {
            index: 0,
            title: catalog
                .name(selection.keystone)
                .unwrap_or("Highest win rate")
                .to_string(),
            primary_page_id: selection.primary_page_id,
            secondary_page_id: selection.secondary_page_id,
            keystone: selection.keystone,
            primary_runes: selection.primary_runes,
            secondary_runes: selection.secondary_runes,
            shards: selection.shards,
            win_pct: win.build.core_win_pct,
            play: win.build.core_games,
            spells: win.spells.map(|pair| pair.to_vec()).unwrap_or_default(),
        },
        build: keystone_build_view(win.build.clone(), stale, updated_at, item_names),
    })
}

/// The display model for one lolalytics build.
pub(in crate::runes) fn keystone_build_view(
    build: lolalytics::KeystoneBuild,
    stale: bool,
    updated_at: Option<i64>,
    item_names: &HashMap<i64, String>,
) -> KeystoneBuildView {
    KeystoneBuildView {
        starters: build
            .starters
            .iter()
            .map(|stack| ItemStackView {
                id: stack.id,
                name: item_names
                    .get(&stack.id)
                    .cloned()
                    .unwrap_or_else(|| format!("Item {}", stack.id)),
                count: stack.count,
            })
            .collect(),
        items: item_views(&build.items, item_names),
        options: build
            .options
            .iter()
            .map(|candidate| {
                let name = item_names
                    .get(&candidate.id)
                    .cloned()
                    .unwrap_or_else(|| format!("Item {}", candidate.id));
                ItemOptionView {
                    id: candidate.id,
                    name,
                    slot: candidate.slot,
                    games: candidate.games,
                    win_pct: candidate.win_pct,
                }
            })
            .collect(),
        games: build.games,
        core_len: build.core_len,
        core_games: build.core_games,
        core_win_pct: build.core_win_pct,
        stale,
        updated_at,
        skill_priority: build.skill_priority,
        skill_order: build.skill_order,
    }
}

/// Below this share of games a keystone is too rare to suggest as "Other".
const MIN_OTHER_PICK_PCT: f64 = 10.0;

/// The most played keystone not in `exclude`, among those at
/// [`MIN_OTHER_PICK_PCT`] or more.
fn other_keystone(shares: &[lolalytics::KeystoneShare], exclude: &[i64], is_keystone: impl Fn(i64) -> bool) -> Option<i64> {
    shares
        .iter()
        .filter(|share| share.pick_pct >= MIN_OTHER_PICK_PCT && !exclude.contains(&share.id) && is_keystone(share.id))
        .max_by(|a, b| a.pick_pct.total_cmp(&b.pick_pct))
        .map(|share| share.id)
}

/// A common build on another keystone than the ones already shown (`exclude`),
/// e.g. Phase Rush beside First Strike: that keystone's most common runes,
/// spells and items. `None` when no other keystone is played often enough or
/// its page has no runes.
pub async fn other_build_view(champion_id: i64, position: &str, tier: &str, exclude: &[i64]) -> Option<HighestWinView> {
    if champion_id <= 0 {
        return None;
    }
    let lane = lolalytics::lane(position)?;
    let names = data::champion_names().await.ok()?;
    let slug = lolalytics::champion_slug(names.get(&champion_id)?)?;
    let catalog = data::catalog().await.ok()?;
    let overview = super::super::overview::load(&slug, champion_id, Some(lane), tier).await.ok().flatten()?;
    let keystones: Vec<i64> = catalog.trees().flat_map(|tree| catalog.keystones(tree.id)).collect();
    let keystone = other_keystone(&overview.value.keystones, exclude, |id| keystones.contains(&id))?;
    let page = data::keystone_build(&slug, lane, tier, keystone).await.ok().flatten()?;
    let selection = page.value.pick_runes.as_ref()?.selection(&catalog)?;
    super::super::page::validate(&selection, &catalog_index(&catalog)).ok()?;
    let item_names = items::names().await;
    let build = page.value.pick;
    Some(HighestWinView {
        preset: PresetView {
            index: 0,
            title: catalog.name(selection.keystone).unwrap_or("Other").to_string(),
            primary_page_id: selection.primary_page_id,
            secondary_page_id: selection.secondary_page_id,
            keystone: selection.keystone,
            primary_runes: selection.primary_runes,
            secondary_runes: selection.secondary_runes,
            shards: selection.shards,
            win_pct: build.core_win_pct,
            play: build.core_games,
            spells: page.value.pick_spells.map(|pair| pair.to_vec()).unwrap_or_default(),
        },
        build: keystone_build_view(build, page.stale, page.fetched_at, &item_names),
    })
}

#[cfg(test)]
mod other_tests {
    use super::*;

    fn share(id: i64, pick_pct: f64) -> lolalytics::KeystoneShare {
        lolalytics::KeystoneShare { id, pick_pct, games: 100 }
    }

    #[test]
    fn other_is_the_most_played_keystone_not_already_shown() {
        // Kha'Zix jungle, Diamond+: First Strike 48.7%, Phase Rush 29.6%, Dark Harvest 19.5%; 8105 is a minor rune.
        let shares = [share(8369, 48.7), share(8230, 29.6), share(8128, 19.5), share(8105, 60.0), share(8010, 0.8)];
        let keystone = |id: i64| id != 8105;
        assert_eq!(other_keystone(&shares, &[8369], keystone), Some(8230));
        assert_eq!(other_keystone(&shares, &[8369, 8230], keystone), Some(8128));
        assert_eq!(other_keystone(&shares, &[8369, 8230, 8128], keystone), None, "0.8% is too rare");
    }
}
