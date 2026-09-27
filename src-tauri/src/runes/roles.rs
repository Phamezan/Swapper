//! The League client's own position icons.
//!
//! The five SVGs live in the client's `rcp-fe-lol-static-assets` plugin. They
//! are served from the running client when it exposes them and fall back to the
//! CommunityDragon mirror otherwise. Only these five names are ever fetched, so
//! no caller-supplied path reaches the client or the network.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::RuneError;

const CDRAGON_BASE: &str =
    "https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/svg";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

/// One gate per asset name so concurrent requests for a role share a fetch.
static ROLE_FLIGHTS: OnceLock<super::Flights<String>> = OnceLock::new();

fn role_gate(name: &str) -> Arc<tokio::sync::Mutex<()>> {
    ROLE_FLIGHTS
        .get_or_init(super::Flights::new)
        .gate(&name.to_string())
}

/// Maps a role slug, LCU `assignedPosition`, or op.gg position to the icon name
/// used by the client's static-assets plugin.
pub fn asset_name(role: &str) -> Option<&'static str> {
    match role.trim().to_ascii_lowercase().as_str() {
        "top" => Some("top"),
        "jungle" | "jg" => Some("jungle"),
        "mid" | "middle" => Some("middle"),
        "adc" | "bottom" | "bot" => Some("bottom"),
        "support" | "supp" | "utility" => Some("utility"),
        _ => None,
    }
}

/// The LCU paths the plugin has been seen under, tried in order.
fn lcu_paths(name: &str) -> [String; 3] {
    [
        format!("/fe/lol-static-assets/svg/position-{name}.svg"),
        format!("/lol-static-assets/svg/position-{name}.svg"),
        format!("/lol-static-assets/global/default/svg/position-{name}.svg"),
    ]
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    bytes.windows(4).any(|window| window == b"<svg")
}

/// Returns the SVG bytes for a role, cached in memory. `role` must be one of
/// the five known positions; anything else is `NotFound`.
pub async fn icon(role: &str) -> Result<Vec<u8>, RuneError> {
    let name = asset_name(role).ok_or_else(|| RuneError::not_found("Unknown role."))?;
    if let Some(bytes) = super::shared().role_icons.get(name) {
        return Ok(bytes.clone());
    }
    let gate = role_gate(name);
    let _guard = gate.lock().await;
    if let Some(bytes) = super::shared().role_icons.get(name) {
        return Ok(bytes.clone());
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
    super::shared().role_icons.insert(name.to_string(), bytes.clone());
    Ok(bytes)
}

async fn from_cdragon(name: &str) -> Result<Vec<u8>, RuneError> {
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| RuneError::unavailable("Could not create the role icon client."))?;
    let url = format!("{CDRAGON_BASE}/position-{name}.svg");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the role icon."))?;
    if !response.status().is_success() {
        return Err(RuneError::not_found("That role icon is unavailable."));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the role icon."))?;
    if !looks_like_svg(&bytes) {
        return Err(RuneError::not_found("That role icon is unavailable."));
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelists_only_the_five_positions() {
        assert_eq!(asset_name("top"), Some("top"));
        assert_eq!(asset_name("JUNGLE"), Some("jungle"));
        assert_eq!(asset_name("mid"), Some("middle"));
        assert_eq!(asset_name("middle"), Some("middle"));
        assert_eq!(asset_name("adc"), Some("bottom"));
        assert_eq!(asset_name("bottom"), Some("bottom"));
        assert_eq!(asset_name("utility"), Some("utility"));
        assert_eq!(asset_name("support"), Some("utility"));
        assert_eq!(asset_name("all"), None);
        assert_eq!(asset_name("none"), None);
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
