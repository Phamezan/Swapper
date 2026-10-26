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
            value: lolalytics::KeystoneBuilds { pick: overview.value.build?, win: overview.value.win },
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
