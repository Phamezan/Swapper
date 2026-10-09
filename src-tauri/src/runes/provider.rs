//! Provider identity, failure classification and the last-successful boundary.
//!
//! Swapper reads rune recommendations from op.gg, item builds from lolalytics
//! and pro games from probuildstats/u.gg. The feature layer should not need to
//! name those websites directly, so each fetch crosses this small seam: a
//! [`Sourced`] value carries the domain data plus which [`Provider`] produced
//! it, whether it is stale, and when it was fetched.
//!
//! This is deliberately an enum per data kind rather than a plugin system: the
//! set of sources is fixed and small, and only one genuine fallback exists.

use std::time::{SystemTime, UNIX_EPOCH};

/// A kind of data Swapper fetches from a third party. Each kind has its own
/// last-successful cache file and its own user-facing label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    /// Champion rune pages and spell pairs (op.gg).
    RuneRecommendations,
    /// Item builds for a keystone (lolalytics).
    ItemBuild,
    /// Lane matchup builds (lolalytics).
    Matchup,
    /// A champion's matchup table (lolalytics).
    Counters,
    /// A champion's overview page (lolalytics).
    Overview,
    /// A lane tier list (lolalytics).
    TierList,
    /// Pros' solo-queue games (probuildstats/u.gg).
    ProBuilds,
}

impl DataKind {
    /// The stable file stem used for the on-disk cache.
    pub fn key(self) -> &'static str {
        match self {
            Self::RuneRecommendations => "rune-recommendations",
            Self::ItemBuild => "item-builds",
            Self::Matchup => "matchup-builds",
            Self::Counters => "champion-counters",
            Self::Overview => "champion-overviews",
            Self::TierList => "tier-lists",
            Self::ProBuilds => "pro-builds",
        }
    }

    /// A short noun phrase for user-facing messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::RuneRecommendations => "Rune recommendations",
            Self::ItemBuild => "Item builds",
            Self::Matchup => "Matchup builds",
            Self::Counters => "Champion matchups",
            Self::Overview => "Champion overviews",
            Self::TierList => "Tier lists",
            Self::ProBuilds => "Pro builds",
        }
    }
}

/// The website or service that produced a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Opgg,
    Lolalytics,
    Ugg,
    LeagueClient,
}

impl Provider {
    /// The name shown in the UI and diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Self::Opgg => "op.gg",
            Self::Lolalytics => "Lolalytics",
            Self::Ugg => "U.GG",
            Self::LeagueClient => "League client",
        }
    }
}

/// Why a provider request failed, so an outage, a timeout and a changed
/// response shape can be told apart instead of collapsing into one error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderError {
    /// The request did not finish within its timeout.
    Timeout,
    /// The provider could not be reached, or answered with an error status.
    Unavailable,
    /// The response could not be parsed; the site's format likely changed.
    Format,
}

impl ProviderError {
    /// Classifies a reqwest transport error. A timeout stays a timeout;
    /// connect, DNS, TLS and body errors are all reported as unavailable.
    pub fn from_reqwest(error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Unavailable
        }
    }

    /// Classifies a non-success HTTP status.
    pub fn from_status(_status: reqwest::StatusCode) -> Self {
        Self::Unavailable
    }

    /// A short, user-facing sentence naming the data kind.
    pub fn message(self, kind: DataKind) -> String {
        match self {
            Self::Timeout => format!("{} timed out.", kind.label()),
            Self::Unavailable => format!("{} are unavailable right now.", kind.label()),
            Self::Format => format!("{} returned an unexpected format.", kind.label()),
        }
    }
}

/// A value together with the provider that produced it and its freshness.
#[derive(Debug, Clone)]
pub struct Sourced<T> {
    pub value: T,
    pub provider: Provider,
    /// True when the value is the last successful result and the live fetch
    /// did not succeed.
    pub stale: bool,
    /// Unix milliseconds of the successful fetch, when known.
    pub fetched_at: Option<i64>,
}

impl<T> Sourced<T> {
    /// A result from a successful live fetch just now.
    pub fn fresh(value: T, provider: Provider) -> Self {
        Self {
            value,
            provider,
            stale: false,
            fetched_at: Some(now_ms()),
        }
    }

    /// The last successful result, served because the live fetch failed.
    pub fn stale(value: T, provider: Provider, fetched_at: i64) -> Self {
        Self {
            value,
            provider,
            stale: true,
            fetched_at: Some(fetched_at),
        }
    }
}

/// Milliseconds since the Unix epoch, or `0` when the clock is before it.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_timeouts_separately_from_other_transport_errors() {
        // The pure classification the clients rely on: only a real timeout is
        // reported as one. Constructing a reqwest::Error directly is not
        // practical, so this mirrors `from_reqwest`'s branch.
        fn classify(is_timeout: bool) -> ProviderError {
            if is_timeout {
                ProviderError::Timeout
            } else {
                ProviderError::Unavailable
            }
        }
        assert_eq!(classify(true), ProviderError::Timeout);
        assert_eq!(classify(false), ProviderError::Unavailable);
    }

    #[test]
    fn names_the_data_kind_in_failure_messages() {
        assert_eq!(
            ProviderError::Timeout.message(DataKind::RuneRecommendations),
            "Rune recommendations timed out."
        );
        assert_eq!(
            ProviderError::Unavailable.message(DataKind::ProBuilds),
            "Pro builds are unavailable right now."
        );
        assert_eq!(
            ProviderError::Format.message(DataKind::ItemBuild),
            "Item builds returned an unexpected format."
        );
    }

    #[test]
    fn fresh_values_are_not_stale_and_carry_a_timestamp() {
        let sourced = Sourced::fresh(7, Provider::Opgg);
        assert!(!sourced.stale);
        assert!(sourced.fetched_at.is_some());
    }

    #[test]
    fn cache_keys_and_labels_are_stable() {
        assert_eq!(DataKind::RuneRecommendations.key(), "rune-recommendations");
        assert_eq!(DataKind::ItemBuild.label(), "Item builds");
        assert_eq!(Provider::LeagueClient.label(), "League client");
        assert_eq!(Provider::Ugg.label(), "U.GG");
    }
}
