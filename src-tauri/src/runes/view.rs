//! View models for the rune screen, built for both the desktop flyout and the
//! phone remote from the same data.

use std::collections::HashMap;

use super::data;
use super::items;
use super::lolalytics;
use super::perks;
use super::probuilds;
use super::session;
use super::stats;
use super::AppliedView;

mod builds;
mod pro;

pub(super) use builds::keystone_build_view;
pub use builds::preset_build_view;
pub use pro::pro_builds_view;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneView {
    pub id: i64,
    pub name: String,
    pub win_pct: Option<f64>,
    pub pick_pct: Option<f64>,
    pub play: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneRowView {
    pub kind: String,
    pub label: String,
    pub runes: Vec<RuneView>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneTreeView {
    pub id: i64,
    pub name: String,
    pub keystones: Vec<RuneView>,
    pub rows: Vec<RuneRowView>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetView {
    pub index: usize,
    pub title: String,
    pub primary_page_id: i64,
    pub secondary_page_id: i64,
    pub keystone: i64,
    pub primary_runes: Vec<i64>,
    pub secondary_runes: Vec<i64>,
    pub shards: Vec<i64>,
    pub win_pct: Option<f64>,
    pub play: u64,
    /// op.gg's most-played spell pair for this champion and role, if any.
    pub spells: Vec<i64>,
}

/// One selectable summoner spell for the picker.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellView {
    pub id: i64,
    pub name: String,
}

/// One item in a build group.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: i64,
    pub name: String,
}

/// One item in the starting block, including a consumable stack count.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStackView {
    pub id: i64,
    pub name: String,
    pub count: u32,
}

/// A LoLalytics later-slot alternative and its per-slot evidence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemOptionView {
    pub id: i64,
    pub name: String,
    pub slot: u8,
    pub games: u64,
    pub win_pct: Option<f64>,
}

/// The item set people build with one preset's keystone, from lolalytics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeystoneBuildView {
    /// Starting item combination from the selected keystone page.
    pub starters: Vec<ItemStackView>,
    /// Core and the most-supported unused choice in later slots; later slots
    /// are independent marginals, not a joint sample of the full path.
    pub items: Vec<ItemView>,
    /// How many leading `items` are the one core combination `core_games` and
    /// `core_win_pct` describe (0 when no joint sample was reported). The rest
    /// are per-slot popular picks that were never observed as a whole build.
    #[serde(default)]
    pub core_len: usize,
    /// Other unique item-slot candidates, ordered by their reported support.
    pub options: Vec<ItemOptionView>,
    /// Starting-item sample size.
    pub games: u64,
    /// Sample size for the core combination.
    pub core_games: u64,
    /// Win rate for the core combination.
    pub core_win_pct: Option<f64>,
    /// True when this is the last successful lolalytics build, served because
    /// the live fetch failed.
    #[serde(default)]
    pub stale: bool,
    /// Unix milliseconds of that last successful fetch, when known.
    #[serde(default)]
    pub updated_at: Option<i64>,
    /// Ability max order, e.g. "QWE", when lolalytics reports one.
    #[serde(default)]
    pub skill_priority: Option<String>,
    /// Ability order for levels 1-15, set on matchup builds only.
    #[serde(default)]
    pub skill_order: Option<String>,
}

/// Existing pick payload plus the optional alternative from the same page.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetBuildView {
    #[serde(flatten)]
    pub pick: KeystoneBuildView,
    pub highest_win: Option<HighestWinView>,
    pub highest_win_matches_pick: bool,
}

impl PresetBuildView {
    pub fn into_items(self, highest_win: bool) -> Option<KeystoneBuildView> {
        if highest_win {
            self.highest_win.map(|win| win.build)
        } else {
            Some(self.pick)
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighestWinView {
    pub preset: PresetView,
    pub build: KeystoneBuildView,
}

/// One minute of a pro's item path: the items bought that minute, in purchase
/// order, repeated ids stacked with a count.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPathGroupView {
    /// Whole minute of the purchases (`-1` when the API did not report a path).
    pub minute: i64,
    pub items: Vec<ItemStackView>,
}

/// The local player's spells and the picker's choices for this game mode.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellsView {
    /// The player's current `spell1Id` (D).
    pub spell1_id: i64,
    /// The player's current `spell2Id` (F).
    pub spell2_id: i64,
    /// The spells the picker may offer, filtered by mode and level.
    pub available: Vec<SpellView>,
    /// The "Apply summoner spells with runes" setting.
    pub apply_with_runes: bool,
}

impl SpellsView {
    fn empty(apply_with_runes: bool) -> Self {
        Self {
            spell1_id: 0,
            spell2_id: 0,
            available: Vec::new(),
            apply_with_runes,
        }
    }
}

async fn build_spells(
    context: &session::ChampSelectContext,
    apply_with_runes: bool,
) -> SpellsView {
    let level = super::spells::player_level().await;
    let available = match super::spells::catalog().await {
        Ok(list) => super::spells::available(&list, &context.game_mode, level),
        Err(_) => Vec::new(),
    };
    SpellsView {
        spell1_id: context.spell1_id,
        spell2_id: context.spell2_id,
        available: available
            .into_iter()
            .map(|spell| SpellView {
                id: spell.id,
                name: spell.name,
            })
            .collect(),
        apply_with_runes,
    }
}

/// One pro solo-queue game, ready to render and import.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProBuildView {
    pub match_id: i64,
    pub pro_name: String,
    pub team: String,
    pub league: String,
    /// The league's region, for the coloured badge; the league code itself when
    /// the league is not mapped.
    pub region: String,
    /// The API role slug (`top`, `jungle`, `mid`, `adc`, `supp`), for the icon.
    pub role: String,
    pub role_label: String,
    pub win: bool,
    pub played_ago: String,
    pub patch: String,
    pub keystone: i64,
    pub primary_page_id: i64,
    pub secondary_page_id: i64,
    pub primary_runes: Vec<i64>,
    pub secondary_runes: Vec<i64>,
    pub shards: Vec<i64>,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    /// The pro's summoner spell ids, `[D, F]`, when the API reported both.
    pub spells: Vec<i64>,
    /// The final build as item icons, empty slots dropped and the trinket last.
    pub final_items: Vec<ItemView>,
    /// The items the pro completed, in the order the API lists them.
    pub completed_items: Vec<ItemView>,
    /// Purchases grouped by whole minute, for the item path.
    pub item_path: Vec<ItemPathGroupView>,
    /// The ability levelled at each level, as `["E", "Q", ...]`.
    pub skill_order: Vec<String>,
    /// True for a "One Trick Pony" entry, which has no real team.
    pub otp: bool,
}

/// The Pro builds tab payload for one champion, role and page.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProBuildsView {
    pub champion_id: i64,
    pub position: String,
    /// The API role that was requested, or `all` when none was.
    pub role: String,
    pub page: u32,
    /// Whether another page is available to load.
    pub has_more: bool,
    pub matches: Vec<ProBuildView>,
    /// True when the source could not be reached, so the UI shows its small
    /// unavailable state instead of an error.
    pub unavailable: bool,
    pub message: Option<String>,
    /// True when these are the last successful pro games, served because the
    /// live fetch failed.
    #[serde(default)]
    pub stale: bool,
    /// Unix milliseconds of that last successful fetch, when known.
    #[serde(default)]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyView {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunesView {
    pub phase: String,
    pub champion_id: i64,
    pub champion_name: String,
    pub position: String,
    pub mode: String,
    pub source: String,
    pub source_label: String,
    pub message: Option<String>,
    pub presets: Vec<PresetView>,
    pub trees: Vec<RuneTreeView>,
    pub shards: Vec<RuneRowView>,
    pub applied: Option<AppliedView>,
    pub auto_apply: bool,
    /// Whether applying a preset also imports its item build. Set by the
    /// caller from the saved setting.
    pub import_items: bool,
    pub can_apply: bool,
    pub locked: bool,
    /// The local player's summoner spells and the options the picker offers.
    pub spells: SpellsView,
    /// The active op.gg rank bracket slug and its label.
    pub tier: String,
    pub tier_label: String,
    /// The selectable brackets, in a fixed order.
    pub tiers: Vec<TierOption>,
    /// Whether op.gg honours `tier` for this mode.
    pub tier_supported: bool,
    /// True when the active bracket had no data but a broader one did.
    pub tier_empty: bool,
    /// The revealed enemy in the player's lane (0 when unknown), for the
    /// matchup build.
    pub enemy_champion_id: i64,
    pub enemy_champion_name: String,
    /// Every revealed enemy, for the manual opponent picker.
    pub enemies: Vec<EnemyView>,
    /// How many enemy slots the draft has; the unrevealed ones show as "TBD".
    pub enemy_slots: usize,
    /// Total op.gg games behind the preset cards, for the sample-size hint.
    pub games: u64,
    /// True when the presets are the last successful op.gg result, served
    /// because the live fetch failed.
    #[serde(default)]
    pub stale: bool,
    /// Unix milliseconds of that last successful fetch, when known.
    #[serde(default)]
    pub updated_at: Option<i64>,
}

impl RunesView {
    fn empty(
        phase: String,
        message: &str,
        auto_apply: bool,
        apply_with_runes: bool,
        tier: &str,
    ) -> Self {
        Self {
            phase,
            champion_id: 0,
            champion_name: String::new(),
            position: String::new(),
            mode: String::new(),
            source: "unavailable".into(),
            source_label: "Unavailable".into(),
            message: Some(message.into()),
            presets: Vec::new(),
            trees: Vec::new(),
            shards: Vec::new(),
            applied: None,
            auto_apply,
            import_items: false,
            can_apply: false,
            locked: false,
            spells: SpellsView::empty(apply_with_runes),
            tier: super::opgg::normalize_tier(tier).to_string(),
            tier_label: tier_options_label(tier),
            tiers: tier_options(),
            tier_supported: false,
            tier_empty: false,
            enemy_champion_id: 0,
            enemy_champion_name: String::new(),
            enemies: Vec::new(),
            enemy_slots: 0,
            games: 0,
            stale: false,
            updated_at: None,
        }
    }
}

fn tier_options() -> Vec<TierOption> {
    super::opgg::TIERS
        .iter()
        .map(|(value, label)| TierOption {
            value: (*value).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

fn tier_options_label(tier: &str) -> String {
    super::opgg::tier_label(tier)
        .unwrap_or_else(|| super::opgg::tier_label(super::opgg::DEFAULT_TIER).unwrap_or("Emerald+"))
        .to_string()
}

fn rune_view(id: i64, name: &str, stats: &stats::Aggregate, row: &[i64]) -> RuneView {
    let stat = stats.get(id);
    RuneView {
        id,
        name: name.to_string(),
        win_pct: stat.and_then(|stat| stat.win_pct()),
        pick_pct: stats.pick_pct(id, row),
        play: stat.map(|stat| stat.play).unwrap_or(0),
    }
}

pub(crate) fn build_trees(catalog: &perks::PerkCatalog, stats: &stats::Aggregate) -> Vec<RuneTreeView> {
    catalog
        .trees()
        .map(|style| {
            let keystone_ids = catalog.keystones(style.id);
            let keystones = keystone_ids
                .iter()
                .map(|id| {
                    rune_view(
                        *id,
                        catalog.name(*id).unwrap_or("Unknown"),
                        stats,
                        &keystone_ids,
                    )
                })
                .collect();
            let rows = catalog
                .rows(style.id)
                .iter()
                .map(|row| RuneRowView {
                    kind: "row".into(),
                    label: String::new(),
                    runes: row
                        .iter()
                        .map(|id| rune_view(*id, catalog.name(*id).unwrap_or("Unknown"), stats, row))
                        .collect(),
                })
                .collect();
            RuneTreeView {
                id: style.id,
                name: style.name.clone(),
                keystones,
                rows,
            }
        })
        .collect()
}

pub(crate) fn build_shards(catalog: &perks::PerkCatalog, stats: &stats::Aggregate) -> Vec<RuneRowView> {
    catalog
        .shard_rows()
        .into_iter()
        .enumerate()
        .map(|(index, (label, row))| {
            let row_stats = stats.shard_row(index);
            RuneRowView {
                kind: "statmod".into(),
                label,
                runes: row
                    .iter()
                    .map(|id| rune_view(*id, catalog.name(*id).unwrap_or("Unknown"), &row_stats, &row))
                    .collect(),
            }
        })
        .collect()
}

/// The display name for an item id, falling back to `Item <id>`.
fn item_name(names: &HashMap<i64, String>, id: i64) -> String {
    names
        .get(&id)
        .cloned()
        .unwrap_or_else(|| format!("Item {id}"))
}

/// Maps a group of item ids to `ItemView`s with their display names.
fn item_views(ids: &[i64], names: &HashMap<i64, String>) -> Vec<ItemView> {
    ids.iter()
        .map(|id| ItemView {
            id: *id,
            name: item_name(names, *id),
        })
        .collect()
}

/// Structural index used to validate selections from either surface.
pub fn catalog_index(catalog: &perks::PerkCatalog) -> super::CatalogIndex {
    let mut index = super::CatalogIndex::default();
    for style in catalog.trees() {
        for keystone in catalog.keystones(style.id) {
            index.keystones.insert(keystone, style.id);
        }
        index.rows.insert(style.id, catalog.rows(style.id));
    }
    index.shard_rows = catalog
        .shard_rows()
        .into_iter()
        .map(|(_, runes)| runes)
        .collect();
    index
}

fn source_label(source: &str, provider: super::provider::Provider) -> &'static str {
    match source {
        "none" => "Unavailable",
        _ => provider.label(),
    }
}

/// Builds the rune screen for the current champion select.
pub async fn view(
    auto_apply: bool,
    apply_with_runes: bool,
    tier: &str,
    requested_position: Option<&str>,
) -> RunesView {
    let (phase, context) = match super::rune_context().await {
        Ok(super::RuneContext { phase, context, .. }) => (phase, context),
        Err(error) => {
            return RunesView::empty(
                "Unavailable".into(),
                error.message(),
                auto_apply,
                apply_with_runes,
                tier,
            );
        }
    };
    let Some(mut context) = context else {
        let message = if phase == "ChampSelect" {
            "Pick a champion to load runes."
        } else {
            "Not in champion select."
        };
        return RunesView::empty(phase, message, auto_apply, apply_with_runes, tier);
    };
    let catalog = match data::catalog().await {
        Ok(catalog) => catalog,
        Err(error) => {
            return RunesView::empty(
                phase,
                error.message(),
                auto_apply,
                apply_with_runes,
                tier,
            );
        }
    };
    if context.champion_name.trim().is_empty() {
        if let Ok(names) = data::champion_names().await {
            if let Some(name) = names.get(&context.champion_id) {
                context.champion_name = name.clone();
            }
        }
    }
    let current = match super::lcu().await {
        Ok(lcu) => lcu,
        Err(error) => {
            return RunesView::empty(
                phase,
                error.message(),
                auto_apply,
                apply_with_runes,
                tier,
            );
        }
    };
    let detected_position = super::position_for(&current, &context, tier).await;
    let position = requested_position
        .and_then(session::position_from_request)
        .filter(|position| *position != super::opgg::POSITION_NONE)
        .unwrap_or(detected_position);
    let loaded = data::load_for(&current, &context, &catalog, tier, position)
        .await
        .unwrap_or(data::Loaded {
            source: "none",
            provider: super::provider::Provider::Opgg,
            groups: Vec::new(),
            selections: Vec::new(),
            spell_pair: None,
            tier_empty: false,
            stale: false,
            fetched_at: None,
        });
    let preset_spells = loaded
        .spell_pair
        .map(|pair| pair.to_vec())
        .unwrap_or_default();
    let aggregates = stats::aggregate(&loaded.groups);
    let presets: Vec<PresetView> = loaded
        .selections
        .iter()
        .enumerate()
        .map(|(index, preset)| PresetView {
            index,
            title: preset.title.clone(),
            primary_page_id: preset.selection.primary_page_id,
            secondary_page_id: preset.selection.secondary_page_id,
            keystone: preset.selection.keystone,
            primary_runes: preset.selection.primary_runes.clone(),
            secondary_runes: preset.selection.secondary_runes.clone(),
            shards: preset.selection.shards.clone(),
            win_pct: preset.win_pct,
            play: preset.play,
            spells: preset_spells.clone(),
        })
        .collect();
    let position = position.to_string();
    let mode = context.mode().to_string();
    let applied = super::shared()
        .applied
        .clone()
        .filter(|applied| applied.champion_id == context.champion_id);
    let can_apply = !presets.is_empty();
    let games: u64 = loaded.groups.iter().map(|group| group.play).sum();
    let tier = super::opgg::normalize_tier(tier).to_string();
    let tier_label = tier_options_label(&tier);
    let tier_empty = loaded.tier_empty;
    let spells = build_spells(&context, apply_with_runes).await;
    // The auto-detected laner is only a default for the assigned lane; the
    // picker lists every revealed enemy whatever role is being viewed.
    let enemy_champion_id = context
        .enemy_champion_id
        .filter(|_| position == detected_position)
        .unwrap_or(0);
    let champion_names = data::champion_names().await.unwrap_or_default();
    let enemy_champion_name = champion_names
        .get(&enemy_champion_id)
        .cloned()
        .unwrap_or_default();
    let enemies = context
        .enemy_champion_ids
        .iter()
        .map(|id| EnemyView {
            id: *id,
            name: champion_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("Champion {id}")),
        })
        .collect();
    let message = if tier_empty {
        Some(format!("Not enough games at {tier_label}."))
    } else {
        (loaded.source == "none" && presets.is_empty())
            .then(|| "Rune recommendations are unavailable right now.".to_string())
    };
    RunesView {
        phase,
        champion_id: context.champion_id,
        champion_name: context.champion_name,
        position,
        mode: mode.clone(),
        source: loaded.source.to_string(),
        source_label: source_label(loaded.source, loaded.provider).to_string(),
        message,
        presets,
        trees: build_trees(&catalog, &aggregates),
        shards: build_shards(&catalog, &aggregates),
        applied,
        auto_apply,
        import_items: false,
        can_apply,
        locked: context.locked,
        spells,
        tier,
        tier_label,
        tiers: tier_options(),
        tier_supported: super::opgg::tier_supported(&mode),
        tier_empty,
        enemy_champion_id,
        enemy_champion_name,
        enemies,
        enemy_slots: context.enemy_slots,
        games,
        stale: loaded.stale,
        updated_at: loaded.fetched_at,
    }
}



#[cfg(test)]
mod tests;
