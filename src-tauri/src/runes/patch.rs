//! Patch blending: early in a patch the samples are tiny, so the previous
//! patch's data is blended in with a fixed, small weight.
//!
//! One constant ([`PRIOR_GAMES`]), two formulas ([`blend_rate`] for a single
//! rate, [`blend_choices`] for a table of options) and one fetch wrapper
//! ([`blended`]) are shared by every lolalytics and op.gg call site. The
//! previous patch is immutable once it has ended, so its parsed data is cached
//! on disk keyed by patch and fetched about once per patch. A failed fetch of
//! it never breaks the current view: the current patch is shown as before.
//!
//! Patch names come from Data Dragon's `versions.json` (newest first, so the
//! year rollover `16.1` -> `15.24` needs no arithmetic).

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::time::{Duration, Instant};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::cache::LastGoodCache;
use super::provider::ProviderError;

/// Games' worth of weight given to the previous patch. A rate becomes
/// `(wins_now + rate_prev * K) / (games_now + K)`; a table of options merges
/// the previous patch's counts scaled to total K games.
pub const PRIOR_GAMES: f64 = 400.0;

const VERSIONS_URL: &str = "https://ddragon.leagueoflegends.com/api/versions.json";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const PATCHES_TTL: Duration = Duration::from_secs(60 * 60);
const PATCHES_RETRY: Duration = Duration::from_secs(5 * 60);

/// The current and previous patch names, e.g. `16.20` and `16.19`.
#[derive(Debug, Clone, PartialEq)]
pub struct Patches {
    pub current: String,
    pub previous: Option<String>,
}

/// What a blended value was built from, carried inside the value so cached and
/// persisted copies keep it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchInfo {
    pub patch: String,
    /// The previous patch, only when it actually contributed.
    pub previous: Option<String>,
}

/// One option's blended weight and rate (win %).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    pub weight: f64,
    pub win_pct: f64,
}

/// `16.20.1` -> `16.20`.
pub fn patch_name(version: &str) -> Option<String> {
    let mut parts = version.trim().split('.');
    let (major, minor) = (parts.next()?, parts.next()?);
    (major.parse::<u32>().is_ok() && minor.parse::<u32>().is_ok()).then(|| format!("{major}.{minor}"))
}

/// The patch before `current` in Data Dragon's newest-first version list.
pub fn previous_patch(versions: &[String], current: &str) -> Option<String> {
    let mut names = versions.iter().filter_map(|version| patch_name(version));
    let mut seen_current = false;
    names.find(|name| {
        if seen_current && name != current {
            return true;
        }
        seen_current |= name == current;
        false
    })
}

/// A rate blended with the previous patch's rate for the same entity.
pub fn blend_rate(rate_now: f64, games_now: u64, rate_prev: f64) -> f64 {
    (rate_now * games_now as f64 + rate_prev * PRIOR_GAMES) / (games_now as f64 + PRIOR_GAMES)
}

/// A table of options (`key`, games, win %) merged with the previous patch's.
/// An option's weight is its current games plus the previous patch's games
/// scaled so the whole previous table counts as [`PRIOR_GAMES`]; its rate is
/// blended with [`blend_rate`] when both patches have it. Options only the
/// previous patch has come after the current ones, with their previous rate.
/// Weights only order and select; callers keep showing current games.
pub fn blend_choices<K: Eq + Hash + Clone>(
    now: &[(K, u64, Option<f64>)],
    prev: &[(K, u64, Option<f64>)],
) -> Vec<(K, Rate)> {
    let total_prev: u64 = prev.iter().map(|(_, games, _)| games).sum();
    let scale = if total_prev == 0 { 0.0 } else { PRIOR_GAMES / total_prev as f64 };
    let prev_by_key: HashMap<&K, (u64, Option<f64>)> =
        prev.iter().map(|(key, games, rate)| (key, (*games, *rate))).collect();
    let mut out: Vec<(K, Rate)> = now
        .iter()
        .map(|(key, games, rate)| {
            let (prev_games, prev_rate) = prev_by_key.get(key).copied().unwrap_or((0, None));
            let win_pct = match (rate, prev_rate) {
                (Some(now), Some(prev)) => blend_rate(*now, *games, prev),
                (Some(now), None) => *now,
                (None, Some(prev)) => prev,
                (None, None) => 0.0,
            };
            (
                key.clone(),
                Rate {
                    weight: *games as f64 + prev_games as f64 * scale,
                    win_pct,
                },
            )
        })
        .collect();
    for (key, games, rate) in prev {
        if !now.iter().any(|(other, _, _)| other == key) {
            out.push((
                key.clone(),
                Rate {
                    weight: *games as f64 * scale,
                    win_pct: rate.unwrap_or(0.0),
                },
            ));
        }
    }
    out
}

/// The previous patch's data for `key`: from the disk cache when it is there
/// (it never changes), otherwise fetched once and stored. `None` on a failed
/// fetch or an empty page, so the caller falls back to the current patch.
pub async fn prior<T, Fut>(cache: &LastGoodCache<T>, key: &str, fetch: Fut) -> Option<T>
where
    T: Clone + Serialize + DeserializeOwned,
    Fut: Future<Output = Result<Option<T>, ProviderError>>,
{
    if let Some(cached) = cache.lookup(key) {
        return Some(cached.value);
    }
    let found = fetch.await.ok().flatten()?;
    cache.store(key.to_string(), found.clone());
    Some(found)
}

/// The disk-cache key for one patch's data.
pub fn prior_key(base: &str, patch: &str) -> String {
    format!("{base}|p{patch}")
}

/// Blends `now` with the previous patch's data. `fetch` loads that patch's data
/// given its name; `merge` receives the current value, the previous one when
/// there is one, and the [`PatchInfo`] to store in the result (empty when the
/// patch names could not be loaded).
pub async fn blended<T, Fut>(
    now: T,
    base_key: &str,
    cache: &LastGoodCache<T>,
    fetch: impl FnOnce(String) -> Fut,
    merge: impl FnOnce(T, Option<&T>, PatchInfo) -> T,
) -> T
where
    T: Clone + Serialize + DeserializeOwned,
    Fut: Future<Output = Result<Option<T>, ProviderError>>,
{
    let patches = patches().await;
    blend_with(patches, now, base_key, cache, fetch, merge).await
}

pub(super) async fn blend_with<T, Fut>(
    patches: Option<Patches>,
    now: T,
    base_key: &str,
    cache: &LastGoodCache<T>,
    fetch: impl FnOnce(String) -> Fut,
    merge: impl FnOnce(T, Option<&T>, PatchInfo) -> T,
) -> T
where
    T: Clone + Serialize + DeserializeOwned,
    Fut: Future<Output = Result<Option<T>, ProviderError>>,
{
    let Some(patches) = patches else {
        return merge(now, None, PatchInfo::default());
    };
    let mut info = PatchInfo {
        patch: patches.current.clone(),
        previous: None,
    };
    let Some(previous) = patches.previous else {
        return merge(now, None, info);
    };
    let found = prior(cache, &prior_key(base_key, &previous), fetch(previous.clone())).await;
    if found.is_some() {
        info.previous = Some(previous);
    }
    merge(now, found.as_ref(), info)
}

// ---------------------------------------------------------------------------
// Patch names from Data Dragon
// ---------------------------------------------------------------------------

static PATCHES: tokio::sync::Mutex<Option<(Instant, Option<Patches>)>> =
    tokio::sync::Mutex::const_new(None);

/// The current and previous patch, cached for an hour (five minutes after a
/// failure). `None` when Data Dragon cannot be reached: nothing is blended.
pub async fn patches() -> Option<Patches> {
    let mut slot = PATCHES.lock().await;
    if let Some((at, found)) = slot.as_ref() {
        let ttl = if found.is_some() { PATCHES_TTL } else { PATCHES_RETRY };
        if at.elapsed() < ttl {
            return found.clone();
        }
    }
    let found = fetch_patches().await;
    *slot = Some((Instant::now(), found.clone()));
    found
}

async fn fetch_patches() -> Option<Patches> {
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .ok()?;
    let versions: Vec<String> = client.get(VERSIONS_URL).send().await.ok()?.json().await.ok()?;
    let latest = versions.first()?;
    let current = patch_name(latest)?;
    Some(Patches {
        previous: previous_patch(&versions, &current),
        current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn versions(list: &[&str]) -> Vec<String> {
        list.iter().map(|v| v.to_string()).collect()
    }

    fn memory_cache() -> LastGoodCache<Vec<u64>> {
        LastGoodCache::open(None, Duration::from_secs(60), 16, 1 << 20)
    }

    fn patches_16_20() -> Option<Patches> {
        Some(Patches {
            current: "16.20".into(),
            previous: Some("16.19".into()),
        })
    }

    /// Merge that records which inputs it saw: (now, prev, info).previous.
    fn summarize(now: Vec<u64>, prev: Option<&Vec<u64>>, info: PatchInfo) -> Vec<u64> {
        let mut out = now;
        out.extend(prev.cloned().unwrap_or_default());
        out.push(info.previous.is_some() as u64);
        out
    }

    #[test]
    fn patch_names_drop_the_hotfix_digit() {
        assert_eq!(patch_name("16.20.1").as_deref(), Some("16.20"));
        assert_eq!(patch_name("nonsense"), None);
    }

    #[test]
    fn the_previous_patch_comes_from_the_list_across_a_year_boundary() {
        let list = versions(&["16.1.1", "15.24.1", "15.23.1"]);
        assert_eq!(previous_patch(&list, "16.1").as_deref(), Some("15.24"));
        let list = versions(&["16.20.1", "16.19.2", "16.19.1", "16.18.1"]);
        // A hotfix entry of the same patch is not the previous patch.
        assert_eq!(previous_patch(&list, "16.20").as_deref(), Some("16.19"));
        assert_eq!(previous_patch(&list, "16.19").as_deref(), Some("16.18"));
        assert_eq!(previous_patch(&list, "16.18"), None);
    }

    #[test]
    fn day_zero_takes_the_previous_patch_rate() {
        let blended = blend_choices(&[("a", 0, None)], &[("a", 5000, Some(52.0))]);
        assert!((blended[0].1.win_pct - 52.0).abs() < 1e-9);
        assert!((blend_rate(40.0, 0, 52.0) - 52.0).abs() < 1e-9);
    }

    #[test]
    fn a_large_current_sample_dominates() {
        let blended = blend_rate(50.0, 5000, 54.0);
        assert!((blended - 50.0).abs() < 0.5, "blended was {blended}");
    }

    #[test]
    fn choices_merge_so_a_current_only_option_can_rank() {
        // "new" exists only now; "old" only before; "both" in both.
        let now = [("new", 300, Some(55.0)), ("both", 100, Some(50.0))];
        let prev = [("both", 8000, Some(52.0)), ("old", 2000, Some(51.0))];
        let merged = blend_choices(&now, &prev);
        let weight = |name: &str| merged.iter().find(|(key, _)| *key == name).unwrap().1.weight;
        // Previous counts are scaled to 400 games in total: both 320, old 80.
        assert!((weight("both") - 420.0).abs() < 1e-9);
        assert!((weight("old") - 80.0).abs() < 1e-9);
        assert!(weight("new") > weight("old"), "a current-only option outranks a fading one");
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn prior_keys_differ_per_patch() {
        assert_ne!(prior_key("yone|counters", "16.19"), prior_key("yone|counters", "16.20"));
        assert_eq!(prior_key("k", "16.19"), "k|p16.19");
    }

    #[tokio::test]
    async fn a_failed_previous_fetch_falls_back_to_the_current_patch_only() {
        let cache = memory_cache();
        let out = blend_with(
            patches_16_20(),
            vec![1],
            "k",
            &cache,
            |_| async { Err::<Option<Vec<u64>>, _>(ProviderError::Unavailable) },
            summarize,
        )
        .await;
        assert_eq!(out, vec![1, 0], "no previous data and no previous patch in the info");
        assert!(cache.is_empty(), "a failure is not cached");
    }

    #[tokio::test]
    async fn the_previous_patch_is_fetched_once_and_then_served_from_the_cache() {
        let cache = memory_cache();
        let fetches = AtomicUsize::new(0);
        for _ in 0..2 {
            let out = blend_with(
                patches_16_20(),
                vec![1],
                "k",
                &cache,
                |patch| {
                    assert_eq!(patch, "16.19");
                    // Counted when the request actually runs, not when it is built.
                    async {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        Ok(Some(vec![9]))
                    }
                },
                summarize,
            )
            .await;
            assert_eq!(out, vec![1, 9, 1]);
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        assert!(cache.lookup("k|p16.19").is_some());
        assert!(cache.lookup("k|p16.20").is_none());
    }

    #[tokio::test]
    async fn no_patch_names_means_no_blending() {
        let cache = memory_cache();
        let out = blend_with(None, vec![1], "k", &cache, |_| async { Ok(Some(vec![9])) }, summarize).await;
        assert_eq!(out, vec![1, 0]);
    }
}
