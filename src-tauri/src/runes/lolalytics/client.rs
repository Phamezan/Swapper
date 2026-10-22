//! The lolalytics HTTP client and its in-memory success/failure cache.

use super::*;

/// A cached lookup result.
pub enum Cached<T = KeystoneBuild> {
    Fresh(T),
    Unavailable,
    Miss,
}

/// Entries kept per cache; past this the oldest are dropped, so a long
/// session cannot grow the cache without bound.
pub(super) const MAX_CACHE_ENTRIES: usize = 256;

/// Successes and failures for keystone builds, keyed by champion/lane/tier/
/// keystone. Matchup pages use the same cache with their own value type.
pub struct BuildCache<T = KeystoneBuild> {
    success: std::collections::HashMap<String, (Instant, T)>,
    failures: std::collections::HashMap<String, Instant>,
}

impl<T> Default for BuildCache<T> {
    fn default() -> Self {
        Self {
            success: std::collections::HashMap::new(),
            failures: std::collections::HashMap::new(),
        }
    }
}

impl<T: Clone> BuildCache<T> {
    pub fn lookup(&self, key: &str) -> Cached<T> {
        self.lookup_at(key, Instant::now())
    }

    pub fn lookup_at(&self, key: &str, now: Instant) -> Cached<T> {
        if let Some((at, build)) = self.success.get(key) {
            if now.saturating_duration_since(*at) < SUCCESS_TTL {
                return Cached::Fresh(build.clone());
            }
        }
        if let Some(at) = self.failures.get(key) {
            if now.saturating_duration_since(*at) < FAILURE_TTL {
                return Cached::Unavailable;
            }
        }
        Cached::Miss
    }

    pub fn store(&mut self, key: String, build: T) {
        self.failures.remove(&key);
        self.success.insert(key, (Instant::now(), build));
        self.prune(Instant::now());
    }

    pub fn fail(&mut self, key: String) {
        self.failures.insert(key, Instant::now());
        self.prune(Instant::now());
    }

    /// Drops expired entries, then the oldest ones past [`MAX_CACHE_ENTRIES`].
    fn prune(&mut self, now: Instant) {
        self.success
            .retain(|_, (at, _)| now.saturating_duration_since(*at) < SUCCESS_TTL);
        self.failures
            .retain(|_, at| now.saturating_duration_since(*at) < FAILURE_TTL);
        while self.success.len() > MAX_CACHE_ENTRIES {
            let Some(oldest) = self
                .success
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.success.remove(&oldest);
        }
        while self.failures.len() > MAX_CACHE_ENTRIES {
            let Some(oldest) = self
                .failures
                .iter()
                .min_by_key(|(_, at)| **at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.failures.remove(&oldest);
        }
    }
}

pub struct LolalyticsClient {
    client: reqwest::Client,
}

impl LolalyticsClient {
    pub fn new() -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(3))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| ProviderError::Unavailable)?;
        Ok(Self { client })
    }

    /// Fetches and parses one keystone's build page. `Ok(None)` means the page
    /// loaded but carried no build for that filter.
    pub async fn build(
        &self,
        champion_slug: &str,
        lane: &str,
        tier: &str,
        keystone: i64,
        patch: Option<&str>,
    ) -> Result<Option<KeystoneBuilds>, ProviderError> {
        let body = self
            .page(url(champion_slug, lane, tier, keystone, patch))
            .await?;
        Ok(checked(&body, "build page", false, parse_builds(&body)))
    }

    /// The HTML of one lolalytics page. A failure is recorded as the site
    /// being unreachable; callers record whether the page parsed.
    pub(crate) async fn page(&self, url: String) -> Result<String, ProviderError> {
        let body = async {
            let response = self
                .client
                .get(url)
                .header(reqwest::header::ACCEPT, "text/html")
                .send()
                .await
                .map_err(|e| ProviderError::from_reqwest(&e))?;
            if !response.status().is_success() {
                return Err(ProviderError::from_status(response.status()));
            }
            response
                .text()
                .await
                .map_err(|_| ProviderError::Unavailable)
        }
        .await;
        if body.is_err() {
            health::unreachable(Source::Lolalytics);
        }
        body
    }
}

/// Records how a fetched page parsed and passes the result through. A page
/// without its Qwik state means lolalytics changed its format; one with the
/// state but nothing for this filter is only missing data, unless the page
/// always has data (`expect_data`).
pub(crate) fn checked<T>(
    body: &str,
    parser: &'static str,
    expect_data: bool,
    found: Option<T>,
) -> Option<T> {
    if found.is_some() {
        health::ok(Source::Lolalytics);
    } else if expect_data
        || qwik_json(body)
            .and_then(|json| serde_json::from_str::<Qwik>(json).ok())
            .is_none()
    {
        health::format_changed(Source::Lolalytics, parser);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn caches_successes_for_thirty_minutes_and_failures_for_five() {
        let now = Instant::now();
        let mut cache = BuildCache::default();
        let key = cache_key("ahri", "middle", "emerald_plus", 8112);
        assert!(matches!(cache.lookup_at(&key, now), Cached::Miss));

        cache.store(
            key.clone(),
            KeystoneBuild {
                items: vec![1, 2, 3],
                games: 10,
                ..KeystoneBuild::default()
            },
        );
        assert!(matches!(cache.lookup_at(&key, now), Cached::Fresh(_)));
        assert!(matches!(
            cache.lookup_at(&key, now + SUCCESS_TTL / 2),
            Cached::Fresh(_)
        ));
        assert!(matches!(
            cache.lookup_at(&key, now + SUCCESS_TTL + Duration::from_secs(1)),
            Cached::Miss
        ));

        let failed = cache_key("ahri", "middle", "emerald_plus", 8229);
        cache.fail(failed.clone());
        assert!(matches!(
            cache.lookup_at(&failed, Instant::now()),
            Cached::Unavailable
        ));
    }

    #[test]
    fn the_cache_is_bounded_and_drops_expired_entries() {
        let mut cache = BuildCache::<u32>::default();
        for index in 0..(MAX_CACHE_ENTRIES + 50) {
            cache.store(format!("k{index}"), index as u32);
            cache.fail(format!("f{index}"));
        }
        assert_eq!(cache.success.len(), MAX_CACHE_ENTRIES);
        assert_eq!(cache.failures.len(), MAX_CACHE_ENTRIES);
        // The newest survive; the oldest are gone.
        assert!(matches!(cache.lookup("k305"), Cached::Fresh(305)));
        assert!(matches!(cache.lookup("k0"), Cached::Miss));
        // Expired entries are pruned on the next write.
        cache.prune(Instant::now() + SUCCESS_TTL + Duration::from_secs(1));
        assert!(cache.success.is_empty() && cache.failures.is_empty());
    }
}
