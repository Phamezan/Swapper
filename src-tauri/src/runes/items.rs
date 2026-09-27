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
use std::sync::{Arc, OnceLock};
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

/// One gate per item id, so concurrent icon requests for the same item share a
/// single fetch instead of each missing the cache at once.
static ITEM_FLIGHTS: OnceLock<super::Flights<i64>> = OnceLock::new();
/// Serializes catalog loads so concurrent misses fetch the file once.
static CATALOG_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn catalog_lock() -> &'static tokio::sync::Mutex<()> {
    CATALOG_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn icon_gate(id: i64) -> Arc<tokio::sync::Mutex<()>> {
    ITEM_FLIGHTS.get_or_init(super::Flights::new).gate(&id)
}

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

/// The item catalog, indexed for O(1) icon-path and name lookups so an icon
/// request never clones the whole list to find one entry.
#[derive(Default)]
pub struct Catalog {
    icons: HashMap<i64, String>,
    names: Arc<HashMap<i64, String>>,
}

impl Catalog {
    /// Builds the index from the parsed game-data list, keeping only items the
    /// client can serve: named, with a safe asset path where the file has one.
    pub fn build(items: Vec<Item>) -> Self {
        let mut icons = HashMap::new();
        let mut names = HashMap::new();
        for item in items.iter().filter(|item| item.is_playable()) {
            names.insert(item.id, item.name.clone());
            if let Some(path) = lcu_asset_path(&item.icon_path) {
                icons.insert(item.id, path);
            }
        }
        Self {
            icons,
            names: Arc::new(names),
        }
    }

    /// The client asset path for an item id, when the catalog has one.
    pub fn icon_path(&self, id: i64) -> Option<&str> {
        self.icons.get(&id).map(String::as_str)
    }

    /// The id-to-name map, shared with the view layer without cloning it.
    pub fn names(&self) -> Arc<HashMap<i64, String>> {
        Arc::clone(&self.names)
    }
}

/// The item catalog from the League client, cached for the session. Concurrent
/// misses wait on one fetch.
pub async fn catalog() -> Result<Arc<Catalog>, RuneError> {
    if let Some((at, catalog)) = super::shared().items.as_ref() {
        if at.elapsed() < super::CATALOG_TTL {
            return Ok(Arc::clone(catalog));
        }
    }
    let _guard = catalog_lock().lock().await;
    if let Some((at, catalog)) = super::shared().items.as_ref() {
        if at.elapsed() < super::CATALOG_TTL {
            return Ok(Arc::clone(catalog));
        }
    }
    let lcu = super::lcu().await?;
    let text = super::lcu_get_text(&lcu, ITEMS_PATH).await?;
    let catalog = Arc::new(Catalog::build(parse(&text)?));
    super::shared().items = Some((Instant::now(), Arc::clone(&catalog)));
    Ok(catalog)
}

/// Item id to name, shared as an `Arc` so a caller never clones the map.
pub async fn names() -> Arc<HashMap<i64, String>> {
    match catalog().await {
        Ok(catalog) => catalog.names(),
        Err(_) => Arc::new(HashMap::new()),
    }
}

/// The CommunityDragon mirror of the item file, cached, used only when the LCU
/// is unreachable.
async fn cdragon_catalog() -> Result<Arc<Catalog>, RuneError> {
    if let Some((at, catalog)) = super::shared().item_fallback.as_ref() {
        if at.elapsed() < super::CATALOG_TTL {
            return Ok(Arc::clone(catalog));
        }
    }
    let _guard = catalog_lock().lock().await;
    if let Some((at, catalog)) = super::shared().item_fallback.as_ref() {
        if at.elapsed() < super::CATALOG_TTL {
            return Ok(Arc::clone(catalog));
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
    let catalog = Arc::new(Catalog::build(parse(&body)?));
    super::shared().item_fallback = Some((Instant::now(), Arc::clone(&catalog)));
    Ok(catalog)
}

/// The PNG bytes for an item icon, cached in memory. Only a positive numeric id
/// is accepted; everything else is `NotFound`. Concurrent requests for the same
/// id share one fetch.
pub async fn icon(id: i64) -> Result<Vec<u8>, RuneError> {
    if !valid_id(id) {
        return Err(RuneError::not_found("Unknown item."));
    }
    if let Some(bytes) = super::shared().item_icons.get(&id) {
        return Ok(bytes.clone());
    }
    let gate = icon_gate(id);
    let _guard = gate.lock().await;
    if let Some(bytes) = super::shared().item_icons.get(&id) {
        return Ok(bytes.clone());
    }
    let path = match catalog().await {
        Ok(catalog) => catalog.icon_path(id).map(str::to_string),
        Err(_) => None,
    };
    let bytes = match path {
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
    let catalog = cdragon_catalog().await?;
    let path = catalog
        .icon_path(id)
        .ok_or_else(|| RuneError::not_found("Unknown item."))?;
    let url = cdragon_asset_url(path).ok_or_else(|| RuneError::not_found("Unknown item."))?;
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

    #[test]
    fn catalog_indexes_icon_paths_and_names_for_playable_items_only() {
        let catalog = Catalog::build(parse(FIXTURE).unwrap());
        assert_eq!(
            catalog.icon_path(1001),
            Some("/lol-game-data/assets/ASSETS/Items/Icons2D/1001_Class_T1_BootsofSpeed.png")
        );
        assert!(catalog.icon_path(1056).is_some());
        // The unnamed placeholder and the zero id are not indexed.
        assert!(catalog.icon_path(9999).is_none());
        assert!(catalog.icon_path(0).is_none());
        assert!(catalog.icon_path(123456).is_none());

        let names = catalog.names();
        assert_eq!(names.get(&1001).map(String::as_str), Some("Boots"));
        assert_eq!(names.get(&1056).map(String::as_str), Some("Doran's Ring"));
        assert!(names.get(&9999).is_none());
    }

    /// Live benchmark against a running League client. The first 10 ids are
    /// fetched serially, then 50 distinct ids at once, so a per-call client or
    /// a missing single-flight shows up as latency or failures. Run with
    /// `cargo test --manifest-path src-tauri/Cargo.toml bench_item_icons -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs a running League client"]
    async fn bench_item_icons_against_the_live_client() {
        use futures_util::future::join_all;
        use std::time::Instant;

        if super::super::lcu().await.is_err() {
            eprintln!("League Client is not running; skipping.");
            return;
        }
        let catalog = match catalog().await {
            Ok(catalog) => catalog,
            Err(error) => {
                eprintln!("item catalog unavailable: {error}");
                return;
            }
        };
        let mut ids: Vec<i64> = catalog.icons.keys().copied().collect();
        ids.sort_unstable();
        if ids.len() < 60 {
            eprintln!("only {} item ids available; skipping.", ids.len());
            return;
        }
        let serial = &ids[0..10];
        let parallel = &ids[10..60];

        let start = Instant::now();
        for id in serial {
            assert!(icon(*id).await.is_ok(), "serial icon {id} failed");
        }
        let serial_ms = start.elapsed().as_millis();

        let start = Instant::now();
        let results = join_all(parallel.iter().map(|id| icon(*id))).await;
        let parallel_ms = start.elapsed().as_millis();
        let failures = results.iter().filter(|result| result.is_err()).count();

        eprintln!(
            "serial 10 = {serial_ms}ms ({:.1}ms each); parallel 50 = {parallel_ms}ms; failures = {failures}",
            serial_ms as f64 / 10.0
        );
        assert_eq!(failures, 0, "no icon fetch may fail under load");
    }
}
