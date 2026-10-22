//! League's own match-history objective icons (dragon, baron, herald, grubs, towers, inhibitors, gold, kills) from
//! the client's `rcp-fe-lol-match-history` plugin, via CommunityDragon. Only these fixed names are ever fetched;
//! copies are kept on disk so match detail shows them offline.

use std::time::Duration;

use super::asset_cache;
use super::RuneError;

const MATCH_HISTORY: &str =
    "https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-match-history/global/default";
/// The match-history plugin has no single void grub icon; the minimap's is used for grubs.
const MINIMAP: &str = "https://raw.communitydragon.org/latest/game/assets/ux/minimap/icons";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_BYTES: usize = 256 * 1024;
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Every icon the app may ask for. `-100` is the blue-side tint, `-200` the red one.
pub const FILES: &[&str] = &[
    "dragon-100",
    "dragon-200",
    "baron-100",
    "baron-200",
    "herald-100",
    "herald-200",
    "tower-100",
    "tower-200",
    "inhibitor-100",
    "inhibitor-200",
    "grub",
    "gold",
    "kills",
    "map11",
];

pub async fn icon(name: &str) -> Result<Vec<u8>, RuneError> {
    if !FILES.contains(&name) {
        return Err(RuneError::not_found("Unknown objective icon."));
    }
    if let Some(bytes) = asset_cache::read(asset_cache::Kind::Objective, name) {
        return Ok(bytes);
    }
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| RuneError::unavailable("Could not load the objective icon."))?;
    let response = client
        .get(format!(
            "{}/{name}.png",
            if name == "grub" {
                MINIMAP
            } else {
                MATCH_HISTORY
            }
        ))
        .send()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the objective icon."))?;
    if !response.status().is_success() {
        return Err(RuneError::not_found("That objective icon is unavailable."));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the objective icon."))?;
    if !is_png(
        &bytes,
        if name == "map11" {
            768 * 1024
        } else {
            MAX_BYTES
        },
    ) {
        return Err(RuneError::not_found("That objective icon is unavailable."));
    }
    asset_cache::write(asset_cache::Kind::Objective, name, &bytes);
    Ok(bytes.to_vec())
}

fn is_png(bytes: &[u8], max_bytes: usize) -> bool {
    bytes.len() <= max_bytes && bytes.starts_with(&PNG_MAGIC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_names_and_real_pngs_are_accepted() {
        assert!(FILES.contains(&"dragon-100"));
        assert!(!FILES.contains(&"../secret"));
        let mut png = PNG_MAGIC.to_vec();
        png.extend_from_slice(&[1, 2, 3]);
        assert!(is_png(&png, MAX_BYTES));
        assert!(!is_png(b"<html>", MAX_BYTES));
        assert!(FILES.contains(&"map11"));
        png.resize(500 * 1024, 0);
        assert!(!is_png(&png, MAX_BYTES));
        assert!(is_png(&png, 768 * 1024));
    }

    #[tokio::test]
    async fn unknown_names_never_reach_the_network() {
        assert!(icon("../../etc").await.is_err());
    }
}
