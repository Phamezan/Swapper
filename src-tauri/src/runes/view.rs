//! View models for the rune screen, built for both the desktop flyout and the
//! phone remote from the same data.

use super::data;
use super::perks;
use super::probuilds;
use super::stats;
use super::AppliedView;

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
}

/// One pro solo-queue game, ready to render and import.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProBuildView {
    pub match_id: i64,
    pub pro_name: String,
    pub team: String,
    pub league: String,
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
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierOption {
    pub value: String,
    pub label: String,
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
    pub can_apply: bool,
    pub locked: bool,
    /// The active op.gg rank bracket slug and its label.
    pub tier: String,
    pub tier_label: String,
    /// The selectable brackets, in a fixed order.
    pub tiers: Vec<TierOption>,
    /// Whether op.gg honours `tier` for this mode.
    pub tier_supported: bool,
    /// True when the active bracket had no data but a broader one did.
    pub tier_empty: bool,
    /// Total op.gg games behind the preset cards, for the sample-size hint.
    pub games: u64,
}

impl RunesView {
    fn empty(phase: String, message: &str, auto_apply: bool, tier: &str) -> Self {
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
            can_apply: false,
            locked: false,
            tier: super::opgg::normalize_tier(tier).to_string(),
            tier_label: tier_options_label(tier),
            tiers: tier_options(),
            tier_supported: false,
            tier_empty: false,
            games: 0,
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

fn build_trees(catalog: &perks::PerkCatalog, stats: &stats::Aggregate) -> Vec<RuneTreeView> {
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

fn build_shards(catalog: &perks::PerkCatalog, stats: &stats::Aggregate) -> Vec<RuneRowView> {
    catalog
        .shard_rows()
        .into_iter()
        .map(|(label, row)| RuneRowView {
            kind: "statmod".into(),
            label,
            runes: row
                .iter()
                .map(|id| rune_view(*id, catalog.name(*id).unwrap_or("Unknown"), stats, &row))
                .collect(),
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

fn source_label(source: &str) -> &'static str {
    match source {
        "opgg" => "op.gg",
        "lcu" => "League client",
        _ => "Unavailable",
    }
}

/// Builds the rune screen for the current champion select.
pub async fn view(auto_apply: bool, tier: &str) -> RunesView {
    let (phase, context) = match super::rune_context().await {
        Ok(super::RuneContext { phase, context }) => (phase, context),
        Err(error) => {
            return RunesView::empty("Unavailable".into(), error.message(), auto_apply, tier);
        }
    };
    let Some(mut context) = context else {
        let message = if phase == "ChampSelect" {
            "Pick a champion to load runes."
        } else {
            "Not in champion select."
        };
        return RunesView::empty(phase, message, auto_apply, tier);
    };
    let catalog = match data::catalog().await {
        Ok(catalog) => catalog,
        Err(error) => {
            return RunesView::empty(phase, error.message(), auto_apply, tier);
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
            return RunesView::empty(phase, error.message(), auto_apply, tier);
        }
    };
    let loaded = data::load_for(&current, &context, &catalog, tier)
        .await
        .unwrap_or(data::Loaded {
            source: "none",
            groups: Vec::new(),
            selections: Vec::new(),
            tier_empty: false,
        });
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
        })
        .collect();
    let position = super::position_for(&current, &context).await.to_string();
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
        source_label: source_label(loaded.source).to_string(),
        message,
        presets,
        trees: build_trees(&catalog, &aggregates),
        shards: build_shards(&catalog, &aggregates),
        applied,
        auto_apply,
        can_apply,
        locked: context.locked,
        tier,
        tier_label,
        tiers: tier_options(),
        tier_supported: super::opgg::tier_supported(&mode),
        tier_empty,
        games,
    }
}

// ---------------------------------------------------------------------------
// Pro builds
// ---------------------------------------------------------------------------

/// `"16_19"` -> `"16.19"`, the patch label players recognise.
fn patch_label(version: &str) -> String {
    let version = version.trim();
    if version.is_empty() {
        "–".to_string()
    } else {
        version.replace('_', ".")
    }
}

/// A short "how long ago" string from millisecond timestamps.
fn played_ago(now_ms: i64, at_ms: i64) -> String {
    let seconds = ((now_ms - at_ms).max(0)) / 1000;
    if seconds < 60 {
        "just now".to_string()
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

fn pro_build_view(matched: &probuilds::ProMatch, now: i64) -> Option<ProBuildView> {
    let selection = matched.selection()?;
    let team = if matched.pro_info.current_team.trim().is_empty() {
        matched.current_team.trim()
    } else {
        matched.pro_info.current_team.trim()
    };
    Some(ProBuildView {
        match_id: matched.match_id,
        pro_name: matched.display_name().to_string(),
        team: team.to_string(),
        league: matched.pro_league.clone(),
        role_label: probuilds::role_label(&matched.calculated_role).to_string(),
        win: matched.win,
        played_ago: played_ago(now, matched.match_timestamp),
        patch: patch_label(&matched.version),
        keystone: selection.keystone,
        primary_page_id: selection.primary_page_id,
        secondary_page_id: selection.secondary_page_id,
        primary_runes: selection.primary_runes,
        secondary_runes: selection.secondary_runes,
        shards: selection.shards,
        kills: matched.total_kills,
        deaths: matched.total_deaths,
        assists: matched.total_assists,
    })
}

fn unavailable_pro_builds(champion_id: i64, position: &str, role: &str, page: u32, message: &str) -> ProBuildsView {
    ProBuildsView {
        champion_id,
        position: position.to_string(),
        role: role.to_string(),
        page,
        has_more: false,
        matches: Vec::new(),
        unavailable: true,
        message: Some(message.to_string()),
    }
}

/// Builds the Pro builds tab for a champion and role. The frontend calls this
/// once when the tab opens and again for each explicit "load more".
pub async fn pro_builds_view(champion_id: i64, position: &str, page: u32) -> ProBuildsView {
    let role = probuilds::role_arg(position).unwrap_or("all");
    let page = page.max(1);
    if champion_id <= 0 {
        return ProBuildsView {
            champion_id,
            position: position.to_string(),
            role: role.to_string(),
            page,
            has_more: false,
            matches: Vec::new(),
            unavailable: false,
            message: Some("Pick a champion to load pro builds.".to_string()),
        };
    }
    match data::pro_builds(champion_id, position, page).await {
        Ok(matches) => {
            let now = now_ms();
            let has_more = matches.len() >= probuilds::PAGE_SIZE;
            let views: Vec<ProBuildView> =
                matches.iter().filter_map(|entry| pro_build_view(entry, now)).collect();
            ProBuildsView {
                champion_id,
                position: position.to_string(),
                role: role.to_string(),
                page,
                has_more,
                matches: views,
                unavailable: false,
                message: None,
            }
        }
        Err(_) => unavailable_pro_builds(
            champion_id,
            position,
            role,
            page,
            "Pro builds unavailable.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_patches_from_the_api_version_string() {
        assert_eq!(patch_label("16_19"), "16.19");
        assert_eq!(patch_label(""), "–");
    }

    #[test]
    fn formats_how_long_ago_a_match_was_played() {
        let now = 1_000_000_000_000;
        assert_eq!(played_ago(now, now - 30_000), "just now");
        assert_eq!(played_ago(now, now - 12 * 60_000), "12m ago");
        assert_eq!(played_ago(now, now - 3 * 3_600_000), "3h ago");
        assert_eq!(played_ago(now, now - 2 * 86_400_000), "2d ago");
        // A clock skew in the future must not produce a negative duration.
        assert_eq!(played_ago(now, now + 5_000), "just now");
    }
}
