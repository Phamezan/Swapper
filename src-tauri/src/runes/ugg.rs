//! Champion statistics from u.gg's public, versioned aggregate datasets.
//!
//! The bulk ranking file includes every champion in every role, even off-meta
//! picks. Matchup files include the full table; the ranking file only includes
//! ten opponents and must not be used as a counters table.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::Value;

use super::cache::{self, LastGoodCache};
use super::counters::{self, CounterRow, Counters};
use super::data::{keystone_fetches, keystone_gate};
use super::lolalytics::{self, BuildCache, Cached};
use super::overview::{Overview, OverviewStats};
use super::patch::{PatchInfo, Patches};
use super::health::{self, Source};
use super::provider::{DataKind, Provider, ProviderError, Sourced};
use super::tierlist::{self, TierList, TierRow};
use super::RuneError;

mod merge;

use merge::merge_datasets;

pub const VERSIONS_URL: &str =
    "https://static.bigbrain.gg/assets/lol/riot_patch_update/prod/ugg/ugg-api-versions.json";
const BASE: &str = "https://stats2.u.gg/lol/1.5";
const TIMEOUT: Duration = Duration::from_secs(6);

/// Doctor probes live data without treating a last-good cache as a healthy service.
pub async fn probe() -> Result<(), ProviderError> {
    let client = reqwest::Client::builder()
        .connect_timeout(TIMEOUT)
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| ProviderError::Unavailable)?;
    async fn get(client: &reqwest::Client, url: &str) -> Result<Value, ProviderError> {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| ProviderError::from_reqwest(&e))?;
        if !response.status().is_success() {
            return Err(ProviderError::from_status(response.status()));
        }
        response.json().await.map_err(|_| ProviderError::Format)
    }
    let versions = get(&client, VERSIONS_URL).await?;
    let patches = patch_names(&versions).ok_or(ProviderError::Format)?;
    let url = dataset_url(
        &versions,
        "champion_ranking",
        &patches.current,
        "emerald_plus",
        None,
    )
    .ok_or(ProviderError::Format)?;
    let body = get(&client, &url).await?;
    parse_tierlist(&body, "top").ok_or(ProviderError::Format)?;
    Ok(())
}

/// One upstream bucket's (URL slug, numeric rank). Gold+ is composed in `dataset`
/// from the disjoint Gold and Platinum+ buckets rather than a single endpoint.
pub fn rank(tier: &str) -> Option<(&'static str, &'static str)> {
    if tier == "gold" {
        return Some(("gold", "5"));
    }
    match lolalytics::tier_slug(tier) {
        "all" => Some(("overall", "8")),
        "platinum_plus" => Some(("platinum_plus", "10")),
        "emerald_plus" => Some(("emerald_plus", "17")),
        "diamond_plus" => Some(("diamond_plus", "11")),
        "master_plus" => Some(("master_plus", "14")),
        "challenger" => Some(("challenger", "1")),
        _ => None,
    }
}

/// (bulk file key, numeric role), accepting Swapper and lolalytics lane names.
fn role(lane: &str) -> Option<(&'static str, &'static str)> {
    match lolalytics::lane(lane)? {
        "top" => Some(("top", "4")),
        "jungle" => Some(("jungle", "1")),
        "middle" => Some(("mid", "5")),
        "bottom" => Some(("adc", "3")),
        "support" => Some(("supp", "2")),
        _ => None,
    }
}

fn counts(wins: &Value, games: &Value) -> Option<(f64, u64)> {
    let (wins, games) = (wins.as_u64()?, games.as_u64()?);
    (games > 0 && wins <= games).then_some((100.0 * wins as f64 / games as f64, games))
}

pub(super) fn parse_tierlist(root: &Value, lane: &str) -> Option<TierList> {
    let rows = root.get(0)?.get(role(lane)?.0)?.as_array()?;
    let bans = root.get(1)?;
    // Deliberately different denominators, verified against the website:
    // 106575 / 1662713 = 6.4% pick; 114076 / 1534032 = 7.4% ban (Aatrox).
    let pick_total = root
        .get(3)?
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)?;
    let ban_total = bans
        .get("total_matches")?
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)?;
    let rows: Vec<TierRow> = rows
        .iter()
        .filter_map(|row| {
            let id = row.get(0)?.as_str()?;
            let champion_id = id.parse::<i64>().ok().filter(|id| *id > 0)?;
            let (win_pct, games) = counts(row.get(2)?, row.get(3)?)?;
            let ban_count = bans.get(id)?.as_u64()?;
            if games as f64 > pick_total || ban_count as f64 > ban_total {
                return None;
            }
            Some(TierRow {
                champion_id,
                win_pct,
                games,
                pick_pct: 100.0 * games as f64 / pick_total,
                ban_pct: 100.0 * ban_count as f64 / ban_total,
                ..TierRow::default()
            })
        })
        .collect();
    (!rows.is_empty()).then_some(TierList { rows, patch: None })
}

/// Only tier-list placement uses role share. Overview and opponent baselines
/// keep the unfiltered counts, including a champion's rarely played roles.
pub(super) fn parse_lane_tierlist(root: &Value, lane: &str) -> Option<TierList> {
    let mut list = parse_tierlist(root, lane)?;
    list.rows.retain(|row| {
        let games: u64 = ["top", "jungle", "mid", "adc", "supp"]
            .iter()
            .filter_map(|role| {
                root.get(0)?
                    .get(*role)?
                    .as_array()?
                    .iter()
                    .find(|entry| {
                        entry
                            .get(0)
                            .and_then(Value::as_str)
                            .and_then(|id| id.parse::<i64>().ok())
                            == Some(row.champion_id)
                    })?
                    .get(3)?
                    .as_u64()
            })
            .sum();
        row.games as f64 >= games as f64 * tierlist::MIN_ROLE_SHARE
    });
    Some(list)
}

pub(super) fn parse_counters(root: &Value, lane: &str, list: &TierList) -> Option<Counters> {
    let rows = root.get(role(lane)?.1)?.get(0)?.as_array()?;
    let rows: Vec<CounterRow> = rows
        .iter()
        .filter_map(|row| {
            let champion_id = row.get(0)?.as_i64().filter(|id| *id > 0)?;
            // u.gg's wins column counts the opponent's wins: a champion's rows
            // sum to the complement of its own win rate over the same games
            // (Ezreal D+ ADC: rows 54.1%, ranking 45.9%, 108,439 games).
            let (opponent_win_pct, games) = counts(row.get(1)?, row.get(2)?)?;
            let vs_win_pct = 100.0 - opponent_win_pct;
            let opponent = list
                .rows
                .iter()
                .find(|row| row.champion_id == champion_id)?;
            Some(CounterRow {
                champion_id,
                vs_win_pct,
                games,
                all_win_pct: opponent.win_pct, prev_games: 0,
            })
        })
        .collect();
    (!rows.is_empty()).then(|| Counters {
        lane: lolalytics::lane(lane).unwrap().into(),
        rows,
        patch: None,
    })
}

fn patch_parts(name: &str) -> Option<(u32, u32)> {
    let (year, number) = name.split_once('_')?;
    Some((year.parse().ok()?, number.parse().ok()?))
}

pub(super) fn patch_names(versions: &Value) -> Option<Patches> {
    let mut patches: Vec<_> = versions
        .as_object()?
        .keys()
        .filter_map(|key| Some((patch_parts(key)?, key.replace('_', "."))))
        .collect();
    patches.sort_by(|a, b| b.0.cmp(&a.0));
    Some(Patches {
        current: patches.first()?.1.clone(),
        previous: patches.get(1).map(|p| p.1.clone()),
    })
}

static LAST_GOOD: OnceLock<LastGoodCache<Value>> = OnceLock::new();
static FAILURES: OnceLock<Mutex<BuildCache<()>>> = OnceLock::new();

fn last_good() -> &'static LastGoodCache<Value> {
    LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::ChampionStats),
            lolalytics::SUCCESS_TTL,
            96,
            2 * 1024 * 1024,
        )
    })
}

/// Bounded requests, per-key single flight, negative caching, and persisted
/// last-good data. Previous-patch entries are immutable and reused indefinitely.
async fn json_cached(
    key: String,
    url: String,
    historical: bool,
    parser: &'static str,
    select: impl FnOnce(Value) -> Option<Value>,
) -> Option<Sourced<Value>> {
    let gate = keystone_gate(&key);
    let _guard = gate.lock().await;
    let cached = last_good().lookup(&key);
    let stale = || {
        cached
            .clone()
            .map(|hit| Sourced::stale(hit.value, Provider::Ugg, hit.fetched_at))
    };
    if let Some(hit) = cached.as_ref().filter(|hit| historical || hit.fresh) {
        return Some(Sourced {
            value: hit.value.clone(),
            provider: Provider::Ugg,
            stale: false,
            fetched_at: Some(hit.fetched_at),
        });
    }
    let failures = FAILURES.get_or_init(|| Mutex::new(BuildCache::default()));
    if matches!(
        failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .lookup(&key),
        Cached::Unavailable
    ) {
        return stale();
    }
    let found = async {
        let _permit = keystone_fetches()
            .acquire()
            .await
            .map_err(|_| ProviderError::Unavailable)?;
        let client = reqwest::Client::builder()
            .connect_timeout(TIMEOUT)
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| ProviderError::Unavailable)?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| ProviderError::from_reqwest(&e))?;
        if !response.status().is_success() {
            return Err(ProviderError::from_status(response.status()));
        }
        let body = response
            .text()
            .await
            .map_err(|_| ProviderError::Unavailable)?;
        serde_json::from_str(&body)
            .ok()
            .and_then(select)
            .ok_or(ProviderError::Format)
    }
    .await;
    health::record(Source::UggStats, parser, &found);
    match found {
        Ok(value) => {
            last_good().store(key, value.clone());
            Some(Sourced::fresh(value, Provider::Ugg))
        }
        Err(_) => {
            failures.lock().unwrap_or_else(|e| e.into_inner()).fail(key);
            stale()
        }
    }
}

async fn versions() -> Option<Sourced<Value>> {
    json_cached(
        "ugg-v1|versions".into(),
        VERSIONS_URL.into(),
        false,
        "versions map",
        |value| {
            patch_names(&value)?;
            Some(value)
        },
    )
    .await
}

pub(super) fn dataset_url(
    versions: &Value,
    dataset: &str,
    patch: &str,
    tier: &str,
    champion: Option<i64>,
) -> Option<String> {
    let (slug, _) = rank(tier)?;
    let patch = patch.replace('.', "_");
    patch_parts(&patch)?;
    let version = versions.get(&patch)?.get(dataset)?.as_str()?;
    // The path comes from an external map; only numeric semantic versions belong here.
    if version.split('.').count() != 3
        || !version.split('.').all(|part| part.parse::<u32>().is_ok())
    {
        return None;
    }
    Some(match champion {
        Some(id) if id > 0 => {
            format!("{BASE}/{dataset}/{patch}/ranked_solo_5x5/{id}/{version}.json")
        }
        Some(_) => return None,
        None => format!("{BASE}/{dataset}/world/{patch}/ranked_solo_5x5/{slug}/{version}.json"),
    })
}

async fn dataset(
    versions: &Sourced<Value>,
    kind: &str,
    patch: &str,
    tier: &str,
    champion: Option<i64>,
    historical: bool,
) -> Option<Sourced<Value>> {
    if lolalytics::tier_slug(tier) != "gold_plus" {
        return dataset_bucket(versions, kind, patch, tier, champion, historical).await;
    }
    // Both buckets must be present: a partial union would silently display
    // another population under the Gold+ label. Failures use the normal fallback.
    let (gold, platinum) = tokio::join!(
        dataset_bucket(versions, kind, patch, "gold", champion, historical),
        dataset_bucket(versions, kind, patch, "platinum_plus", champion, historical),
    );
    let (gold, platinum) = (gold?, platinum?);
    Some(Sourced {
        value: merge_datasets(kind, &gold.value, &platinum.value)?,
        provider: Provider::Ugg,
        stale: gold.stale || platinum.stale,
        fetched_at: gold
            .fetched_at
            .zip(platinum.fetched_at)
            .map(|(a, b)| a.min(b)),
    })
}

async fn dataset_bucket(
    versions: &Sourced<Value>,
    kind: &str,
    patch: &str,
    tier: &str,
    champion: Option<i64>,
    historical: bool,
) -> Option<Sourced<Value>> {
    let url = dataset_url(&versions.value, kind, patch, tier, champion)?;
    let rank_id = rank(tier)?.1;
    let parser = match kind {
        "champion_ranking" => "champion ranking",
        "matchups" => "matchups",
        "duos" => "duos",
        _ => "dataset",
    };
    let mut found = json_cached(
        format!("ugg-v1|{url}|{rank_id}"),
        url.clone(),
        historical,
        parser,
        |value| match kind {
            "champion_ranking" => {
                ["top", "jungle", "middle", "bottom", "support"]
                    .iter()
                    .find_map(|lane| parse_tierlist(&value, lane))?;
                Some(value)
            }
            "matchups" => {
                let selected = value.get("12")?.get(rank_id)?;
                selected.as_object()?.values().find(|role| {
                    role.get(0)
                        .and_then(Value::as_array)
                        .is_some_and(|rows| !rows.is_empty())
                })?;
                Some(selected.clone())
            }
            "duos" => {
                super::duos::parse_value(&value)?;
                Some(value)
            }
            _ => None,
        },
    )
    .await?;
    found.stale |= versions.stale;
    Some(found)
}

/// Shared by all five tier lists, overviews and counters: one bulk request per patch/bracket.
async fn raw_list(
    versions: &Sourced<Value>,
    patch: &str,
    lane: &str,
    tier: &str,
    historical: bool,
) -> Option<Sourced<TierList>> {
    let found = dataset(versions, "champion_ranking", patch, tier, None, historical).await?;
    Some(Sourced {
        value: parse_tierlist(&found.value, lane)?,
        provider: Provider::Ugg,
        stale: found.stale,
        fetched_at: found.fetched_at,
    })
}

async fn raw_lane_tierlist(
    versions: &Sourced<Value>,
    patch: &str,
    lane: &str,
    tier: &str,
    historical: bool,
) -> Option<Sourced<TierList>> {
    let found = dataset(versions, "champion_ranking", patch, tier, None, historical).await?;
    Some(Sourced {
        value: parse_lane_tierlist(&found.value, lane)?,
        provider: Provider::Ugg,
        stale: found.stale,
        fetched_at: found.fetched_at,
    })
}

pub async fn tier_list(lane: &str, tier: &str) -> Option<Sourced<TierList>> {
    role(lane)?;
    let versions = versions().await?;
    let patches = patch_names(&versions.value)?;
    let now = raw_lane_tierlist(&versions, &patches.current, lane, tier, false).await;
    let prev = match &patches.previous {
        Some(patch) => raw_lane_tierlist(&versions, patch, lane, tier, true).await,
        None => None,
    };
    let source = now.as_ref().or(prev.as_ref())?;
    let info = PatchInfo {
        patch: patches.current,
        previous: prev.as_ref().and(patches.previous),
    };
    Some(Sourced {
        value: tierlist::blend_tierlist(
            now.as_ref().map(|s| s.value.clone()).unwrap_or_default(),
            prev.as_ref().map(|s| &s.value),
            tier,
            info,
        ),
        provider: Provider::Ugg,
        stale: source.stale || prev.as_ref().is_some_and(|s| s.stale),
        fetched_at: source.fetched_at,
    })
}

pub(super) async fn default_lane(champion: i64, tier: &str) -> Option<&'static str> {
    let versions = versions().await?;
    let patches = patch_names(&versions.value)?;
    let found = match dataset(
        &versions,
        "champion_ranking",
        &patches.current,
        tier,
        None,
        false,
    )
    .await
    {
        Some(found) => found,
        None => {
            dataset(
                &versions,
                "champion_ranking",
                patches.previous.as_deref()?,
                tier,
                None,
                true,
            )
            .await?
        }
    };
    ["top", "jungle", "middle", "bottom", "support"]
        .into_iter()
        .filter_map(|lane| {
            let list = parse_tierlist(&found.value, lane)?;
            Some((
                lane,
                list.rows
                    .iter()
                    .find(|row| row.champion_id == champion)?
                    .games,
            ))
        })
        .max_by_key(|(_, games)| *games)
        .map(|(lane, _)| lane)
}

pub async fn overview(champion: i64, lane: Option<&str>, tier: &str) -> Option<Sourced<Overview>> {
    let lane = match lane {
        Some(lane) => lolalytics::lane(lane)?,
        None => default_lane(champion, tier).await?,
    };
    let versions = versions().await?;
    let patches = patch_names(&versions.value)?;
    let (now, prev) = tokio::join!(
        raw_list(&versions, &patches.current, lane, tier, false),
        async {
            match &patches.previous {
                Some(patch) => raw_list(&versions, patch, lane, tier, true).await,
                None => None,
            }
        }
    );
    overview_from_lists(champion, lane, now, prev, patches)
}

/// Overviews include low-sample champions too; the tier-list floor only controls
/// placement. Both screens still blend the same four rates with the same prior.
fn overview_from_lists(
    champion: i64,
    lane: &str,
    now: Option<Sourced<TierList>>,
    prev: Option<Sourced<TierList>>,
    patches: Patches,
) -> Option<Sourced<Overview>> {
    let current = now
        .as_ref()
        .and_then(|s| s.value.rows.iter().find(|r| r.champion_id == champion));
    let previous = prev
        .as_ref()
        .and_then(|s| s.value.rows.iter().find(|r| r.champion_id == champion));
    let row = current.or(previous)?;
    let source = if current.is_some() {
        now.as_ref()?
    } else {
        prev.as_ref()?
    };
    let info = PatchInfo {
        patch: patches.current.clone(),
        previous: previous.and(patches.previous),
    };
    let mut stats = OverviewStats {
        win_pct: row.win_pct,
        avg_win_pct: 50.0,
        pick_pct: row.pick_pct,
        ban_pct: row.ban_pct,
        games: current.map_or(0, |r| r.games),
        patch: patches.current,
        ..Default::default()
    };
    if let Some(previous) = previous {
        stats.win_pct = super::patch::blend_rate(stats.win_pct, stats.games, previous.win_pct);
        stats.pick_pct = super::patch::blend_rate(stats.pick_pct, stats.games, previous.pick_pct);
        stats.ban_pct = super::patch::blend_rate(stats.ban_pct, stats.games, previous.ban_pct);
    }
    Some(Sourced {
        value: Overview {
            lane: lane.into(),
            stats,
            blend: Some(info),
            ..Default::default()
        },
        provider: Provider::Ugg,
        stale: source.stale || (previous.is_some() && prev.as_ref().is_some_and(|s| s.stale)),
        fetched_at: source.fetched_at,
    })
}

async fn raw_counters(
    versions: &Sourced<Value>,
    patch: &str,
    champion: i64,
    lane: &str,
    tier: &str,
    historical: bool,
) -> Option<Sourced<Counters>> {
    let (list, found) = tokio::join!(
        raw_list(versions, patch, lane, tier, historical),
        dataset(versions, "matchups", patch, tier, Some(champion), historical),
    );
    let (list, found) = (list?, found?);
    Some(Sourced {
        value: parse_counters(&found.value, lane, &list.value)?,
        provider: Provider::Ugg,
        stale: list.stale || found.stale,
        fetched_at: found.fetched_at,
    })
}

pub async fn counters(champion: i64, lane: Option<&str>, tier: &str) -> Option<Sourced<Counters>> {
    let lane = match lane {
        Some(lane) => lolalytics::lane(lane)?,
        None => default_lane(champion, tier).await?,
    };
    let versions = versions().await?;
    let patches = patch_names(&versions.value)?;
    // This patch and the previous one are independent downloads: fetch them together.
    let (now, prev) = tokio::join!(
        raw_counters(&versions, &patches.current, champion, lane, tier, false),
        async {
            match &patches.previous {
                Some(patch) => raw_counters(&versions, patch, champion, lane, tier, true).await,
                None => None,
            }
        }
    );
    let source = now.as_ref().or(prev.as_ref())?;
    Some(Sourced {
        value: counters::blend_counters(
            now.as_ref()
                .map(|s| s.value.clone())
                .unwrap_or_else(|| Counters {
                    lane: lane.into(),
                    ..Default::default()
                }),
            prev.as_ref().map(|s| &s.value),
            PatchInfo {
                patch: patches.current,
                previous: prev.as_ref().and(patches.previous),
            },
        ),
        provider: Provider::Ugg,
        stale: source.stale || prev.as_ref().is_some_and(|s| s.stale),
        fetched_at: source.fetched_at,
    })
}

pub async fn matchup_stats(
    champion: i64,
    enemy: i64,
    lane: &str,
    tier: &str,
) -> Option<Sourced<super::matchup::MatchupStats>> {
    let found = counters(champion, Some(lane), tier).await?;
    let row = found
        .value
        .rows
        .iter()
        .find(|row| row.champion_id == enemy)?;
    let average = overview(champion, Some(lane), tier).await?;
    Some(Sourced {
        value: super::matchup::MatchupStats {
            win_pct: row.vs_win_pct,
            games: row.games,
            avg_win_pct: average.value.stats.win_pct,
            patch: found.value.patch.as_ref()?.patch.clone(),
        },
        provider: Provider::Ugg,
        stale: found.stale || average.stale,
        fetched_at: found.fetched_at,
    })
}

pub async fn duos(tier: &str) -> Option<Sourced<super::duos::DuoList>> {
    let versions = versions().await?;
    let patches = patch_names(&versions.value)?;
    let now = dataset(&versions, "duos", &patches.current, tier, None, false).await;
    let prev = match &patches.previous {
        Some(patch) => dataset(&versions, "duos", patch, tier, None, true).await,
        None => None,
    };
    let source = now.as_ref().or(prev.as_ref())?;
    Some(Sourced {
        value: super::duos::blend(
            now.as_ref()
                .and_then(|s| super::duos::parse_value(&s.value))
                .unwrap_or_default(),
            prev.as_ref()
                .and_then(|s| super::duos::parse_value(&s.value))
                .as_ref(),
        ),
        provider: Provider::Ugg,
        stale: source.stale || prev.as_ref().is_some_and(|s| s.stale),
        fetched_at: source.fetched_at,
    })
}

/// Live u.gg first, live lolalytics next, then the last successful u.gg result.
/// Calling the fallback lazily keeps a warm stats screen free of extra requests.
pub async fn prefer<T>(
    primary: Option<Sourced<T>>,
    fallback: impl Future<Output = Result<Option<Sourced<T>>, RuneError>>,
) -> Result<Option<Sourced<T>>, RuneError> {
    if primary.as_ref().is_some_and(|source| !source.stale) {
        return Ok(primary);
    }
    match fallback.await {
        Ok(Some(found)) if !found.stale || primary.is_none() => Ok(Some(found)),
        _ if primary.is_some() => Ok(primary),
        result => result,
    }
}

#[cfg(test)]
mod tests;
