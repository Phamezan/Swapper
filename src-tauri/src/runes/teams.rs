//! Pro team logos from the probuildstats CDN.
//!
//! Unlike the role and rank assets, team names are not a fixed set, so the only
//! variable part of the request is a slug derived from the name. The slug is
//! lowercased with whitespace removed and every other character kept
//! (punctuation, accents); it is only ever appended to one fixed host, and the
//! on-disk cache re-sanitizes it. Misses are remembered in memory for the
//! session so a missing logo is not refetched on every render.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::asset_cache;
use super::RuneError;

/// The one host logos are fetched from. Only the slug varies.
const BASE: &str = "https://static.bigbrain.gg/assets/probuildstats/team_icons";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

/// One gate per slug so concurrent requests for the same team share a fetch.
static TEAM_FLIGHTS: OnceLock<super::Flights<String>> = OnceLock::new();

fn team_gate(slug: &str) -> Arc<tokio::sync::Mutex<()>> {
    TEAM_FLIGHTS
        .get_or_init(super::Flights::new)
        .gate(&slug.to_string())
}

/// The CDN slug for a team name: lowercased with whitespace removed.
///
/// Everything else is kept, which is what the CDN expects: verified live,
/// `"Gen.G Esports"` hits `gen.g` (not `geng`), `"KaBuM! Eports"` hits
/// `kabum!eports`, `"E Wie Einfach E-Sports"` hits `ewieeinfache-sports` and
/// `"Movistar KOI Fénix"` hits `movistarkoifénix`.
pub fn slug(team: &str) -> String {
    team.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The longest slug looked up; real team names are well under this.
const MAX_SLUG_CHARS: usize = 64;
/// Upper bound on remembered 404s for the session.
const MAX_REMEMBERED_MISSES: usize = 512;

/// Whether a slug is a plausible team name: bounded, has a letter or digit, and
/// holds only characters real team names use. Rejecting `/`, `?`, `#` and `%`
/// keeps the slug a single URL path segment on the fixed host, and the bound
/// keeps a remote client from growing the session caches with junk names.
fn valid_slug(slug: &str) -> bool {
    slug.chars().count() <= MAX_SLUG_CHARS
        && slug.chars().any(char::is_alphanumeric)
        && slug
            .chars()
            .all(|c| c.is_alphanumeric() || ".!-'&+_".contains(c))
}

fn looks_like_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

/// Returns the PNG bytes for a team logo, cached in memory. An empty name (or
/// one that sanitizes to nothing) is `NotFound`. A 404 is remembered for the
/// session so a team without a logo is not refetched on every render.
pub async fn icon(team: &str) -> Result<Vec<u8>, RuneError> {
    let slug = slug(team);
    if !valid_slug(&slug) {
        return Err(RuneError::not_found("Unknown team."));
    }
    if let Some(bytes) = super::shared().team_icons.get(&slug) {
        return Ok(bytes.clone());
    }
    if super::shared().team_icon_misses.contains(&slug) {
        return Err(RuneError::not_found("That team logo is unavailable."));
    }
    let gate = team_gate(&slug);
    let _guard = gate.lock().await;
    if let Some(bytes) = super::shared().team_icons.get(&slug) {
        return Ok(bytes.clone());
    }
    if super::shared().team_icon_misses.contains(&slug) {
        return Err(RuneError::not_found("That team logo is unavailable."));
    }
    // A previous run's copy outlives the RAM cache, so a restart and a down CDN
    // no longer mean a blank logo.
    if let Some(bytes) = asset_cache::read(asset_cache::Kind::Team, &slug) {
        super::shared().team_icons.insert(slug, bytes.clone());
        return Ok(bytes);
    }
    match fetch(&slug).await {
        Ok(Some(bytes)) => {
            asset_cache::write(asset_cache::Kind::Team, &slug, &bytes);
            super::shared().team_icons.insert(slug, bytes.clone());
            Ok(bytes)
        }
        // A 404 is permanent for the session; an outage is not.
        Ok(None) => {
            let mut state = super::shared();
            // ponytail: wholesale reset bounds the set against junk names from a
            // remote client; real teams without logos are only a few dozen.
            if state.team_icon_misses.len() >= MAX_REMEMBERED_MISSES {
                state.team_icon_misses.clear();
            }
            state.team_icon_misses.insert(slug);
            Err(RuneError::not_found("That team logo is unavailable."))
        }
        Err(error) => Err(error),
    }
}

/// Fetches one logo. `Ok(None)` means the CDN answered 404 (no logo for this
/// team); `Err` is a transient failure worth retrying later.
async fn fetch(slug: &str) -> Result<Option<Vec<u8>>, RuneError> {
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| RuneError::unavailable("Could not create the team logo client."))?;
    let url = format!("{BASE}/{slug}.png");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the team logo."))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(RuneError::unavailable("Could not load the team logo."));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the team logo."))?;
    if !looks_like_png(&bytes) {
        return Ok(None);
    }
    Ok(Some(bytes.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plausible_team_slugs_are_looked_up() {
        for good in ["fnatic", "gen.gesports", "kabum!eports", "anyone'slegend", "movistarkoifénix"] {
            assert!(valid_slug(good), "{good} should be allowed");
        }
        for bad in ["", "..", "../x", "a/b", "a?b", "a#b", "a%2fb", "a\\b", &"x".repeat(65)] {
            assert!(!valid_slug(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn slugs_like_the_cdn_expects() {
        assert_eq!(slug("Fnatic"), "fnatic");
        assert_eq!(slug("T1"), "t1");
        assert_eq!(slug("Karmine Corp"), "karminecorp");
        // Punctuation is kept, whitespace is not.
        assert_eq!(slug("Gen.G Esports"), "gen.gesports");
        assert_eq!(slug("KaBuM! Eports"), "kabum!eports");
        assert_eq!(slug("E Wie Einfach E-Sports"), "ewieeinfache-sports");
        // Accents survive in the URL; reqwest percent-encodes them on the wire.
        assert_eq!(slug("Movistar KOI Fénix"), "movistarkoifénix");
        assert_eq!(slug("One Trick Pony"), "onetrickpony");
        assert_eq!(slug(""), "");
        assert_eq!(slug("   "), "");
    }

    #[test]
    fn recognises_png_bodies() {
        assert!(looks_like_png(b"\x89PNG\r\n\x1a\n\x00\x00"));
        assert!(!looks_like_png(b"<svg></svg>"));
        assert!(!looks_like_png(b""));
    }
}
