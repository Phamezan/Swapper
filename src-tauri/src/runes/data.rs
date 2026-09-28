//! Loading rune data: op.gg statistics with the League client as a fallback,
//! the rune catalog, champion names, and rune icons. Everything is cached so a
//! locked champion select stays cheap.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::lolalytics;
use super::opgg;
use super::perks;
use super::probuilds;
use super::session;
use super::{
    region_for, CATALOG_TTL, CHAMPIONS_PATH, GROUP_TTL, Lcu, PERKS_PATH,
    PERKSTYLES_PATH, RuneError,
};

/// Failed or empty op.gg lookups are cached for this long so a locked champion
/// select does not hit op.gg on every poll.
const GROUP_FAILURE_TTL: Duration = Duration::from_secs(60);

/// Serializes rune-catalog loads and champion-name loads so concurrent misses
/// fetch each file once.
static CATALOG_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static NAMES_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
/// Keyed gates for op.gg champion data, pro builds, per-rune icons and
/// per-keystone lolalytics builds.
static GROUP_FLIGHTS: OnceLock<super::Flights<String>> = OnceLock::new();
static PRO_BUILDS_FLIGHTS: OnceLock<super::Flights<String>> = OnceLock::new();
static RUNE_ICON_FLIGHTS: OnceLock<super::Flights<i64>> = OnceLock::new();
static KEYSTONE_FLIGHTS: OnceLock<super::Flights<String>> = OnceLock::new();
/// Bounds how many lolalytics pages are fetched at once. A preset list has a
/// handful of keystones and each page is a few hundred KB, so this keeps the
/// warm-up and on-demand loads polite without serializing them.
static KEYSTONE_FETCHES: OnceLock<tokio::sync::Semaphore> = OnceLock::new();

fn keystone_fetches() -> &'static tokio::sync::Semaphore {
    KEYSTONE_FETCHES.get_or_init(|| tokio::sync::Semaphore::new(3))
}

fn catalog_lock() -> &'static tokio::sync::Mutex<()> {
    CATALOG_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn names_lock() -> &'static tokio::sync::Mutex<()> {
    NAMES_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn group_gate(key: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    GROUP_FLIGHTS
        .get_or_init(super::Flights::new)
        .gate(&key.to_string())
}

fn pro_build_gate(key: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    PRO_BUILDS_FLIGHTS
        .get_or_init(super::Flights::new)
        .gate(&key.to_string())
}

fn rune_icon_gate(id: i64) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    RUNE_ICON_FLIGHTS.get_or_init(super::Flights::new).gate(&id)
}

fn keystone_gate(key: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    KEYSTONE_FLIGHTS
        .get_or_init(super::Flights::new)
        .gate(&key.to_string())
}

pub async fn catalog() -> Result<perks::PerkCatalog, RuneError> {
    if let Some(catalog) = cached_catalog() {
        return Ok(catalog);
    }
    let _guard = catalog_lock().lock().await;
    if let Some(catalog) = cached_catalog() {
        return Ok(catalog);
    }
    let lcu = super::lcu().await?;
    let perks_json = super::lcu_get_text(&lcu, PERKS_PATH).await?;
    let styles_json = super::lcu_get_text(&lcu, PERKSTYLES_PATH).await?;
    let catalog = perks::PerkCatalog::new(
        perks::parse_perks(&perks_json)?,
        perks::parse_styles(&styles_json)?,
    );
    super::shared().catalog = Some((Instant::now(), catalog.clone()));
    Ok(catalog)
}

fn cached_catalog() -> Option<perks::PerkCatalog> {
    super::shared()
        .catalog
        .as_ref()
        .filter(|(at, _)| at.elapsed() < CATALOG_TTL)
        .map(|(_, catalog)| catalog.clone())
}

pub async fn champion_names() -> Result<HashMap<i64, String>, RuneError> {
    if let Some(names) = cached_champion_names() {
        return Ok(names);
    }
    let _guard = names_lock().lock().await;
    if let Some(names) = cached_champion_names() {
        return Ok(names);
    }
    let lcu = super::lcu().await?;
    let text = super::lcu_get_text(&lcu, CHAMPIONS_PATH).await?;
    let names = session::parse_champion_summary(&text)?;
    super::shared().names = Some((Instant::now(), names.clone()));
    Ok(names)
}

fn cached_champion_names() -> Option<HashMap<i64, String>> {
    super::shared()
        .names
        .as_ref()
        .filter(|(at, _)| at.elapsed() < CATALOG_TTL)
        .map(|(_, names)| names.clone())
}

/// The session cache key for an op.gg lookup. The rank bracket is part of the
/// key so switching tiers never serves another bracket's data.
pub fn group_key(region: &str, mode: &str, champion_id: i64, position: &str, tier: &str) -> String {
    format!("{region}|{mode}|{champion_id}|{position}|{tier}")
}

/// op.gg champion data (rune pages and spell pairs) for a champion and role,
/// cached for the session.
///
/// A failure or an empty answer is remembered for [`GROUP_FAILURE_TTL`] and
/// returned as an error, so the caller falls back to the League client without
/// retrying op.gg on every watcher poll.
pub async fn champion_data(
    region: &str,
    mode: &str,
    champion_id: i64,
    position: &str,
    tier: &str,
) -> Result<opgg::ChampionData, RuneError> {
    let tier = opgg::normalize_tier(tier);
    let key = group_key(region, mode, champion_id, position, tier);
    if let Some(result) = cached_group(&key) {
        return result;
    }
    let gate = group_gate(&key);
    let _guard = gate.lock().await;
    if let Some(result) = cached_group(&key) {
        return result;
    }
    let client = opgg::OpggClient::new()?;
    match client
        .champion_data(region, mode, champion_id, position, tier)
        .await
    {
        Ok(data) if !data.rune_pages.is_empty() => {
            let mut state = super::shared();
            state.group_failures.remove(&key);
            state.groups.insert(key, (Instant::now(), data.clone()));
            Ok(data)
        }
        Ok(_) => {
            super::shared().group_failures.insert(key, Instant::now());
            Ok(opgg::ChampionData::default())
        }
        Err(error) => {
            super::shared().group_failures.insert(key, Instant::now());
            Err(error)
        }
    }
}

/// A cached op.gg result for a group key, if one is still fresh.
fn cached_group(key: &str) -> Option<Result<opgg::ChampionData, RuneError>> {
    let state = super::shared();
    if let Some((at, data)) = state.groups.get(key) {
        if at.elapsed() < GROUP_TTL {
            return Some(Ok(data.clone()));
        }
    }
    if let Some(at) = state.group_failures.get(key) {
        if at.elapsed() < GROUP_FAILURE_TTL {
            return Some(Err(RuneError::unavailable(
                "op.gg data is temporarily unavailable.",
            )));
        }
    }
    None
}

/// pros' solo-queue games for a champion and role, cached for the session.
///
/// The API pages 20 games at a time; `page` is 1-based. Successes are cached
/// for [`probuilds::SUCCESS_TTL`] and failures for [`probuilds::FAILURE_TTL`],
/// so a locked champion select never polls the endpoint. Concurrent misses for
/// the same page wait on one request.
pub async fn pro_builds(
    champion_id: i64,
    position: &str,
    page: u32,
) -> Result<Vec<probuilds::ProMatch>, RuneError> {
    let role = probuilds::role_arg(position);
    let page = page.max(1);
    let key = probuilds::cache_key(champion_id, role, page);
    if let Some(cached) = cached_pro_builds(&key) {
        return cached;
    }
    let gate = pro_build_gate(&key);
    let _guard = gate.lock().await;
    if let Some(cached) = cached_pro_builds(&key) {
        return cached;
    }
    let client = probuilds::ProBuildsClient::new()?;
    match client.matches(champion_id, role, page, false).await {
        Ok(matches) => {
            super::shared().pro_builds.store(key, matches.clone());
            Ok(matches)
        }
        Err(error) => {
            super::shared().pro_builds.fail(key);
            Err(error)
        }
    }
}

/// A cached pro-build lookup for a page, if one is still fresh.
fn cached_pro_builds(key: &str) -> Option<Result<Vec<probuilds::ProMatch>, RuneError>> {
    match super::shared().pro_builds.lookup(key) {
        probuilds::Cached::Fresh(matches) => Some(Ok(matches)),
        probuilds::Cached::Unavailable => Some(Err(RuneError::unavailable(
            "Pro builds are temporarily unavailable.",
        ))),
        probuilds::Cached::Miss => None,
    }
}

/// The 6-item build for one keystone on lolalytics, cached for the session.
///
/// `Ok(None)` means the page carried no build for that champion/lane/bracket/
/// keystone. A failure or an empty page is negative-cached for
/// [`lolalytics::FAILURE_TTL`] so a locked champion select does not fetch the
/// same page repeatedly. Concurrent misses for the same filter wait on one
/// request.
pub async fn keystone_build(
    champion_slug: &str,
    lane: &str,
    tier: &str,
    keystone: i64,
) -> Result<Option<lolalytics::KeystoneBuild>, RuneError> {
    let key = lolalytics::cache_key(champion_slug, lane, tier, keystone);
    if let Some(cached) = cached_keystone_build(&key) {
        return cached;
    }
    let gate = keystone_gate(&key);
    let _guard = gate.lock().await;
    if let Some(cached) = cached_keystone_build(&key) {
        return cached;
    }
    let _permit = keystone_fetches()
        .acquire()
        .await
        .expect("the keystone fetch semaphore is never closed");
    let client = lolalytics::LolalyticsClient::new()?;
    match client.build(champion_slug, lane, tier, keystone).await {
        Ok(Some(build)) => {
            super::shared().lolalytics.store(key, build.clone());
            Ok(Some(build))
        }
        Ok(None) => {
            super::shared().lolalytics.fail(key);
            Ok(None)
        }
        Err(error) => {
            super::shared().lolalytics.fail(key);
            Err(error)
        }
    }
}

/// A cached keystone build for a filter key, if one is still fresh.
fn cached_keystone_build(
    key: &str,
) -> Option<Result<Option<lolalytics::KeystoneBuild>, RuneError>> {
    match super::shared().lolalytics.lookup(key) {
        lolalytics::Cached::Fresh(build) => Some(Ok(Some(build))),
        lolalytics::Cached::Unavailable => Some(Ok(None)),
        lolalytics::Cached::Miss => None,
    }
}

async fn lcu_recommended(
    current: &Lcu,
    context: &session::ChampSelectContext,
    position: &str,
) -> Result<Vec<perks::RecommendedPage>, RuneError> {
    let lcu_position = match position {
        "mid" => "middle",
        "adc" => "bottom",
        "support" => "utility",
        "top" | "jungle" => position,
        _ if !context.assigned_position.trim().is_empty() => context.assigned_position.trim(),
        _ => opgg::POSITION_NONE,
    };
    let path = format!(
        "/lol-perks/v1/recommended-pages/champion/{}/position/{}/map/{}",
        context.champion_id, lcu_position, context.map_id
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
    /// op.gg's most-played summoner-spell pair, shown on the preset cards and
    /// applied with the page when the setting is on. `None` for the League
    /// fallback, which does not carry spells.
    pub spell_pair: Option<[i64; 2]>,
    /// True when the chosen rank bracket had no op.gg data but a broader bracket
    /// did. The caller shows a "not enough games" state instead of silently
    /// falling back to a different bracket.
    pub tier_empty: bool,
}

pub async fn load_for(
    current: &Lcu,
    context: &session::ChampSelectContext,
    catalog: &perks::PerkCatalog,
    tier: &str,
    position: &str,
) -> Result<Loaded, RuneError> {
    let region = region_for(current).await;
    let mode = context.mode();
    let tier = opgg::normalize_tier(tier);
    if let Ok(data) = champion_data(&region, mode, context.champion_id, position, tier).await {
        let spell_pair = data.top_spell_pair();
        let presets = opgg::presets(&data.rune_pages);
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
                groups: data.rune_pages,
                selections,
                spell_pair,
                tier_empty: false,
            });
        }
        // The chosen bracket had no data. Only skip the fallback when a broader
        // bracket does have data, so the user is told the bracket is too narrow
        // instead of being shown a different bracket's presets.
        if tier != opgg::TIER_ALL {
            let broad = champion_data(
                &region,
                mode,
                context.champion_id,
                position,
                opgg::TIER_ALL,
            )
            .await
            .unwrap_or_default();
            if !broad.rune_pages.is_empty() {
                return Ok(Loaded {
                    source: "none",
                    groups: Vec::new(),
                    selections: Vec::new(),
                    spell_pair: None,
                    tier_empty: true,
                });
            }
        }
    }
    let recommended = lcu_recommended(current, context, position).await.unwrap_or_default();
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
        spell_pair: None,
        tier_empty: false,
    })
}

/// Returns the PNG bytes for a rune, keystone, tree, or shard icon id.
/// Concurrent requests for the same id share one fetch.
pub async fn icon(id: i64) -> Result<Vec<u8>, RuneError> {
    if let Some(bytes) = super::shared().icons.get(&id) {
        return Ok(bytes.clone());
    }
    let gate = rune_icon_gate(id);
    let _guard = gate.lock().await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_key_carries_the_rank_bracket() {
        let emerald = group_key("euw", "ranked", 103, "mid", "emerald_plus");
        let diamond = group_key("euw", "ranked", 103, "mid", "diamond_plus");
        assert_ne!(emerald, diamond);
        assert!(emerald.ends_with("|emerald_plus"));
        assert_eq!(emerald, "euw|ranked|103|mid|emerald_plus");
    }
}
