//! Loading rune data: op.gg statistics with the League client as a fallback,
//! the rune catalog, champion names, and rune icons. Everything is cached so a
//! locked champion select stays cheap.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::cache::{self, LastGoodCache};
use super::lolalytics;
use super::matchup::Matchup;
use super::opgg;
use super::perks;
use super::probuilds;
use super::provider::{DataKind, Provider, ProviderError, Sourced};
use super::session;
use super::{
    region_for, CATALOG_TTL, CHAMPIONS_PATH, GROUP_TTL, Lcu, PERKS_PATH,
    PERKSTYLES_PATH, RuneError,
};

/// Failed or empty op.gg lookups are cached for this long so a locked champion
/// select does not hit op.gg on every poll.
const GROUP_FAILURE_TTL: Duration = Duration::from_secs(60);
/// Bounds for each provider's persisted last-successful cache, so it survives
/// restarts without growing without limit.
const LAST_GOOD_MAX_ENTRIES: usize = 128;
const LAST_GOOD_MAX_BYTES: usize = 1024 * 1024;

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

pub(super) fn keystone_fetches() -> &'static tokio::sync::Semaphore {
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

pub(super) fn keystone_gate(key: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    KEYSTONE_FLIGHTS
        .get_or_init(super::Flights::new)
        .gate(&key.to_string())
}

/// The last successful result per kind, persisted under the Swapper data
/// folder. A failed fetch is served from this cache, labelled stale.
static RUNE_LAST_GOOD: OnceLock<LastGoodCache<opgg::ChampionData>> = OnceLock::new();
static BUILD_LAST_GOOD: OnceLock<LastGoodCache<lolalytics::KeystoneBuild>> = OnceLock::new();
static MATCHUP_LAST_GOOD: OnceLock<LastGoodCache<Matchup>> = OnceLock::new();
static PRO_LAST_GOOD: OnceLock<LastGoodCache<Vec<probuilds::ProMatch>>> = OnceLock::new();

fn rune_last_good() -> &'static LastGoodCache<opgg::ChampionData> {
    RUNE_LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::RuneRecommendations),
            GROUP_TTL,
            LAST_GOOD_MAX_ENTRIES,
            LAST_GOOD_MAX_BYTES,
        )
    })
}

fn build_last_good() -> &'static LastGoodCache<lolalytics::KeystoneBuild> {
    BUILD_LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::ItemBuild),
            lolalytics::SUCCESS_TTL,
            LAST_GOOD_MAX_ENTRIES,
            LAST_GOOD_MAX_BYTES,
        )
    })
}

fn matchup_last_good() -> &'static LastGoodCache<Matchup> {
    MATCHUP_LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::Matchup),
            lolalytics::SUCCESS_TTL,
            LAST_GOOD_MAX_ENTRIES,
            LAST_GOOD_MAX_BYTES,
        )
    })
}

fn pro_last_good() -> &'static LastGoodCache<Vec<probuilds::ProMatch>> {
    PRO_LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::ProBuilds),
            probuilds::SUCCESS_TTL,
            LAST_GOOD_MAX_ENTRIES,
            LAST_GOOD_MAX_BYTES,
        )
    })
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
/// cached for the session and, on success, persisted per request key.
///
/// A failure or an empty answer is remembered for [`GROUP_FAILURE_TTL`] and, if
/// a previous result is on disk, that result is served stale instead of an
/// error. With no cached result the classified error is returned, so the caller
/// falls back to the League client without retrying op.gg on every watcher poll.
pub async fn champion_data(
    region: &str,
    mode: &str,
    champion_id: i64,
    position: &str,
    tier: &str,
) -> Result<Sourced<opgg::ChampionData>, RuneError> {
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
    let client = match opgg::OpggClient::new() {
        Ok(client) => client,
        Err(error) => {
            super::shared()
                .group_failures
                .insert(key.clone(), (Instant::now(), super::GroupMiss::Failed));
            return group_stale_or_error(&key, error);
        }
    };
    match client
        .champion_data(region, mode, champion_id, position, tier)
        .await
    {
        Ok(data) if !data.rune_pages.is_empty() => {
            {
                let mut state = super::shared();
                state.group_failures.remove(&key);
                state.groups.insert(key.clone(), (Instant::now(), data.clone()));
            }
            rune_last_good().store(key, data.clone());
            Ok(Sourced::fresh(data, Provider::Opgg))
        }
        Ok(_) => {
            super::shared()
                .group_failures
                .insert(key, (Instant::now(), super::GroupMiss::Empty));
            Ok(Sourced::fresh(opgg::ChampionData::default(), Provider::Opgg))
        }
        Err(error) => {
            super::shared()
                .group_failures
                .insert(key.clone(), (Instant::now(), super::GroupMiss::Failed));
            group_stale_or_error(&key, error)
        }
    }
}

/// A cached op.gg result for a group key, if one is still fresh. A remembered
/// empty answer replays as an empty result; a remembered failure returns the
/// persisted last-good result when there is one, otherwise the error.
fn cached_group(key: &str) -> Option<Result<Sourced<opgg::ChampionData>, RuneError>> {
    let miss = {
        let state = super::shared();
        if let Some((at, data)) = state.groups.get(key) {
            if at.elapsed() < GROUP_TTL {
                return Some(Ok(Sourced {
                    value: data.clone(),
                    provider: Provider::Opgg,
                    stale: false,
                    fetched_at: None,
                }));
            }
        }
        state
            .group_failures
            .get(key)
            .filter(|(at, _)| at.elapsed() < GROUP_FAILURE_TTL)
            .map(|(_, kind)| *kind)
    };
    match miss {
        // A remembered empty answer must replay exactly like the original empty
        // answer, so the caller's bracket probe and League fallback behave the
        // same on the first visit and on later ones.
        Some(super::GroupMiss::Empty) => Some(Ok(Sourced::fresh(
            opgg::ChampionData::default(),
            Provider::Opgg,
        ))),
        Some(super::GroupMiss::Failed) => {
            Some(group_stale_or_error(key, ProviderError::Unavailable))
        }
        None => None,
    }
}

/// The persisted last-good op.gg result for a key, or the classified error.
fn group_stale_or_error(
    key: &str,
    error: ProviderError,
) -> Result<Sourced<opgg::ChampionData>, RuneError> {
    match rune_last_good().lookup(key) {
        Some(cached) if !cached.fresh => Ok(Sourced::stale(
            cached.value,
            Provider::Opgg,
            cached.fetched_at,
        )),
        Some(cached) => Ok(Sourced {
            value: cached.value,
            provider: Provider::Opgg,
            stale: false,
            fetched_at: Some(cached.fetched_at),
        }),
        None => Err(RuneError::provider(error, DataKind::RuneRecommendations)),
    }
}

/// pros' solo-queue games for a champion and role, cached for the session and,
/// on success, persisted per request key.
///
/// The API pages 20 games at a time; `page` is 1-based. Successes are cached
/// for [`probuilds::SUCCESS_TTL`] and failures for [`probuilds::FAILURE_TTL`],
/// so a locked champion select never polls the endpoint. A failure serves the
/// persisted last-good page when there is one. Concurrent misses for the same
/// page wait on one request.
pub async fn pro_builds(
    champion_id: i64,
    position: &str,
    page: u32,
) -> Result<Sourced<Vec<probuilds::ProMatch>>, RuneError> {
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
    let client = match probuilds::ProBuildsClient::new() {
        Ok(client) => client,
        Err(error) => {
            super::shared().pro_builds.fail(key.clone());
            return pro_stale_or_error(&key, error);
        }
    };
    match client.matches(champion_id, role, page, false).await {
        Ok(matches) => {
            super::shared().pro_builds.store(key.clone(), matches.clone());
            pro_last_good().store(key, matches.clone());
            Ok(Sourced::fresh(matches, Provider::Ugg))
        }
        Err(error) => {
            super::shared().pro_builds.fail(key.clone());
            pro_stale_or_error(&key, error)
        }
    }
}

/// A cached pro-build lookup for a page. A remembered failure returns the
/// persisted last-good page when there is one.
fn cached_pro_builds(key: &str) -> Option<Result<Sourced<Vec<probuilds::ProMatch>>, RuneError>> {
    match super::shared().pro_builds.lookup(key) {
        probuilds::Cached::Fresh(matches) => Some(Ok(Sourced {
            value: matches,
            provider: Provider::Ugg,
            stale: false,
            fetched_at: None,
        })),
        probuilds::Cached::Unavailable => {
            Some(pro_stale_or_error(key, ProviderError::Unavailable))
        }
        probuilds::Cached::Miss => None,
    }
}

/// The persisted last-good pro-build page for a key, or the classified error.
fn pro_stale_or_error(
    key: &str,
    error: ProviderError,
) -> Result<Sourced<Vec<probuilds::ProMatch>>, RuneError> {
    match pro_last_good().lookup(key) {
        Some(cached) if !cached.fresh => Ok(Sourced::stale(
            cached.value,
            Provider::Ugg,
            cached.fetched_at,
        )),
        Some(cached) => Ok(Sourced {
            value: cached.value,
            provider: Provider::Ugg,
            stale: false,
            fetched_at: Some(cached.fetched_at),
        }),
        None => Err(RuneError::provider(error, DataKind::ProBuilds)),
    }
}

/// The 6-item build for one keystone on lolalytics, cached for the session.
///
/// `Ok(None)` means the page carried no build for that champion/lane/bracket/
/// keystone. A failure or an empty page is negative-cached for
/// [`lolalytics::FAILURE_TTL`] so a locked champion select does not fetch the
/// same page repeatedly. Concurrent misses for the same filter wait on one
/// request. A failed fetch serves the persisted last-good build when there is
/// one; with no cached build the card simply hides, as before.
pub async fn keystone_build(
    champion_slug: &str,
    lane: &str,
    tier: &str,
    keystone: i64,
) -> Result<Option<Sourced<lolalytics::KeystoneBuild>>, RuneError> {
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
    let client = match lolalytics::LolalyticsClient::new() {
        Ok(client) => client,
        Err(_) => return Ok(keystone_stale(&key)),
    };
    match client.build(champion_slug, lane, tier, keystone).await {
        Ok(Some(build)) => {
            super::shared().lolalytics.store(key.clone(), build.clone());
            build_last_good().store(key, build.clone());
            Ok(Some(Sourced::fresh(build, Provider::Lolalytics)))
        }
        Ok(None) => {
            super::shared().lolalytics.fail(key);
            Ok(None)
        }
        Err(_) => Ok(keystone_stale(&key)),
    }
}

/// A cached keystone build for a filter key, if one is still fresh. A
/// remembered failure returns the persisted last-good build when there is one.
fn cached_keystone_build(
    key: &str,
) -> Option<Result<Option<Sourced<lolalytics::KeystoneBuild>>, RuneError>> {
    match super::shared().lolalytics.lookup(key) {
        lolalytics::Cached::Fresh(build) => Some(Ok(Some(Sourced {
            value: build,
            provider: Provider::Lolalytics,
            stale: false,
            fetched_at: None,
        }))),
        lolalytics::Cached::Unavailable => Some(Ok(keystone_stale(key))),
        lolalytics::Cached::Miss => None,
    }
}

/// The persisted last-good build for a key, marked stale. `None` when there is
/// nothing cached, so the card hides rather than showing an error.
fn keystone_stale(key: &str) -> Option<Sourced<lolalytics::KeystoneBuild>> {
    build_last_good().lookup(key).map(|cached| {
        if cached.fresh {
            Sourced {
                value: cached.value,
                provider: Provider::Lolalytics,
                stale: false,
                fetched_at: Some(cached.fetched_at),
            }
        } else {
            Sourced::stale(cached.value, Provider::Lolalytics, cached.fetched_at)
        }
    })
}

/// The lane matchup build for a champion against one enemy, cached for the
/// session like [`keystone_build`]: `Ok(None)` means lolalytics had no page for
/// that matchup, failures are negative-cached, concurrent misses share one
/// request, and a failed fetch serves the persisted last-good matchup. The
/// caller decides whether the sample is large enough to use.
pub async fn matchup(
    champion_slug: &str,
    enemy_slug: &str,
    champion_id: i64,
    enemy_id: i64,
    lane: &str,
    tier: &str,
) -> Result<Option<Sourced<Matchup>>, RuneError> {
    let key = lolalytics::vs_cache_key(champion_slug, enemy_slug, lane, tier);
    if let Some(cached) = cached_matchup(&key) {
        return cached;
    }
    let gate = keystone_gate(&key);
    let _guard = gate.lock().await;
    if let Some(cached) = cached_matchup(&key) {
        return cached;
    }
    let _permit = keystone_fetches()
        .acquire()
        .await
        .expect("the keystone fetch semaphore is never closed");
    let client = match lolalytics::LolalyticsClient::new() {
        Ok(client) => client,
        Err(_) => return Ok(matchup_stale(&key)),
    };
    match client
        .matchup(champion_slug, enemy_slug, champion_id, enemy_id, lane, tier)
        .await
    {
        Ok(Some(found)) => {
            super::shared().matchups.store(key.clone(), found.clone());
            matchup_last_good().store(key, found.clone());
            Ok(Some(Sourced::fresh(found, Provider::Lolalytics)))
        }
        Ok(None) => {
            super::shared().matchups.fail(key);
            Ok(None)
        }
        Err(_) => {
            super::shared().matchups.fail(key.clone());
            Ok(matchup_stale(&key))
        }
    }
}

fn cached_matchup(key: &str) -> Option<Result<Option<Sourced<Matchup>>, RuneError>> {
    match super::shared().matchups.lookup(key) {
        lolalytics::Cached::Fresh(found) => Some(Ok(Some(Sourced {
            value: found,
            provider: Provider::Lolalytics,
            stale: false,
            fetched_at: None,
        }))),
        lolalytics::Cached::Unavailable => Some(Ok(matchup_stale(key))),
        lolalytics::Cached::Miss => None,
    }
}

/// The persisted last-good matchup for a key, marked stale.
fn matchup_stale(key: &str) -> Option<Sourced<Matchup>> {
    matchup_last_good().lookup(key).map(|cached| {
        if cached.fresh {
            Sourced {
                value: cached.value,
                provider: Provider::Lolalytics,
                stale: false,
                fetched_at: Some(cached.fetched_at),
            }
        } else {
            Sourced::stale(cached.value, Provider::Lolalytics, cached.fetched_at)
        }
    })
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
    /// The provider the presets came from, used to label the source.
    pub provider: Provider,
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
    /// True when the presets are the last successful op.gg result, served
    /// because the live fetch failed.
    pub stale: bool,
    /// Unix milliseconds of that last successful fetch, when known.
    pub fetched_at: Option<i64>,
}

/// Loads the recommendation set for a champion select.
///
/// op.gg is the source of record; when it answers with nothing, the League
/// client's own recommended pages are the fallback. That is the only genuine
/// substitute Swapper has: item builds (lolalytics) and pro games
/// (probuildstats/u.gg) carry data shapes and meanings the other sources cannot
/// produce, so no cross-kind fallback is attempted.
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
    if let Ok(sourced) = champion_data(&region, mode, context.champion_id, position, tier).await {
        let Sourced {
            value: data,
            provider,
            stale,
            fetched_at,
        } = sourced;
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
                provider,
                groups: data.rune_pages,
                selections,
                spell_pair,
                tier_empty: false,
                stale,
                fetched_at,
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
            .map(|sourced| sourced.value)
            .unwrap_or_default();
            if !broad.rune_pages.is_empty() {
                return Ok(Loaded {
                    source: "none",
                    provider,
                    groups: Vec::new(),
                    selections: Vec::new(),
                    spell_pair: None,
                    tier_empty: true,
                    stale: false,
                    fetched_at: None,
                });
            }
        }
    }
    let recommended = lcu_recommended(current, context, position).await.unwrap_or_default();
    let selections: Vec<LoadedPreset> = recommended
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
        // The League fallback is only the source when it actually produced a
        // usable preset. Pages that parse but cannot form a selection leave the
        // list empty, and then there is simply no data to show or badge.
        source: if selections.is_empty() { "none" } else { "lcu" },
        provider: Provider::LeagueClient,
        groups: Vec::new(),
        selections,
        spell_pair: None,
        tier_empty: false,
        // The League client answered live; this is not cached op.gg data.
        stale: false,
        fetched_at: None,
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

    #[test]
    fn a_fresh_in_memory_group_is_served_without_a_live_fetch() {
        let key = group_key("euw", "ranked", 999, "mid", "emerald_plus");
        crate::runes::shared()
            .groups
            .insert(key.clone(), (Instant::now(), opgg::ChampionData::default()));
        let cached = cached_group(&key).expect("a fresh hit");
        let sourced = cached.expect("a fresh hit is not an error");
        assert!(!sourced.stale);
    }

    #[test]
    fn a_remembered_empty_group_replays_as_empty_not_an_error() {
        // op.gg answering with no rune pages is remembered as an empty answer.
        // It must replay as an empty result, exactly like the first visit, so
        // the caller still probes the broader bracket instead of treating the
        // champion and role as an error and dropping straight to the fallback.
        let key = group_key("euw", "ranked", 999, "jungle", "emerald_plus");
        crate::runes::shared().group_failures.insert(
            key.clone(),
            (Instant::now(), crate::runes::GroupMiss::Empty),
        );
        let cached = cached_group(&key).expect("a remembered miss is served from the cache");
        let sourced = cached.expect("an empty answer must not replay as an error");
        assert!(sourced.value.rune_pages.is_empty());
    }
}
