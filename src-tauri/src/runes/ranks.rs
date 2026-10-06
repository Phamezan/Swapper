//! The League client's ranked mini-crest SVGs for the rank picker.
//!
//! The crests live in the client's `rcp-fe-lol-static-assets` plugin. They are
//! served from the running client when it exposes them and fall back to the
//! CommunityDragon mirror otherwise. Only a whitelisted tier is ever fetched,
//! so no caller-supplied path reaches the client or the network.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::asset_cache;
use super::RuneError;

const CDRAGON_BASE: &str =
    "https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/ranked-mini-crests";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

/// One gate per crest name so concurrent requests for a bracket share a fetch.
static RANK_FLIGHTS: OnceLock<super::Flights<String>> = OnceLock::new();

fn rank_gate(name: &str) -> Arc<tokio::sync::Mutex<()>> {
    RANK_FLIGHTS
        .get_or_init(super::Flights::new)
        .gate(&name.to_string())
}

/// Maps an op.gg tier slug to the crest asset name the client uses. The
/// broadest bracket has no tier of its own, so it borrows the unranked crest.
pub fn asset_name(tier: &str) -> Option<&'static str> {
    match tier.trim().to_ascii_lowercase().as_str() {
        "all" => Some("unranked"),
        "gold_plus" => Some("gold"),
        "platinum_plus" => Some("platinum"),
        "emerald_plus" => Some("emerald"),
        "diamond_plus" => Some("diamond"),
        "master_plus" => Some("master"),
        "challenger" => Some("challenger"),
        _ => None,
    }
}

/// The LCU paths the plugin has been seen under, tried in order.
fn lcu_paths(name: &str) -> [String; 3] {
    [
        format!("/fe/lol-static-assets/global/default/images/ranked-mini-crests/{name}.svg"),
        format!("/lol-static-assets/global/default/images/ranked-mini-crests/{name}.svg"),
        format!("/lol-static-assets/images/ranked-mini-crests/{name}.svg"),
    ]
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    bytes.windows(4).any(|window| window == b"<svg")
}

/// Returns the SVG bytes for a rank crest, cached in memory. `tier` must be one
/// of the op.gg bracket slugs; anything else is `NotFound`.
pub async fn icon(tier: &str) -> Result<Vec<u8>, RuneError> {
    let name = asset_name(tier).ok_or_else(|| RuneError::not_found("Unknown rank bracket."))?;
    if let Some(bytes) = super::shared().rank_icons.get(name) {
        return Ok(bytes.clone());
    }
    let gate = rank_gate(name);
    let _guard = gate.lock().await;
    if let Some(bytes) = super::shared().rank_icons.get(name) {
        return Ok(bytes.clone());
    }
    // A previous run's copy outlives the RAM cache, so an in-game client and a
    // down mirror no longer mean a blank crest on launch.
    if let Some(bytes) = asset_cache::read(asset_cache::Kind::Rank, name) {
        super::shared().rank_icons.insert(name.to_string(), bytes.clone());
        return Ok(bytes);
    }
    let from_lcu = match super::lcu().await {
        Ok(lcu) => {
            let mut found = None;
            for path in lcu_paths(name) {
                if let Ok(bytes) = super::lcu_get_bytes(&lcu, &path).await {
                    if looks_like_svg(&bytes) {
                        found = Some(bytes);
                        break;
                    }
                }
            }
            found
        }
        Err(_) => None,
    };
    let bytes = match from_lcu {
        Some(bytes) => bytes,
        None => from_cdragon(name).await?,
    };
    asset_cache::write(asset_cache::Kind::Rank, name, &bytes);
    super::shared().rank_icons.insert(name.to_string(), bytes.clone());
    Ok(bytes)
}

async fn from_cdragon(name: &str) -> Result<Vec<u8>, RuneError> {
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| RuneError::unavailable("Could not create the rank crest client."))?;
    let url = format!("{CDRAGON_BASE}/{name}.svg");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the rank crest."))?;
    if !response.status().is_success() {
        return Err(RuneError::not_found("That rank crest is unavailable."));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the rank crest."))?;
    if !looks_like_svg(&bytes) {
        return Err(RuneError::not_found("That rank crest is unavailable."));
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelists_only_the_offered_brackets() {
        assert_eq!(asset_name("all"), Some("unranked"));
        assert_eq!(asset_name("gold_plus"), Some("gold"));
        assert_eq!(asset_name("PLATINUM_PLUS"), Some("platinum"));
        assert_eq!(asset_name("emerald_plus"), Some("emerald"));
        assert_eq!(asset_name("diamond_plus"), Some("diamond"));
        assert_eq!(asset_name("master_plus"), Some("master"));
        assert_eq!(asset_name("challenger"), Some("challenger"));
        // Bare tier names are not slugs the picker sends.
        assert_eq!(asset_name("gold"), None);
        assert_eq!(asset_name("grandmaster_plus"), None);
        // No arbitrary paths or traversal survive.
        assert_eq!(asset_name("../../secret"), None);
        assert_eq!(asset_name(""), None);
    }

    #[test]
    fn recognises_svg_bodies() {
        assert!(looks_like_svg(b"<svg xmlns=\"...\">"));
        assert!(looks_like_svg(b"<?xml?>\n<svg>"));
        assert!(!looks_like_svg(b"<html></html>"));
        assert!(!looks_like_svg(b""));
    }
}
