//! The League client's item catalog and item icons.
//!
//! The list comes from the client's own `/lol-game-data/assets/v1/items.json`;
//! the icon for an item is the `iconPath` in that file. Both go through
//! [`super::game_data`], so with the client closed they come from the disk copy
//! or CommunityDragon; Data Dragon is the last resort for an icon.
//!
//! Only a numeric item id ever reaches this module: the icon route takes an
//! `i64`, and every path built here starts from the client's own `iconPath`
//! (`/lol-game-data/assets/...`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::RuneError;

/// The client's game-data item file.
const ITEMS_PATH: &str = "/lol-game-data/assets/v1/items.json";
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
    #[serde(default)]
    pub from: Vec<i64>,
    #[serde(default)]
    pub to: Vec<i64>,
    #[serde(default)]
    pub price_total: i64,
    #[serde(default)]
    pub categories: Vec<String>,
}

impl Item {
    /// Items with a name and an icon the client can serve. The file also carries
    /// unnamed placeholders and a few desktop-only entries.
    pub fn is_playable(&self) -> bool {
        self.id > 0 && !self.name.trim().is_empty() && !self.icon_path.trim().is_empty()
    }

    /// A last-tier item: nothing builds from it, and it has a recipe or a full
    /// item's price (Seraph's and Diadem have no recipe; Doran's items are cheap).
    pub fn is_finished(&self) -> bool {
        const FULL_ITEM_PRICE: i64 = 2000;
        self.to.is_empty()
            && (!self.from.is_empty() || self.price_total >= FULL_ITEM_PRICE)
            && !self.categories.iter().any(|c| c == "Consumable" || c == "Trinket")
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

/// An item's `(from, to)` recipe links.
type Links = (Vec<i64>, Vec<i64>);

/// The item catalog, indexed for O(1) icon-path and name lookups so an icon
/// request never clones the whole list to find one entry.
#[derive(Default)]
pub struct Catalog {
    icons: HashMap<i64, String>,
    names: Arc<HashMap<i64, String>>,
    finished: Arc<HashSet<i64>>,
    boots: HashSet<i64>,
    /// Each item's `(from, to)` recipe links.
    links: HashMap<i64, Links>,
}

impl Catalog {
    /// Builds the index from the parsed game-data list, keeping only items the
    /// client can serve: named, with a safe asset path where the file has one.
    pub fn build(items: Vec<Item>) -> Self {
        let mut icons = HashMap::new();
        let mut names = HashMap::new();
        let mut finished = HashSet::new();
        let mut boots = HashSet::new();
        let mut links = HashMap::new();
        for item in items.iter().filter(|item| item.is_playable()) {
            names.insert(item.id, item.name.clone());
            links.insert(item.id, (item.from.clone(), item.to.clone()));
            if item.is_finished() {
                finished.insert(item.id);
            }
            if item.categories.iter().any(|c| c == "Boots") {
                boots.insert(item.id);
            }
            if let Some(path) = lcu_asset_path(&item.icon_path) {
                icons.insert(item.id, path);
            }
        }
        Self {
            icons,
            names: Arc::new(names),
            finished: Arc::new(finished),
            boots,
            links,
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

    /// The ids of last-tier items; see [`Item::is_finished`].
    pub fn finished(&self) -> Arc<HashSet<i64>> {
        Arc::clone(&self.finished)
    }

    pub fn is_boots(&self, id: i64) -> bool {
        self.boots.contains(&id)
    }

    /// Whether `part` is a (possibly indirect) component of `whole`. Some
    /// upgrades (Seraph's, Diadem) list no recipe, so both directions are read.
    pub fn builds_into(&self, part: i64, whole: i64) -> bool {
        let reaches = |start: i64, target: i64, next: fn(&Links) -> &Vec<i64>| {
            let mut seen = HashSet::new();
            let mut stack = vec![start];
            while let Some(id) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                for link in self.links.get(&id).map(next).into_iter().flatten() {
                    if *link == target {
                        return true;
                    }
                    stack.push(*link);
                }
            }
            false
        };
        reaches(part, whole, |(_, to)| to) || reaches(whole, part, |(from, _)| from)
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
    let text = super::game_data::text(ITEMS_PATH).await?;
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
    let from_game_data = match path {
        Some(path) => super::game_data::bytes(&path).await,
        None => Err(RuneError::not_found("Unknown item.")),
    };
    let bytes = match from_game_data {
        Ok(bytes) => bytes,
        // CommunityDragon's raw host has outages; Riot's own CDN covers them.
        Err(_) => from_ddragon(id).await?,
    };
    super::shared().item_icons.insert(id, bytes.clone());
    Ok(bytes)
}

/// Riot's Data Dragon item icon, which needs only the id and the latest patch
/// version. Used when CommunityDragon cannot serve the icon.
async fn from_ddragon(id: i64) -> Result<Vec<u8>, RuneError> {
    static VERSION: OnceLock<String> = OnceLock::new();
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| RuneError::unavailable("Could not create the item client."))?;
    let version = match VERSION.get() {
        Some(version) => version.clone(),
        None => {
            let versions: Vec<String> = client
                .get("https://ddragon.leagueoflegends.com/api/versions.json")
                .send()
                .await
                .map_err(|_| RuneError::unavailable("Could not load the item icon."))?
                .json()
                .await
                .map_err(|_| RuneError::unavailable("Could not load the item icon."))?;
            let latest = versions
                .into_iter()
                .next()
                .ok_or_else(|| RuneError::unavailable("Could not load the item icon."))?;
            VERSION.get_or_init(|| latest).clone()
        }
    };
    let response = client
        .get(format!(
            "https://ddragon.leagueoflegends.com/cdn/{version}/img/item/{id}.png"
        ))
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

    #[test]
    fn finished_items_are_last_tier_full_items_not_components_starters_or_consumables() {
        let items = parse(r#"[
            {"id":3870,"name":"Dream Maker","iconPath":"/lol-game-data/assets/a.png","from":[3867],"to":[],"priceTotal":400,"categories":["Lane"]},
            {"id":3040,"name":"Seraph's Embrace","iconPath":"/lol-game-data/assets/b.png","from":[],"to":[],"priceTotal":2900},
            {"id":3070,"name":"Tear of the Goddess","iconPath":"/lol-game-data/assets/c.png","from":[],"to":[3003],"priceTotal":400},
            {"id":1056,"name":"Doran's Ring","iconPath":"/lol-game-data/assets/d.png","from":[],"to":[],"priceTotal":400},
            {"id":2033,"name":"Corrupting Potion","iconPath":"/lol-game-data/assets/e.png","from":[2031],"to":[],"priceTotal":500,"categories":["Consumable"]},
            {"id":3364,"name":"Oracle Lens","iconPath":"/lol-game-data/assets/f.png","from":[3340],"to":[],"priceTotal":0,"categories":["Trinket"]}
        ]"#).unwrap();
        let finished = Catalog::build(items).finished();
        let mut ids: Vec<_> = finished.iter().copied().collect();
        ids.sort();
        assert_eq!(ids, vec![3040, 3870]);
    }

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
