//! Champion-select prefetch.
//!
//! When the local player's champion becomes known — hovering or locking — the
//! presets and the pro builds page for that champion and role are warmed in the
//! background, along with the icon bytes they reference. The 2s u.gg latency is
//! then hidden behind the pick and opening a tab is instant.
//!
//! Only one prefetch runs at a time: a new champion bumps a generation counter
//! and the previous task stops at its next checkpoint. There are no retries.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use futures_util::stream::{self, StreamExt};

use super::session::ChampSelectContext;
use super::{data, RuneSelection};

/// How many icon fetches may run at once while warming. Keeps the warm-up
/// polite without making it serial.
const ICON_CONCURRENCY: usize = 8;
/// How many lolalytics build pages a prefetch may fetch at once. Each page is a
/// few hundred KB, so this is deliberately small.
const BUILD_CONCURRENCY: usize = 3;

static GENERATION: AtomicU64 = AtomicU64::new(0);
/// `champion|position|tier` of the prefetch already started, so the watcher's
/// poll does not restart it every few seconds.
static CURRENT_KEY: Mutex<Option<String>> = Mutex::new(None);

/// Warms the presets and pro builds for a champion select, unless the same
/// champion, role and bracket are already being warmed.
pub fn spawn(app: &tauri::AppHandle, context: &ChampSelectContext) {
    if context.champion_id <= 0 {
        return;
    }
    let tier = super::watch::configured_tier(app);
    let position = context.position().unwrap_or("jungle").to_string();
    let key = format!("{}|{}|{}", context.champion_id, position, tier);
    {
        let mut current = CURRENT_KEY
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if current.as_deref() == Some(key.as_str()) {
            return;
        }
        *current = Some(key);
    }
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let context = context.clone();
    tauri::async_runtime::spawn(async move {
        run(context, tier, generation).await;
    });
}

/// Whether a newer prefetch has started, so this one should stop.
fn stale(generation: u64) -> bool {
    GENERATION.load(Ordering::SeqCst) != generation
}

async fn run(context: ChampSelectContext, tier: String, generation: u64) {
    let Ok(catalog) = data::catalog().await else {
        return;
    };
    if stale(generation) {
        return;
    }
    // Catalog-backed data both views need. Each is single-flighted and cached.
    let _ = data::champion_names().await;
    let _ = super::items::names().await;
    let _ = super::spells::catalog().await;
    let current = super::lcu().await.ok();
    let position = match current.as_ref() {
        Some(lcu) => super::position_for(lcu, &context).await.to_string(),
        None => context.position().unwrap_or("jungle").to_string(),
    };
    let mut runes: Vec<i64> = Vec::new();
    let mut items: Vec<i64> = Vec::new();
    let mut spells: Vec<i64> = Vec::new();
    let mut keystones: Vec<i64> = Vec::new();

    // Presets for the current champion and role.
    if let Some(lcu) = current.as_ref() {
        if let Ok(loaded) = data::load_for(lcu, &context, &catalog, &tier, &position).await {
            if stale(generation) {
                return;
            }
            for preset in &loaded.selections {
                collect_selection(&preset.selection, &mut runes);
                keystones.push(preset.selection.keystone);
            }
            if let Some(pair) = loaded.spell_pair {
                spells.extend(pair);
            }
        }
    }
    if stale(generation) {
        return;
    }
    // Each preset's keystone build from lolalytics, bounded and single-flighted.
    // The item ids are collected so their icons are warm too.
    unique(&mut keystones);
    let builds: Vec<Option<super::KeystoneBuildView>> = stream::iter(keystones)
        .map(|keystone| {
            let position = position.clone();
            let tier = tier.clone();
            async move {
                super::view::preset_build_view(context.champion_id, &position, &tier, keystone)
                    .await
            }
        })
        .buffer_unordered(BUILD_CONCURRENCY)
        .collect()
        .await;
    if stale(generation) {
        return;
    }
    for build in builds.into_iter().flatten() {
        items.extend(build.starters.iter().map(|item| item.id));
        items.extend(build.items.iter().map(|item| item.id));
        items.extend(build.options.iter().map(|item| item.id));
    }
    // Pro builds page 1: the slow u.gg call this prefetch exists to hide.
    if let Ok(sourced) = data::pro_builds(context.champion_id, &position, 1, false).await {
        if stale(generation) {
            return;
        }
        for matched in &sourced.value {
            if let Some(selection) = matched.selection() {
                collect_selection(&selection, &mut runes);
            }
            items.extend(matched.final_items());
            items.extend(matched.completed_items.iter().copied().filter(|id| *id > 0));
            items.extend(
                matched
                    .item_path_groups()
                    .into_iter()
                    .flat_map(|(_, group)| group.into_iter().map(|(id, _)| id)),
            );
            spells.extend(
                matched
                    .summoner_spells
                    .iter()
                    .copied()
                    .filter(|id| *id > 0)
                    .take(2),
            );
        }
    }
    if stale(generation) {
        return;
    }
    let _ = super::roles::icon(&position).await;
    warm(runes, items, spells, generation).await;
}

fn collect_selection(selection: &RuneSelection, runes: &mut Vec<i64>) {
    runes.push(selection.primary_page_id);
    runes.push(selection.secondary_page_id);
    runes.push(selection.keystone);
    runes.extend(selection.primary_runes.iter().copied());
    runes.extend(selection.secondary_runes.iter().copied());
    runes.extend(selection.shards.iter().copied());
}

/// The icon fetches themselves, bounded by [`ICON_CONCURRENCY`]. The backend's
/// single-flight gates collapse any duplicate ids.
async fn warm(runes: Vec<i64>, items: Vec<i64>, spells: Vec<i64>, generation: u64) {
    enum Job {
        Rune(i64),
        Item(i64),
        Spell(i64),
    }
    let mut runes = runes;
    let mut items = items;
    let mut spells = spells;
    unique(&mut runes);
    unique(&mut items);
    unique(&mut spells);
    let jobs: Vec<Job> = runes
        .into_iter()
        .map(Job::Rune)
        .chain(items.into_iter().map(Job::Item))
        .chain(spells.into_iter().map(Job::Spell))
        .collect();
    stream::iter(jobs)
        .for_each_concurrent(ICON_CONCURRENCY, |job| async move {
            if stale(generation) {
                return;
            }
            match job {
                Job::Rune(id) => {
                    let _ = data::icon(id).await;
                }
                Job::Item(id) => {
                    let _ = super::items::icon(id).await;
                }
                Job::Spell(id) => {
                    let _ = super::spells::icon(id).await;
                }
            }
        })
        .await;
}

fn unique(ids: &mut Vec<i64>) {
    ids.retain(|id| *id > 0);
    ids.sort_unstable();
    ids.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_drops_non_positive_and_duplicate_ids() {
        let mut ids = vec![5008, 0, -1, 5008, 8112, 8112, 5005];
        unique(&mut ids);
        assert_eq!(ids, vec![5005, 5008, 8112]);
    }

    #[test]
    fn collect_selection_gathers_every_rune_id_including_trees_and_shards() {
        let selection = RuneSelection {
            primary_page_id: 8100,
            secondary_page_id: 8200,
            keystone: 8112,
            primary_runes: vec![8139, 8140, 8106],
            secondary_runes: vec![8237, 8226],
            shards: vec![5005, 5008, 5011],
        };
        let mut runes = Vec::new();
        collect_selection(&selection, &mut runes);
        assert_eq!(
            runes,
            vec![8100, 8200, 8112, 8139, 8140, 8106, 8237, 8226, 5005, 5008, 5011]
        );
    }
}
