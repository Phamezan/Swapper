//! The League client's item catalog and item icons.
//!
//! The list comes from the client's own `/lol-game-data/assets/v1/items.json`;
//! the icon for an item is the `iconPath` in that file, fetched through the same
//! LCU proxy as the rune and spell icons. When the client is not reachable the
//! CommunityDragon mirror of the same game data is used instead, so the phone
//! page still gets item art.
//!
//! Only a numeric item id ever reaches this module: the icon route takes an
//! `i64`, and every path built here starts from the client's own `iconPath`
//! (`/lol-game-data/assets/...`) or the mirrored copy of it.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::RuneError;

/// The client's game-data item file.
const ITEMS_PATH: &str = "/lol-game-data/assets/v1/items.json";
/// The CommunityDragon mirror of the client's game-data plugin. The client's
/// `/lol-game-data/assets/` prefix maps to `.../rcp-be-lol-game-data/global/default/`.
const CDRAGON_BASE: &str =
    "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default";
const CDRAGON_ITEMS: &str = "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/items.json";
const LCU_ASSET_PREFIX: &str = "/lol-game-data/assets/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

/// One item from the client's game data.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon_path: String,
}

impl Item {
    /// Items with a name and an icon the client can serve. The file also carries
    /// unnamed placeholders and a few desktop-only entries.
    pub fn is_playable(&self) -> bool {
        self.id > 0 && !self.name.trim().is_empty() && !self.icon_path.trim().is_empty()
    }
}

/// Whether an id could name a real item. Guards the icon route and the caches
/// from zero, negative, and nonsensical ids.
pub fn valid_id(id: i64) -> bool {
    id > 0
}

/// Parses the client's item game-data file.
pub fn parse(body: &str) -> Result<Vec<Item>, RuneError> {
    serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read item data: {e}")))
}

/// The safe LCU asset path for an item, or `None` when it is not an asset the
/// client's own game-data file points at.
fn lcu_asset_path(icon_path: &str) -> Option<String> {
    let path = icon_path.trim();
    if !path.starts_with(LCU_ASSET_PREFIX) || path.contains("..") {
        return None;
    }
    Some(path.to_string())
}

/// The CommunityDragon URL mirroring an LCU asset path. CommunityDragon lower
/// cases its paths, so `ASSETS/Items/Icons2D/...` becomes `assets/items/icons2d/...`.
fn cdragon_asset_url(icon_path: &str) -> Option<String> {
    let relative = icon_path.trim().strip_prefix(LCU_ASSET_PREFIX)?;
    if relative.is_empty() || relative.contains("..") {
        return None;
    }
    Some(format!("{CDRAGON_BASE}/{}", relative.to_ascii_lowercase()))
}

/// The item catalog from the League client, cached for the session.
pub async fn catalog() -> Result<Vec<Item>, RuneError> {
    if let Some((at, items)) = super::shared().items.as_ref() {
        if at.elapsed() < super::CATALOG_TTL {
            return Ok(items.clone());
        }
    }
    let lcu = super::lcu().await?;
    let text = super::lcu_get_text(&lcu, ITEMS_PATH).await?;
    let items = parse(&text)?;
    super::shared().items = Some((Instant::now(), items.clone()));
    Ok(items)
}

/// Item id to name, cached. Used to label build and pro-build icons without a
/// request per id.
pub async fn names() -> HashMap<i64, String> {
    if let Some((at, names)) = super::shared().item_names.as_ref() {
        if at.elapsed() < super::CATALOG_TTL {
            return names.clone();
        }
    }
    let map: HashMap<i64, String> = catalog()
        .await
        .map(|items| {
            items
                .into_iter()
                .filter(Item::is_playable)
                .map(|item| (item.id, item.name))
                .collect()
        })
        .unwrap_or_default();
    super::shared().item_names = Some((Instant::now(), map.clone()));
    map
}

/// The CommunityDragon mirror of the item file, cached, used only when the LCU
/// is unreachable.
async fn cdragon_catalog() -> Result<Vec<Item>, RuneError> {
    if let Some((at, items)) = super::shared().item_fallback.as_ref() {
        if at.elapsed() < super::CATALOG_TTL {
            return Ok(items.clone());
        }
    }
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| RuneError::unavailable("Could not create the item client."))?;
    let response = client
        .get(CDRAGON_ITEMS)
        .send()
        .await
        .map_err(|_| RuneError::unavailable("Could not load item data."))?;
    if !response.status().is_success() {
        return Err(RuneError::unavailable("Could not load item data."));
    }
    let body = response
        .text()
        .await
        .map_err(|_| RuneError::unavailable("Could not load item data."))?;
    let items = parse(&body)?;
    super::shared().item_fallback = Some((Instant::now(), items.clone()));
    Ok(items)
}

/// The PNG bytes for an item icon, cached in memory. Only a positive numeric id
/// is accepted; everything else is `NotFound`.
pub async fn icon(id: i64) -> Result<Vec<u8>, RuneError> {
    if !valid_id(id) {
        return Err(RuneError::not_found("Unknown item."));
    }
    if let Some(bytes) = super::shared().item_icons.get(&id) {
        return Ok(bytes.clone());
    }
    let lcu_path = catalog()
        .await
        .ok()
        .and_then(|items| {
            items
                .into_iter()
                .find(|item| item.id == id && item.is_playable())
                .and_then(|item| lcu_asset_path(&item.icon_path))
        });
    let bytes = match lcu_path {
        Some(path) => {
            let lcu = super::lcu().await?;
            super::lcu_get_bytes(&lcu, &path).await?
        }
        None => from_cdragon(id).await?,
    };
    super::shared().item_icons.insert(id, bytes.clone());
    Ok(bytes)
}

async fn from_cdragon(id: i64) -> Result<Vec<u8>, RuneError> {
    let items = cdragon_catalog().await?;
    let item = items
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| RuneError::not_found("Unknown item."))?;
    let url = cdragon_asset_url(&item.icon_path)
        .ok_or_else(|| RuneError::not_found("Unknown item."))?;
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| RuneError::unavailable("Could not create the item client."))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the item icon."))?;
    if !response.status().is_success() {
        return Err(RuneError::not_found("That item icon is unavailable."));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| RuneError::unavailable("Could not load the item icon."))?;
    if bytes.is_empty() || bytes.len() > super::MAX_ICON_BYTES {
        return Err(RuneError::not_found("That item icon is unavailable."));
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"[
        {"id":1001,"name":"Boots","iconPath":"/lol-game-data/assets/ASSETS/Items/Icons2D/1001_Class_T1_BootsofSpeed.png"},
        {"id":1056,"name":"Doran's Ring","iconPath":"/lol-game-data/assets/ASSETS/Items/Icons2D/1056_Mage_T1_DoransRing.png"},
        {"id":9999,"name":"","iconPath":"/lol-game-data/assets/v1/item-icons/9999.png"},
        {"id":0,"name":"Unnamed","iconPath":""}
    ]"#;

    #[test]
    fn parses_the_item_file_and_drops_unplayable_entries() {
        let items = parse(FIXTURE).expect("fixture should parse");
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].name, "Boots");
        assert_eq!(
            items[0].icon_path,
            "/lol-game-data/assets/ASSETS/Items/Icons2D/1001_Class_T1_BootsofSpeed.png"
        );
        let playable: Vec<i64> = items
            .iter()
            .filter(|item| item.is_playable())
            .map(|item| item.id)
            .collect();
        assert_eq!(playable, vec![1001, 1056]);
        assert!(parse("not json").is_err());
    }

    #[test]
    fn only_positive_numeric_item_ids_are_valid() {
        assert!(valid_id(1001));
        assert!(valid_id(6672));
        assert!(!valid_id(0));
        assert!(!valid_id(-1));
    }

    #[test]
    fn builds_an_asset_path_only_from_the_client_prefix() {
        assert_eq!(
            lcu_asset_path("/lol-game-data/assets/v1/item-icons/1001.png").as_deref(),
            Some("/lol-game-data/assets/v1/item-icons/1001.png")
        );
        assert!(lcu_asset_path("https://example.com/evil.png").is_none());
        assert!(lcu_asset_path("/lol-game-data/assets/../secret").is_none());
        assert!(lcu_asset_path("").is_none());
    }

    #[test]
    fn mirrors_an_asset_path_to_community_dragon_in_lower_case() {
        assert_eq!(
            cdragon_asset_url("/lol-game-data/assets/ASSETS/Items/Icons2D/1001_Class_T1_BootsofSpeed.png")
                .as_deref(),
            Some("https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/assets/items/icons2d/1001_class_t1_bootsofspeed.png")
        );
        assert!(cdragon_asset_url("/somewhere/else/1001.png").is_none());
        assert!(cdragon_asset_url("/lol-game-data/assets/../secret.png").is_none());
        assert!(cdragon_asset_url("").is_none());
    }
}
