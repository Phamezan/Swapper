//! A bounded, persisted cache of each provider's last successful result.
//!
//! The rune screen should keep working when op.gg, lolalytics or u.gg is down,
//! slow or changed. Every successful fetch is therefore written to a per-kind
//! file under `%LOCALAPPDATA%\Swapper\runes-cache`, and a failed fetch falls
//! back to that copy, labelled stale with the time it was fetched.
//!
//! Entries are capped by count and by serialized size; the oldest are evicted
//! first. A missing, unreadable or oversized file is treated as empty: this
//! cache is an optimisation and must never block the feature.
//!
//! Async callers use [`LastGoodCache::store`], which coalesces writes onto a
//! background blocking thread and skips a write when the snapshot is unchanged,
//! so the async runtime never touches the disk. [`LastGoodCache::store_at`]
//! writes synchronously and is what the tests use to avoid a race.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::provider::{now_ms, DataKind};

/// A cache file larger than this is ignored rather than loaded.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// The compact JSON frame around the entries: `{"entries":[]}`.
const WRAPPER_LEN: usize = 14;

/// The per-kind cache path under the Swapper data folder, or `None` when
/// `LOCALAPPDATA` is unavailable (the cache then stays in memory only).
pub fn cache_path(kind: DataKind) -> Option<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(root)
            .join("Swapper")
            .join("runes-cache")
            .join(format!("{}.json", kind.key())),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored<T> {
    value: T,
    fetched_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredFile<T> {
    entries: Vec<StoredEntry<T>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredEntry<T> {
    key: String,
    value: T,
    fetched_at: i64,
}

/// A last-good lookup, with whether it is still within the cache's TTL.
#[derive(Debug, Clone)]
pub struct Cached<T> {
    pub value: T,
    pub fetched_at: i64,
    /// False once the entry is older than the cache TTL.
    pub fresh: bool,
}

struct Inner<T> {
    entries: HashMap<String, Stored<T>>,
}

/// The state shared with the background writer thread. It is independent of
/// `T`, so the writer does not need `T: Send`.
#[derive(Default)]
struct Writer {
    /// The last snapshot queued for disk, to skip unchanged writes.
    last_bytes: Option<Vec<u8>>,
    /// The newest snapshot waiting to be written, if any.
    pending: Option<Vec<u8>>,
    /// Whether a background drain is already running.
    writing: bool,
}

/// The last successful result per request key, capped and persisted.
pub struct LastGoodCache<T> {
    inner: Mutex<Inner<T>>,
    writer: Arc<Mutex<Writer>>,
    ttl: Duration,
    max_entries: usize,
    max_bytes: usize,
    path: Option<PathBuf>,
}

impl<T: Clone + Serialize + DeserializeOwned> LastGoodCache<T> {
    /// Opens the cache at `path`, loading whatever was persisted before. A
    /// missing or invalid file simply starts empty.
    pub fn open(path: Option<PathBuf>, ttl: Duration, max_entries: usize, max_bytes: usize) -> Self {
        let entries = path.as_deref().and_then(load_file).unwrap_or_default();
        Self {
            inner: Mutex::new(Inner { entries }),
            writer: Arc::new(Mutex::new(Writer::default())),
            ttl,
            max_entries,
            max_bytes,
            path,
        }
    }

    /// The cached result for `key`, if any. Stale entries are still returned;
    /// the caller decides whether to show them.
    pub fn lookup(&self, key: &str) -> Option<Cached<T>> {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let stored = inner.entries.get(key)?;
        let age = (now_ms() - stored.fetched_at).max(0) as u128;
        Some(Cached {
            value: stored.value.clone(),
            fetched_at: stored.fetched_at,
            fresh: age < self.ttl.as_millis(),
        })
    }

    /// Records a successful result and schedules a background write, so async
    /// callers never block on the disk.
    pub fn store(&self, key: String, value: T) {
        self.record(key, value, now_ms(), true);
    }

    /// Records a result with an explicit fetch time and writes synchronously.
    /// Tests use this so a background write cannot race a reopen.
    #[cfg(test)]
    pub fn store_at(&self, key: String, value: T, fetched_at: i64) {
        self.record(key, value, fetched_at, false);
    }

    fn record(&self, key: String, value: T, fetched_at: i64, background: bool) {
        let snapshot = {
            let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
            inner.entries.insert(key, Stored { value, fetched_at });
            snapshot_within_caps(&mut inner.entries, self.max_entries, self.max_bytes)
        };
        let Some(path) = self.path.clone() else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(&StoredFile { entries: snapshot }) else {
            return;
        };
        let mut writer = self.writer.lock().unwrap_or_else(|error| error.into_inner());
        if writer.last_bytes.as_deref() == Some(bytes.as_slice()) {
            // The snapshot already queued or written; nothing to do.
            return;
        }
        writer.last_bytes = Some(bytes.clone());
        if !background {
            drop(writer);
            write_atomic(&path, &bytes);
            return;
        }
        writer.pending = Some(bytes);
        if writer.writing {
            // A drain is already running and will pick up the new snapshot.
            return;
        }
        writer.writing = true;
        drop(writer);
        let writer = Arc::clone(&self.writer);
        tauri::async_runtime::spawn_blocking(move || drain_writes(writer, path));
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entries
            .len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Writes queued snapshots until none remain, coalescing multiple stores into
/// the newest one. Best-effort: write failures are ignored.
fn drain_writes(writer: Arc<Mutex<Writer>>, path: PathBuf) {
    loop {
        let next = {
            let mut state = writer.lock().unwrap_or_else(|error| error.into_inner());
            state.pending.take()
        };
        match next {
            Some(bytes) => write_atomic(&path, &bytes),
            None => {
                let mut state = writer.lock().unwrap_or_else(|error| error.into_inner());
                if state.pending.is_none() {
                    state.writing = false;
                    return;
                }
            }
        }
    }
}

/// Evicts until the map fits `max_entries` and its serialized form fits
/// `max_bytes`, oldest first, then returns the remaining snapshot for writing.
///
/// Each entry is serialized once to measure it, so eviction is linear in the
/// number of entries rather than re-serializing the map per removal.
fn snapshot_within_caps<T: Clone + Serialize>(
    entries: &mut HashMap<String, Stored<T>>,
    max_entries: usize,
    max_bytes: usize,
) -> Vec<StoredEntry<T>> {
    while entries.len() > max_entries {
        if !remove_oldest(entries) {
            break;
        }
    }
    if entries.is_empty() {
        return Vec::new();
    }
    // Oldest first, so eviction drops from the front.
    let mut snapshot = sorted_entries(entries);
    let sizes: Vec<usize> = snapshot
        .iter()
        .map(|entry| {
            serde_json::to_vec(entry)
                .map(|bytes| bytes.len())
                .unwrap_or(usize::MAX)
        })
        .collect();
    let mut total = sizes
        .iter()
        .fold(WRAPPER_LEN, |total, size| total.saturating_add(*size))
        + snapshot.len().saturating_sub(1);
    let mut removed = 0usize;
    while total > max_bytes && removed < snapshot.len() {
        // Removing an entry also removes the comma before it.
        total = total.saturating_sub(sizes[removed].saturating_add(1));
        removed += 1;
    }
    if removed > 0 {
        let evicted: HashSet<String> = snapshot[..removed]
            .iter()
            .map(|entry| entry.key.clone())
            .collect();
        entries.retain(|key, _| !evicted.contains(key));
        snapshot.drain(..removed);
    }
    snapshot
}

/// Removes the single oldest entry, returning whether one was removed.
fn remove_oldest<T>(entries: &mut HashMap<String, Stored<T>>) -> bool {
    let Some(oldest) = entries
        .iter()
        .min_by_key(|(_, stored)| stored.fetched_at)
        .map(|(key, _)| key.clone())
    else {
        return false;
    };
    entries.remove(&oldest).is_some()
}

/// Clones the entries in a stable oldest-first order.
fn sorted_entries<T: Clone>(entries: &HashMap<String, Stored<T>>) -> Vec<StoredEntry<T>> {
    let mut sorted: Vec<StoredEntry<T>> = entries
        .iter()
        .map(|(key, stored)| StoredEntry {
            key: key.clone(),
            value: stored.value.clone(),
            fetched_at: stored.fetched_at,
        })
        .collect();
    sorted.sort_by_key(|entry| entry.fetched_at);
    sorted
}

fn load_file<T: DeserializeOwned>(path: &Path) -> Option<HashMap<String, Stored<T>>> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let file: StoredFile<T> = serde_json::from_slice(&bytes).ok()?;
    Some(
        file.entries
            .into_iter()
            .map(|entry| {
                (
                    entry.key,
                    Stored {
                        value: entry.value,
                        fetched_at: entry.fetched_at,
                    },
                )
            })
            .collect(),
    )
}

fn write_atomic(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let pending = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    if std::fs::write(&pending, bytes).is_err() {
        let _ = std::fs::remove_file(&pending);
        return;
    }
    if std::fs::rename(&pending, path).is_err() {
        let _ = std::fs::remove_file(&pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("swapper-cache-{tag}-{}", uuid::Uuid::new_v4()))
            .join("cache.json")
    }

    #[test]
    fn misses_an_unknown_key_and_hits_a_stored_one() {
        let cache: LastGoodCache<u32> = LastGoodCache::open(None, Duration::from_secs(60), 8, 4096);
        assert!(cache.lookup("missing").is_none());
        cache.store_at("ahri|mid".into(), 42, now_ms());
        let hit = cache.lookup("ahri|mid").expect("stored entry");
        assert_eq!(hit.value, 42);
        assert!(hit.fresh);
    }

    #[test]
    fn an_entry_older_than_the_ttl_is_returned_but_marked_stale() {
        // A zero TTL makes every entry immediately stale without sleeping.
        let cache: LastGoodCache<u32> = LastGoodCache::open(None, Duration::ZERO, 8, 4096);
        cache.store("key".into(), 1);
        let hit = cache.lookup("key").expect("stale entry is still returned");
        assert_eq!(hit.value, 1);
        assert!(!hit.fresh);
    }

    #[test]
    fn evicts_the_oldest_entry_past_the_count_cap() {
        let cache: LastGoodCache<u32> = LastGoodCache::open(None, Duration::from_secs(60), 2, 4096);
        cache.store_at("oldest".into(), 1, 1_000);
        cache.store_at("middle".into(), 2, 2_000);
        cache.store_at("newest".into(), 3, 3_000);
        assert_eq!(cache.len(), 2);
        assert!(cache.lookup("oldest").is_none());
        assert_eq!(cache.lookup("middle").unwrap().value, 2);
        assert_eq!(cache.lookup("newest").unwrap().value, 3);
    }

    #[test]
    fn evicts_many_entries_down_to_the_count_cap() {
        let cache: LastGoodCache<u32> = LastGoodCache::open(None, Duration::from_secs(60), 4, 1 << 20);
        for index in 0..20u32 {
            cache.store_at(format!("key-{index}"), index, 1_000 + index as i64);
        }
        assert_eq!(cache.len(), 4);
        // The four newest fetched_at values survive.
        for index in 16..20u32 {
            assert_eq!(cache.lookup(&format!("key-{index}")).unwrap().value, index);
        }
        for index in 0..16u32 {
            assert!(cache.lookup(&format!("key-{index}")).is_none());
        }
    }

    #[test]
    fn evicts_to_fit_the_byte_cap() {
        // Room for roughly one short string entry.
        let cache: LastGoodCache<String> =
            LastGoodCache::open(None, Duration::from_secs(60), 8, 120);
        cache.store_at("a".into(), "x".repeat(20), 1_000);
        cache.store_at("b".into(), "y".repeat(20), 2_000);
        cache.store_at("c".into(), "z".repeat(20), 3_000);
        // The newest entry always survives; the oldest is dropped to fit.
        assert!(cache.lookup("c").is_some());
        assert!(cache.lookup("a").is_none());
    }

    #[test]
    fn persists_and_reloads_across_instances() {
        let path = temp_path("roundtrip");
        {
            let cache: LastGoodCache<Vec<i64>> = LastGoodCache::open(
                Some(path.clone()),
                Duration::from_secs(60),
                8,
                64 * 1024,
            );
            cache.store_at(
                "ahri|middle|emerald_plus|8112".into(),
                vec![3118, 3020, 4645],
                1_000,
            );
        }
        let reopened: LastGoodCache<Vec<i64>> = LastGoodCache::open(
            Some(path.clone()),
            Duration::from_secs(60),
            8,
            64 * 1024,
        );
        let hit = reopened.lookup("ahri|middle|emerald_plus|8112").expect("reloaded entry");
        assert_eq!(hit.value, vec![3118, 3020, 4645]);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn an_invalid_file_is_treated_as_empty() {
        let path = temp_path("invalid");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json").unwrap();
        let cache: LastGoodCache<u32> = LastGoodCache::open(Some(path.clone()), Duration::from_secs(60), 8, 4096);
        assert!(cache.is_empty());
        assert!(cache.lookup("anything").is_none());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
