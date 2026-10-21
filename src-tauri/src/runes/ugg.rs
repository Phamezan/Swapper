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
use super::provider::{DataKind, Provider, ProviderError, Sourced};
use super::tierlist::{self, TierList, TierRow};
use super::RuneError;

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

fn parse_tierlist(root: &Value, lane: &str) -> Option<TierList> {
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
fn parse_lane_tierlist(root: &Value, lane: &str) -> Option<TierList> {
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

/// Merge only the count fields we consume; unrelated derived upstream fields
/// (rankings, deltas, timestamps, etc.) must never be summed or averaged.
fn merge_count_rows(
    left: &[Value],
    right: &[Value],
    ids: &[usize],
    counts: &[usize],
) -> Option<Vec<Value>> {
    let width = ids.iter().chain(counts).max()? + 1;
    let mut merged = BTreeMap::<Vec<String>, Vec<Value>>::new();
    for row in left.iter().chain(right) {
        let key: Vec<String> = ids
            .iter()
            .map(|index| {
                let id = row.get(*index)?;
                let id = id
                    .as_str()
                    .and_then(|text| text.parse::<u64>().ok())
                    .or_else(|| id.as_u64())?;
                (id > 0).then(|| id.to_string())
            })
            .collect::<Option<_>>()?;
        let entry = merged.entry(key).or_insert_with(|| {
            let mut entry = vec![Value::Null; width];
            for index in ids {
                entry[*index] = row[*index].clone();
            }
            for index in counts {
                entry[*index] = Value::from(0u64);
            }
            entry
        });
        for index in counts {
            entry[*index] = Value::from(
                entry[*index]
                    .as_u64()?
                    .checked_add(row.get(*index)?.as_u64()?)?,
            );
        }
    }
    Some(merged.into_values().map(Value::Array).collect())
}

fn array_or_empty(value: Option<&Value>) -> Option<&[Value]> {
    match value {
        Some(value) => Some(value.as_array()?.as_slice()),
        None => Some(&[]),
    }
}

fn merge_datasets(kind: &str, left: &Value, right: &Value) -> Option<Value> {
    match kind {
        "champion_ranking" => {
            let (left_roles, right_roles) = (left.get(0)?.as_object()?, right.get(0)?.as_object()?);
            let mut roles = serde_json::Map::new();
            for role in left_roles
                .keys()
                .chain(right_roles.keys())
                .collect::<BTreeSet<_>>()
            {
                let rows = merge_count_rows(
                    array_or_empty(left_roles.get(role))?,
                    array_or_empty(right_roles.get(role))?,
                    &[0],
                    &[2, 3],
                )?;
                roles.insert(role.clone(), Value::Array(rows));
            }
            let (left_bans, right_bans) = (left.get(1)?.as_object()?, right.get(1)?.as_object()?);
            let mut bans = serde_json::Map::new();
            for id in left_bans
                .keys()
                .chain(right_bans.keys())
                .collect::<BTreeSet<_>>()
            {
                let count = |map: &serde_json::Map<String, Value>| match map.get(id) {
                    Some(value) => value.as_u64(),
                    None => Some(0),
                };
                bans.insert(
                    id.clone(),
                    Value::from(count(left_bans)?.checked_add(count(right_bans)?)?),
                );
            }
            Some(serde_json::json!([
                roles,
                bans,
                null,
                left.get(3)?.as_f64()? + right.get(3)?.as_f64()?
            ]))
        }
        "matchups" => {
            let (left, right) = (left.as_object()?, right.as_object()?);
            let mut roles = serde_json::Map::new();
            for role in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
                let rows = |map: &serde_json::Map<String, Value>| match map.get(role) {
                    Some(value) => Some(value.get(0)?.as_array()?.clone()),
                    None => Some(Vec::new()),
                };
                roles.insert(
                    role.clone(),
                    serde_json::json!([
                        merge_count_rows(&rows(left)?, &rows(right)?, &[0], &[1, 2])?,
                        null
                    ]),
                );
            }
            Some(Value::Object(roles))
        }
        "duos" => Some(serde_json::json!([{"adc_supp":merge_count_rows(
            left.get(0)?.get("adc_supp")?.as_array()?, right.get(0)?.get("adc_supp")?.as_array()?,
            &[0,1,4,5], &[2,3,6,7,8,9])?}])),
        _ => None,
    }
}

fn parse_counters(root: &Value, lane: &str, list: &TierList) -> Option<Counters> {
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
                all_win_pct: opponent.win_pct,
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

fn patch_names(versions: &Value) -> Option<Patches> {
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
        let _permit = keystone_fetches().acquire().await.ok()?;
        let client = reqwest::Client::builder()
            .connect_timeout(TIMEOUT)
            .timeout(TIMEOUT)
            .build()
            .ok()?;
        let response = client.get(url).send().await.ok()?.error_for_status().ok()?;
        select(response.json().await.ok()?)
    }
    .await;
    match found {
        Some(value) => {
            last_good().store(key, value.clone());
            Some(Sourced::fresh(value, Provider::Ugg))
        }
        None => {
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
        |value| {
            patch_names(&value)?;
            Some(value)
        },
    )
    .await
}

fn dataset_url(
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
    let mut found = json_cached(
        format!("ugg-v1|{url}|{rank_id}"),
        url.clone(),
        historical,
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

async fn default_lane(champion: i64, tier: &str) -> Option<&'static str> {
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
    let now = raw_list(&versions, &patches.current, lane, tier, false).await;
    let prev = match &patches.previous {
        Some(patch) => raw_list(&versions, patch, lane, tier, true).await,
        None => None,
    };
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
    let list = raw_list(versions, patch, lane, tier, historical).await?;
    let found = dataset(
        versions,
        "matchups",
        patch,
        tier,
        Some(champion),
        historical,
    )
    .await?;
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
    let now = raw_counters(&versions, &patches.current, champion, lane, tier, false).await;
    let prev = match &patches.previous {
        Some(patch) => raw_counters(&versions, patch, champion, lane, tier, true).await,
        None => None,
    };
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
mod tests {
    use super::*;
    use serde_json::json;

    fn bulk() -> Value {
        json!([{"top":[["266",[[27,709,1262]],53358,106575,0,0,0,0,0,0]],
                "mid":[["103",[],510,1000,0,0,0,0,0,0]]},
               {"266":114076,"103":100,"total_matches":1534032},"timestamp",1662713.0])
    }

    #[test]
    fn tier_lists_filter_role_share_on_each_patch_without_filtering_overviews() {
        let root = json!([{
            "top":[["40",[],300,600],["266",[],500,1000],["24",[],500,1000]],
            "mid":[["40",[],0,0],["266",[],5000,10000],["24",[],4500,9000]],
            "jungle":[["40",[],0,0],["266",[],5000,10000],["24",[],0,0]],
            "adc":[["40",[],0,0],["266",[],0,0],["24",[],0,0]],
            "supp":[["40",[],10000,20000],["266",[],0,0],["24",[],0,0]]},
            {"40":100,"266":100,"24":100,"total_matches":100000},"t",100000]);
        let raw = parse_tierlist(&root, "top").unwrap();
        assert_eq!(
            raw.rows.len(),
            3,
            "off-role stats remain available to overviews"
        );
        let filtered = parse_lane_tierlist(&root, "top").unwrap();
        assert_eq!(
            filtered
                .rows
                .iter()
                .map(|row| row.champion_id)
                .collect::<Vec<_>>(),
            vec![24],
            "Janna and Aatrox are below 5%; Jax is above the boundary"
        );
        let mut previous = root.clone();
        previous[0]["mid"][1][3] = json!(9000);
        let previous = parse_lane_tierlist(&previous, "top").unwrap();
        assert_eq!(
            previous
                .rows
                .iter()
                .map(|row| row.champion_id)
                .collect::<Vec<_>>(),
            vec![266, 24],
            "exactly 5% qualifies on the previous patch independently"
        );
        let info = PatchInfo {
            patch: "16.20".into(),
            previous: Some("16.19".into()),
        };
        let blended = tierlist::blend_tierlist(filtered, Some(&previous), "emerald_plus", info);
        assert!(blended.rows.iter().all(|row| row.champion_id != 40));
        assert_eq!(
            blended
                .rows
                .iter()
                .find(|row| row.champion_id == 266)
                .unwrap()
                .games,
            0
        );
        let overview = overview_from_lists(
            40,
            "top",
            Some(Sourced::fresh(raw, Provider::Ugg)),
            None,
            Patches {
                current: "16.20".into(),
                previous: None,
            },
        )
        .unwrap();
        assert_eq!(
            (overview.value.stats.games, overview.value.stats.win_pct),
            (600, 50.0)
        );
    }

    #[test]
    fn gold_plus_sums_counts_and_denominators_before_calculating_rates() {
        let gold = json!([{"top":[["266",[],60,100],["24",[],30,50]]},
            {"266":20,"24":10,"total_matches":200},"t",250]);
        let platinum = json!([{"top":[["266",[],90,300],["54",[],40,50]]},
            {"266":50,"54":20,"total_matches":500},"t",750]);
        let merged = merge_datasets("champion_ranking", &gold, &platinum).unwrap();
        assert_eq!(merged[1]["266"], 70);
        assert_eq!(merged[1]["total_matches"], 700);
        assert_eq!(merged[3], 1000.0);
        let list = parse_tierlist(&merged, "top").unwrap();
        let row = list.rows.iter().find(|row| row.champion_id == 266).unwrap();
        assert_eq!(
            (row.games, row.win_pct, row.pick_pct, row.ban_pct),
            (400, 37.5, 40.0, 10.0)
        );
        assert_eq!(list.rows.len(), 3, "one-bucket-only champions are retained");

        let gold = json!({"4":[[[27,6,10],[24,1,2]],"t"]});
        let platinum = json!({"4":[[[27,9,30],[54,2,3]],"t"]});
        let merged = merge_datasets("matchups", &gold, &platinum).unwrap();
        assert_eq!(merged["4"][0][1], json!([27, 15, 40]));
        assert_eq!(merged["4"][0].as_array().unwrap().len(), 3);

        let gold = json!([{"adc_supp":[[222,3,60,100,117,2,10,20,6,10],
            [51,3,10,20,412,2,10,20,1,2]]}]);
        let platinum = json!([{"adc_supp":[[222,3,90,300,117,2,20,80,9,30],
            [81,3,10,20,201,2,10,20,2,3]]}]);
        let merged = merge_datasets("duos", &gold, &platinum).unwrap();
        let pair = merged[0]["adc_supp"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == 222)
            .unwrap();
        assert_eq!(pair, &json!([222, 3, 150, 400, 117, 2, 30, 100, 15, 40]));
        let pairs = super::super::duos::parse_value(&merged).unwrap();
        assert_eq!(pairs.duos.len(), 3);
        let pair = pairs.duos.iter().find(|duo| duo.adc_id == 222).unwrap();
        assert_eq!((pair.games, pair.win_pct), (40, 37.5));
        assert!(merge_datasets("duos", &gold, &json!(null)).is_none());
    }

    #[test]
    fn bulk_fields_use_full_role_counts_and_distinct_pick_and_ban_denominators() {
        let list = parse_tierlist(&bulk(), "top").unwrap();
        let row = &list.rows[0];
        assert_eq!((row.champion_id, row.games), (266, 106575));
        assert!((row.win_pct - 50.06615).abs() < 0.0001);
        assert!((row.pick_pct - 6.4097).abs() < 0.001);
        assert!((row.ban_pct - 7.4363).abs() < 0.001);
        assert_eq!(
            parse_tierlist(&bulk(), "middle").unwrap().rows[0].champion_id,
            103
        );
        assert!(parse_tierlist(&bulk(), "support").is_none());
    }

    #[test]
    fn exact_brackets_and_roles_do_not_silently_substitute_another_population() {
        for (slug, id) in [
            ("all", "8"),
            ("platinum_plus", "10"),
            ("emerald_plus", "17"),
            ("diamond_plus", "11"),
            ("master_plus", "14"),
            ("challenger", "1"),
        ] {
            assert_eq!(rank(slug).unwrap().1, id);
        }
        assert!(rank("gold_plus").is_none());
        assert_eq!(rank("gold"), Some(("gold", "5")));
        for (lane, key, id) in [
            ("top", "top", "4"),
            ("jungle", "jungle", "1"),
            ("middle", "mid", "5"),
            ("bottom", "adc", "3"),
            ("support", "supp", "2"),
        ] {
            assert_eq!(role(lane), Some((key, id)));
        }
        assert!(role("none").is_none());
    }

    #[test]
    fn full_matchups_are_from_the_requested_champions_perspective() {
        let body = json!({"4":[[[27,709,1262,-879834],[103,0,0,0]],"timestamp"]});
        let mut list = parse_tierlist(&bulk(), "top").unwrap();
        list.rows.push(TierRow {
            champion_id: 27,
            win_pct: 52.0,
            ..Default::default()
        });
        let found = parse_counters(&body, "top", &list).unwrap();
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.rows[0].games, 1262);
        // 709 of 1262 are the opponent's wins.
        assert!((found.rows[0].vs_win_pct - 43.8193).abs() < 0.001);
        assert_eq!(found.rows[0].all_win_pct, 52.0);
        assert!(parse_counters(&body, "middle", &list).is_none());
    }

    #[test]
    fn malformed_or_empty_data_is_missing_instead_of_zero_percent() {
        for body in [
            json!(null),
            json!([]),
            json!([{}, {}, "t", 0]),
            json!([{"top":[["266",[],11,10]]},{"266":1,"total_matches":10},"t",10]),
        ] {
            assert!(parse_tierlist(&body, "top").is_none());
        }
    }

    #[tokio::test]
    async fn missing_failed_and_stale_primary_data_use_lolalytics() {
        for primary in [None, Some(Sourced::stale(1, Provider::Ugg, 0))] {
            let found = prefer(primary, async {
                Ok(Some(Sourced::fresh(2, Provider::Lolalytics)))
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!((found.value, found.provider), (2, Provider::Lolalytics));
        }
        let found = prefer(Some(Sourced::fresh(1, Provider::Ugg)), async {
            panic!("fallback must be lazy")
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(found.value, 1);
        let found = prefer(Some(Sourced::stale(1, Provider::Ugg, 0)), async {
            Ok(None)
        })
        .await
        .unwrap()
        .unwrap();
        assert!(found.stale);
    }

    #[test]
    fn discovers_patch_order_and_dataset_versions_without_hardcoding_or_path_injection() {
        let versions = json!({"15_24":{"champion_ranking":"1.5.1"},"16_1":{"champion_ranking":"1.6.0"},"garbage":{}});
        assert_eq!(
            patch_names(&versions),
            Some(Patches {
                current: "16.1".into(),
                previous: Some("15.24".into())
            })
        );
        assert!(
            dataset_url(&versions, "champion_ranking", "16.1", "all", None)
                .unwrap()
                .ends_with("/overall/1.6.0.json")
        );
        assert!(dataset_url(&versions, "duos", "16.1", "all", None).is_none());
        assert!(dataset_url(
            &json!({"16_1":{"duos":"../../bad"}}),
            "duos",
            "16.1",
            "all",
            None
        )
        .is_none());
        assert!(dataset_url(&versions, "champion_ranking", "16.1", "gold_plus", None).is_none());
    }

    #[test]
    fn overviews_keep_small_samples_and_previous_only_champions_have_zero_games() {
        let mut list = parse_tierlist(&bulk(), "middle").unwrap();
        list.rows[0].games = 10;
        let patches = Patches {
            current: "16.20".into(),
            previous: Some("16.19".into()),
        };
        let now = Some(Sourced::fresh(list.clone(), Provider::Ugg));
        let mut prev = list;
        prev.rows[0].win_pct = 55.0;
        let prev = Some(Sourced::fresh(prev, Provider::Ugg));
        let found = overview_from_lists(103, "middle", now, prev.clone(), patches.clone()).unwrap();
        assert_eq!(found.value.stats.games, 10);
        assert!(
            (found.value.stats.win_pct - super::super::patch::blend_rate(51.0, 10, 55.0)).abs()
                < 1e-9
        );
        let found = overview_from_lists(103, "middle", None, prev, patches).unwrap();
        assert_eq!(
            (found.value.stats.games, found.value.stats.win_pct),
            (0, 55.0)
        );
        assert!(overview_from_lists(
            999,
            "middle",
            None,
            None,
            Patches {
                current: "16.20".into(),
                previous: None
            }
        )
        .is_none());
    }

    #[tokio::test]
    #[ignore = "network: audits live u.gg fields, scores, overview, counters and duos"]
    async fn prints_live_ugg_audit() {
        let versions = versions().await.expect("version map");
        assert!(!versions.stale);
        let patches = patch_names(&versions.value).unwrap();
        println!("patches {patches:?}");
        for (lane, ids) in [
            ("top", vec![266, 86, 122, 24, 54, 114]),
            ("middle", vec![103, 134, 777, 238, 55]),
            ("jungle", vec![]),
            ("bottom", vec![]),
            ("support", vec![]),
        ] {
            let raw = raw_list(&versions, &patches.current, lane, "emerald_plus", false)
                .await
                .unwrap();
            let current =
                raw_lane_tierlist(&versions, &patches.current, lane, "emerald_plus", false)
                    .await
                    .unwrap();
            let current_kept =
                tierlist::blend_tierlist(current.value, None, "emerald_plus", PatchInfo::default())
                    .rows
                    .len();
            let found = tier_list(lane, "emerald_plus").await.unwrap();
            assert!(!found.stale);
            let games: u64 = found.value.rows.iter().map(|row| row.games).sum();
            let weighted = found
                .value
                .rows
                .iter()
                .map(|row| row.win_pct * row.games as f64)
                .sum::<f64>()
                / games as f64;
            let mut tiers = std::collections::BTreeMap::new();
            for row in &found.value.rows {
                *tiers.entry(row.label()).or_insert(0usize) += 1;
            }
            println!(
                "{lane}: {} current-only rows, {} blended rows ({} previous-only), weighted {weighted:.6}%, tiers {tiers:?}",
                current_kept,found.value.rows.len(),found.value.rows.iter().filter(|row|row.games==0).count()
            );
            if lane == "top" {
                assert!(!found.value.rows.iter().any(|row| row.champion_id == 40));
                let off_role = overview(40, Some("top"), "emerald_plus").await.unwrap();
                assert!(off_role.value.stats.games > 0);
                println!(
                    "Janna top: excluded from placement; overview {:.4}% / {} games retained",
                    off_role.value.stats.win_pct, off_role.value.stats.games
                );
            }
            for id in ids {
                let row = found
                    .value
                    .rows
                    .iter()
                    .find(|r| r.champion_id == id)
                    .unwrap();
                let raw = raw.value.rows.iter().find(|r| r.champion_id == id).unwrap();
                let overview = overview(id, Some(lane), "emerald_plus").await.unwrap();
                assert!((overview.value.stats.win_pct - row.win_pct).abs() < 1e-9);
                println!(
                    "{lane} {id}: shown {:.4}/{:.4}/{:.4}/{}; raw {:.4}/{:.4}/{:.4}/{}",
                    row.win_pct,
                    row.pick_pct,
                    row.ban_pct,
                    row.games,
                    raw.win_pct,
                    raw.pick_pct,
                    raw.ban_pct,
                    raw.games
                );
            }
        }
        let counters = counters(266, Some("top"), "emerald_plus").await.unwrap();
        assert!(!counters.stale && counters.value.rows.len() > 20);
        println!(
            "Aatrox counters: {} rows; vs Singed {:?}",
            counters.value.rows.len(),
            counters.value.rows.iter().find(|r| r.champion_id == 27)
        );
        let duos = duos("emerald_plus").await.unwrap();
        assert!(!duos.stale && !duos.value.duos.is_empty());
        println!("duos: {} pairs", duos.value.duos.len());
        probe().await.expect("Doctor live data probe");
    }

    #[tokio::test]
    #[ignore = "network: verifies Gold+ uses merged u.gg counts across all stats screens"]
    async fn prints_live_gold_plus_audit() {
        let list = tierlist::load("top", "gold_plus").await.unwrap().unwrap();
        assert_eq!(list.provider, Provider::Ugg);
        assert!(!list.stale && !list.value.rows.is_empty());
        let overview = super::super::overview::load("aatrox", 266, Some("top"), "gold_plus")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(overview.provider, Provider::Ugg);
        assert!(!overview.stale);
        assert!(overview.value.build.is_some());
        let counters = super::super::counters::load("aatrox", 266, Some("top"), "gold_plus")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(counters.provider, Provider::Ugg);
        assert!(!counters.stale && !counters.value.rows.is_empty());
        let duos = super::super::duos::load("gold_plus").await.unwrap();
        assert_eq!(duos.provider, Provider::Ugg);
        assert!(!duos.stale && !duos.value.duos.is_empty());
        let matchup = matchup_stats(266, 27, "top", "gold_plus").await.unwrap();
        assert!(!matchup.stale);
        println!(
            "Gold+ u.gg: {} tier rows, overview {:.4}% / {} games with build, {} counters, {} duo pairs; vs Singed {:.4}% / {} games",
            list.value.rows.len(),
            overview.value.stats.win_pct,
            overview.value.stats.games,
            counters.value.rows.len(),duos.value.duos.len(),matchup.value.win_pct,matchup.value.games
        );
    }
}
