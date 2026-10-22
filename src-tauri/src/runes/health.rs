//! Per-source health for Swapper Doctor: did the last request reach the site,
//! and did its answer still parse?
//!
//! The scraped sources are undocumented and change without notice. A changed
//! response used to look like "no data" (a stale cache or "unavailable"), so
//! each fetch records its outcome here. In memory only, for this session;
//! nothing is persisted or sent anywhere.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::provider::ProviderError;

/// A site Swapper reads, one per Doctor connection check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Opgg,
    Lolalytics,
    ProBuilds,
    UggStats,
}

const SOURCES: usize = 4;

/// The last outcomes recorded for one source. Each is a sequence number from
/// one session-wide counter, so outcomes order strictly even within one clock tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct Health {
    pub last_ok: Option<u64>,
    pub last_unreachable: Option<u64>,
    /// When a response last failed to parse, and which parser rejected it.
    pub last_format: Option<(u64, &'static str)>,
}

/// What Doctor shows for a source: whichever outcome happened last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing was fetched from this source yet this session.
    Unused,
    Ok,
    Unreachable,
    /// The site answered, but the named parser could not read the response.
    FormatChanged(&'static str),
}

impl Health {
    pub fn state(&self) -> State {
        let ok = self.last_ok.map(|at| (at, State::Ok));
        let down = self.last_unreachable.map(|at| (at, State::Unreachable));
        let format = self
            .last_format
            .map(|(at, parser)| (at, State::FormatChanged(parser)));
        [ok, down, format]
            .into_iter()
            .flatten()
            .max_by_key(|(at, _)| *at)
            .map_or(State::Unused, |(_, state)| state)
    }
}

static HEALTH: Mutex<[Health; SOURCES]> = Mutex::new(
    [Health {
        last_ok: None,
        last_unreachable: None,
        last_format: None,
    }; SOURCES],
);

static SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn update(source: Source, apply: impl FnOnce(&mut Health, u64)) {
    let mut health = HEALTH.lock().unwrap_or_else(|e| e.into_inner());
    apply(
        &mut health[source as usize],
        SEQUENCE.fetch_add(1, Ordering::Relaxed),
    );
}

pub fn ok(source: Source) {
    update(source, |health, at| health.last_ok = Some(at));
}

pub fn unreachable(source: Source) {
    update(source, |health, at| health.last_unreachable = Some(at));
}

pub fn format_changed(source: Source, parser: &'static str) {
    update(source, |health, at| health.last_format = Some((at, parser)));
}

/// Records a fetch that already classifies its failure.
pub fn record<T>(source: Source, parser: &'static str, result: &Result<T, ProviderError>) {
    match result {
        Ok(_) => ok(source),
        Err(ProviderError::Format) => format_changed(source, parser),
        Err(ProviderError::Timeout | ProviderError::Unavailable) => unreachable(source),
    }
}

pub fn get(source: Source) -> Health {
    HEALTH.lock().unwrap_or_else(|e| e.into_inner())[source as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_outcome_wins() {
        let at = |sequence| sequence;
        let mut health = Health::default();
        assert_eq!(health.state(), State::Unused);
        health.last_ok = Some(at(1));
        assert_eq!(health.state(), State::Ok);
        health.last_format = Some((at(2), "counters page"));
        assert_eq!(health.state(), State::FormatChanged("counters page"));
        health.last_unreachable = Some(at(3));
        assert_eq!(health.state(), State::Unreachable);
        health.last_ok = Some(at(4));
        assert_eq!(health.state(), State::Ok);
    }

    #[test]
    fn records_classified_results_per_source() {
        record(
            Source::ProBuilds,
            "pro match list",
            &Err::<(), _>(ProviderError::Format),
        );
        assert_eq!(
            get(Source::ProBuilds).state(),
            State::FormatChanged("pro match list")
        );
        record(
            Source::ProBuilds,
            "pro match list",
            &Err::<(), _>(ProviderError::Timeout),
        );
        assert_eq!(get(Source::ProBuilds).state(), State::Unreachable);
        record(Source::ProBuilds, "pro match list", &Ok(()));
        assert_eq!(get(Source::ProBuilds).state(), State::Ok);
    }
}
