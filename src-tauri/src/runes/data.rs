//! Loading rune data: op.gg statistics with the League client as a fallback,
//! the rune catalog, champion names, and rune icons. Everything is cached so a
//! locked champion select stays cheap.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::opgg;
use super::perks;
use super::session;
use super::{
    position_for, region_for, CATALOG_TTL, CHAMPIONS_PATH, GROUP_TTL, Lcu, PERKS_PATH,
    PERKSTYLES_PATH, RuneError,
};

/// Failed or empty op.gg lookups are cached for this long so a locked champion
/// select does not hit op.gg on every poll.
const GROUP_FAILURE_TTL: Duration = Duration::from_secs(60);

pub async fn catalog() -> Result<perks::PerkCatalog, RuneError> {
    if let Some((at, catalog)) = super::shared().catalog.as_ref() {
        if at.elapsed() < CATALOG_TTL {
            return Ok(catalog.clone());
        }
    }
    let lcu = super::lcu().await?;
    let perks_json = super::lcu_get_text(&lcu, PERKS_PATH).await?;
    let styles_json = super::lcu_get_text(&lcu, PERKSTYLES_PATH).await?;
    let catalog = perks::PerkCatalog::new(
        perks::parse_perks(&perks_json)?,
        perks::parse_styles(&styles_json)?,
    );
    let mut state = super::shared();
    state.catalog = Some((Instant::now(), catalog.clone()));
    Ok(catalog)
}

pub async fn champion_names() -> Result<HashMap<i64, String>, RuneError> {
    if let Some((at, names)) = super::shared().names.as_ref() {
        if at.elapsed() < CATALOG_TTL {
            return Ok(names.clone());
        }
    }
    let lcu = super::lcu().await?;
    let text = super::lcu_get_text(&lcu, CHAMPIONS_PATH).await?;
    let names = session::parse_champion_summary(&text)?;
    let mut state = super::shared();
    state.names = Some((Instant::now(), names.clone()));
    Ok(names)
}

/// op.gg rune-page groups for a champion and role, cached for the session.
///
/// A failure or an empty answer is remembered for [`GROUP_FAILURE_TTL`] and
/// returned as an error, so the caller falls back to the League client without
/// retrying op.gg on every watcher poll.
pub async fn rune_groups(
    region: &str,
    mode: &str,
    champion_id: i64,
    position: &str,
) -> Result<Vec<opgg::RunePageGroup>, RuneError> {
    let key = format!("{region}|{mode}|{champion_id}|{position}");
    {
        let state = super::shared();
        if let Some((at, groups)) = state.groups.get(&key) {
            if at.elapsed() < GROUP_TTL {
                return Ok(groups.clone());
            }
        }
        if let Some(at) = state.group_failures.get(&key) {
            if at.elapsed() < GROUP_FAILURE_TTL {
                return Err(RuneError::unavailable(
                    "op.gg data is temporarily unavailable.",
                ));
            }
        }
    }
    let client = opgg::OpggClient::new()?;
    match client.rune_pages(region, mode, champion_id, position).await {
        Ok(groups) if !groups.is_empty() => {
            let mut state = super::shared();
            state.group_failures.remove(&key);
            state.groups.insert(key, (Instant::now(), groups.clone()));
            Ok(groups)
        }
        Ok(_) => {
            super::shared().group_failures.insert(key, Instant::now());
            Ok(Vec::new())
        }
        Err(error) => {
            super::shared().group_failures.insert(key, Instant::now());
            Err(error)
        }
    }
}

async fn lcu_recommended(
    current: &Lcu,
    context: &session::ChampSelectContext,
) -> Result<Vec<perks::RecommendedPage>, RuneError> {
    let position = if context.assigned_position.trim().is_empty() {
        opgg::POSITION_NONE
    } else {
        context.assigned_position.trim()
    };
    let path = format!(
        "/lol-perks/v1/recommended-pages/champion/{}/position/{}/map/{}",
        context.champion_id, position, context.map_id
    );
    let text = super::lcu_get_text(current, &path).await?;
    perks::parse_recommended_pages(&text)
}

pub struct LoadedPreset {
    pub title: String,
    pub play: u64,
    pub win_pct: Option<f64>,
    pub selection: super::RuneSelection,
}

pub struct Loaded {
    pub source: &'static str,
    /// The op.gg groups the statistics were aggregated from (empty for the
    /// League fallback).
    pub groups: Vec<opgg::RunePageGroup>,
    pub selections: Vec<LoadedPreset>,
}

pub async fn load_for(
    current: &Lcu,
    context: &session::ChampSelectContext,
    catalog: &perks::PerkCatalog,
) -> Result<Loaded, RuneError> {
    let region = region_for(current).await;
    let position = position_for(current, context).await;
    let mode = context.mode();
    if let Ok(groups) = rune_groups(&region, mode, context.champion_id, position).await {
        let presets = opgg::presets(&groups);
        if !presets.is_empty() {
            let selections = presets
                .iter()
                .map(|preset| LoadedPreset {
                    title: catalog
                        .name(preset.keystone)
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("Page {}", preset.index + 1)),
                    play: preset.play,
                    win_pct: preset.win_pct,
                    selection: super::RuneSelection {
                        primary_page_id: preset.primary_page_id,
                        secondary_page_id: preset.secondary_page_id,
                        keystone: preset.keystone,
                        primary_runes: preset.primary_runes.clone(),
                        secondary_runes: preset.secondary_runes.clone(),
                        shards: preset.shards.clone(),
                    },
                })
                .collect();
            return Ok(Loaded {
                source: "opgg",
                groups,
                selections,
            });
        }
    }
    let recommended = lcu_recommended(current, context).await.unwrap_or_default();
    let selections = recommended
        .iter()
        .enumerate()
        .filter_map(|(index, page)| {
            page.selection().map(|selection| LoadedPreset {
                title: if page.name.trim().is_empty() {
                    format!("Recommended {}", index + 1)
                } else {
                    page.name.clone()
                },
                play: 0,
                win_pct: page
                    .win_rate
                    .map(|rate| if rate <= 1.0 { rate * 100.0 } else { rate }),
                selection,
            })
        })
        .collect();
    Ok(Loaded {
        source: if recommended.is_empty() { "none" } else { "lcu" },
        groups: Vec::new(),
        selections,
    })
}

/// Returns the PNG bytes for a rune, keystone, tree, or shard icon id.
pub async fn icon(id: i64) -> Result<Vec<u8>, RuneError> {
    if let Some(bytes) = super::shared().icons.get(&id) {
        return Ok(bytes.clone());
    }
    let catalog = catalog().await?;
    let path = catalog
        .asset_path(id)
        .ok_or_else(|| RuneError::not_found("Unknown rune."))?;
    let lcu = super::lcu().await?;
    let bytes = super::lcu_get_bytes(&lcu, &path).await?;
    super::shared().icons.insert(id, bytes.clone());
    Ok(bytes)
}
