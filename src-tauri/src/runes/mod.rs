//! Rune presets and the rune editor, shared by the desktop flyout and the
//! phone remote.
//!
//! The module is deliberately self-contained: it discovers the League client
//! with the same helpers as the rest of Swapper, reads op.gg for statistics,
//! and owns the League rune pages it creates. Nothing here touches the user's
//! own pages.
//!
//! Responsibilities are split so no single file carries the whole feature:
//! [`data`] loads op.gg and League data, [`view`] builds the flyout and phone
//! view models, [`apply`] writes the single owned page, and [`watch`] polls
//! champion select for the auto-apply setting. This file holds the LCU
//! plumbing and champion-select context they share.

pub mod apply;
mod asset_cache;
pub mod cache;
pub mod champion_views;
pub mod champion_icon;
pub mod counters;
pub mod counters_view;
pub mod data;
pub mod items;
pub mod item_sets;
pub mod lolalytics;
pub mod matchup;
pub mod matchup_view;
pub mod overview;
pub mod opgg;
pub mod page;
pub mod perks;
pub mod prefetch;
pub mod probuilds;
pub mod provider;
pub mod ranks;
pub mod roles;
pub mod session;
pub mod spells;
pub mod stats;
pub mod tierlist;
pub mod view;
pub mod watch;

use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lcu::{self, LcuEndpoint};

pub use apply::{apply_selection, apply_top, spawn_preset_items_import};
pub use data::icon;
pub use opgg::{normalize_tier, tier_slug, DEFAULT_TIER};
pub use page::{CatalogIndex, LcuPage, RuneSelection};
use provider::{DataKind, ProviderError};
pub use champion_views::{
    champion_overview_view, tier_list_view, ChampionOverviewView, TierListView,
};
pub use counters_view::{champion_counters_view, champion_list, ChampionCountersView, ChampionOption};
pub use matchup_view::{matchup_view, MatchupView};
pub use view::{
    preset_build_view, pro_builds_view, view, KeystoneBuildView, ProBuildsView, RunesView,
};
pub use watch::{
    apply_spells_enabled, auto_apply_current, auto_apply_enabled, configured_tier, current_status,
    import_items_enabled,
    spawn_watch, ChampSelectEvent,
};

const PHASE_PATH: &str = "/lol-gameflow/v1/gameflow-phase";
const GAMEFLOW_PATH: &str = "/lol-gameflow/v1/session";
const SESSION_PATH: &str = "/lol-champ-select/v1/session";
const PAGES_PATH: &str = "/lol-perks/v1/pages";
const CURRENT_PAGE_PATH: &str = "/lol-perks/v1/currentpage";
const INVENTORY_PATH: &str = "/lol-perks/v1/inventory";
const REGION_PATH: &str = "/riotclient/region-locale";
const PERKS_PATH: &str = "/lol-game-data/assets/v1/perks.json";
const PERKSTYLES_PATH: &str = "/lol-game-data/assets/v1/perkstyles.json";
const CHAMPIONS_PATH: &str = "/lol-game-data/assets/v1/champion-summary.json";

const GROUP_TTL: Duration = Duration::from_secs(15 * 60);
const CATALOG_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_ICON_BYTES: usize = 512 * 1024;
/// How long a discovered LCU endpoint is trusted before it is re-discovered.
/// A League restart is also picked up sooner by the retry in [`Lcu::send`].
const LCU_ENDPOINT_TTL: Duration = Duration::from_secs(10);

/// A failure that carries enough meaning for the HTTP layer to pick a status.
#[derive(Debug, Clone)]
pub enum RuneError {
    /// League, op.gg, or the network could not answer.
    Unavailable(String),
    /// The request was understood but League's rules or the page list refused it.
    Conflict(String),
    /// The rune or asset does not exist.
    NotFound(String),
}

impl RuneError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// A classified third-party provider failure, reduced to a short message.
    pub fn provider(error: ProviderError, kind: DataKind) -> Self {
        Self::Unavailable(error.message(kind))
    }

    pub fn message(&self) -> &str {
        match self {
            RuneError::Unavailable(message)
            | RuneError::Conflict(message)
            | RuneError::NotFound(message) => message,
        }
    }
}

impl fmt::Display for RuneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for RuneError {}

// ---------------------------------------------------------------------------
// LCU plumbing
// ---------------------------------------------------------------------------

/// The one native-tls client every LCU call shares, so the Windows certificate
/// store is read once and connections are reused. Building a client per call
/// (and re-discovering the endpoint) is what made an uncached icon cost ~80ms.
static LCU_CLIENT: OnceLock<Client> = OnceLock::new();
/// The last discovered endpoint, valid for [`LCU_ENDPOINT_TTL`]. Discovery runs
/// a Toolhelp process snapshot, so it must not happen on every request.
static LCU_ENDPOINT: Mutex<Option<(Instant, LcuEndpoint)>> = Mutex::new(None);

pub(crate) struct Lcu {
    client: Client,
    endpoint: LcuEndpoint,
}

fn lcu_client() -> Result<&'static Client, RuneError> {
    if let Some(client) = LCU_CLIENT.get() {
        return Ok(client);
    }
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| RuneError::unavailable("Could not reach the League Client."))?;
    Ok(LCU_CLIENT.get_or_init(|| client))
}

fn cached_endpoint() -> Option<LcuEndpoint> {
    LCU_ENDPOINT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .filter(|(at, _)| at.elapsed() < LCU_ENDPOINT_TTL)
        .map(|(_, endpoint)| endpoint.clone())
}

fn store_endpoint(endpoint: &LcuEndpoint) {
    *LCU_ENDPOINT
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some((Instant::now(), endpoint.clone()));
}

fn invalidate_endpoint() {
    *LCU_ENDPOINT
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
}

async fn discover_endpoint() -> Result<LcuEndpoint, RuneError> {
    tokio::task::spawn_blocking(lcu::discover)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| RuneError::unavailable("League Client is not connected."))
}

/// Resolves the shared client and a cached endpoint, discovering only when the
/// cache is empty or stale. The returned [`Lcu`] is a cheap snapshot; a stale
/// endpoint is retried by [`Lcu::send`].
async fn lcu() -> Result<Lcu, RuneError> {
    let client = lcu_client()?.clone();
    let endpoint = match cached_endpoint() {
        Some(endpoint) => endpoint,
        None => {
            let endpoint = discover_endpoint().await?;
            store_endpoint(&endpoint);
            endpoint
        }
    };
    Ok(Lcu { client, endpoint })
}

impl Lcu {
    /// Sends an authorized request. When the cached endpoint is stale — the
    /// connection fails or League answers 401 — it is re-discovered once and
    /// the request retried, so a League restart (new port or password) is
    /// picked up without every caller paying for discovery.
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<reqwest::Response, RuneError> {
        let mut client = self.client.clone();
        let mut endpoint = self.endpoint.clone();
        for retried in [false, true] {
            let current = Lcu {
                client: client.clone(),
                endpoint: endpoint.clone(),
            };
            let mut request = authorized(&current, method.clone(), path);
            if let Some(body) = body {
                request = request.json(body);
            }
            match request.send().await {
                Ok(response)
                    if !retried && response.status() == reqwest::StatusCode::UNAUTHORIZED =>
                {
                    invalidate_endpoint();
                    let fresh = lcu().await?;
                    client = fresh.client;
                    endpoint = fresh.endpoint;
                }
                Ok(response) => return Ok(response),
                Err(error) if !retried && error.is_connect() => {
                    invalidate_endpoint();
                    let fresh = lcu().await?;
                    client = fresh.client;
                    endpoint = fresh.endpoint;
                }
                Err(_) => return Err(RuneError::unavailable("League Client did not respond.")),
            }
        }
        Err(RuneError::unavailable("League Client did not respond."))
    }
}

/// Idle gates are dropped when the map reaches this size.
const GATE_PRUNE_AT: usize = 64;

/// Keyed single-flight gates. Concurrent cache misses for the same key await
/// one fetch; the winner stores the value and the waiters then find it. Callers
/// for different keys never block each other.
pub(crate) struct Flights<K> {
    gates: Mutex<HashMap<K, Arc<tokio::sync::Mutex<()>>>>,
}

impl<K: Eq + Hash + Clone> Flights<K> {
    pub(crate) fn new() -> Self {
        Self {
            gates: Mutex::new(HashMap::new()),
        }
    }

    /// The gate for `key`. Hold its async lock, re-check the cache, then fetch.
    pub(crate) fn gate(&self, key: &K) -> Arc<tokio::sync::Mutex<()>> {
        let mut gates = self.gates.lock().unwrap_or_else(|error| error.into_inner());
        // Drop gates nobody holds (only the map owns them) before the map grows.
        if gates.len() >= GATE_PRUNE_AT && !gates.contains_key(key) {
            gates.retain(|_, gate| Arc::strong_count(gate) > 1);
        }
        gates.entry(key.clone()).or_default().clone()
    }
}

fn authorized(lcu: &Lcu, method: Method, path: &str) -> reqwest::RequestBuilder {
    lcu.client
        .request(method, lcu::api_url(&lcu.endpoint, path))
        .header(reqwest::header::AUTHORIZATION, lcu::authorization(&lcu.endpoint))
}

async fn lcu_get_text(lcu: &Lcu, path: &str) -> Result<String, RuneError> {
    let response = lcu.send(Method::GET, path, None).await?;
    if !response.status().is_success() {
        return Err(RuneError::unavailable(format!(
            "League Client returned HTTP {}.",
            response.status().as_u16()
        )));
    }
    response
        .text()
        .await
        .map_err(|_| RuneError::unavailable("League Client response was unreadable."))
}

async fn lcu_get<T: for<'de> Deserialize<'de>>(lcu: &Lcu, path: &str) -> Result<T, RuneError> {
    let text = lcu_get_text(lcu, path).await?;
    serde_json::from_str(&text)
        .map_err(|_| RuneError::unavailable("League Client sent an unexpected response."))
}

async fn lcu_get_bytes(lcu: &Lcu, path: &str) -> Result<Vec<u8>, RuneError> {
    let response = lcu.send(Method::GET, path, None).await?;
    let response = response
        .error_for_status()
        .map_err(|_| RuneError::not_found("That rune asset is unavailable."))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|_| RuneError::unavailable("League Client response was unreadable."))?;
    if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
        return Err(RuneError::not_found("That rune asset is unavailable."));
    }
    Ok(bytes.to_vec())
}

async fn lcu_send(
    lcu: &Lcu,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Option<Value>, RuneError> {
    let response = lcu.send(method, path, body.as_ref()).await?;
    if !response.status().is_success() {
        return Err(RuneError::conflict(format!(
            "League rejected the rune page (HTTP {}).",
            response.status().as_u16()
        )));
    }
    let text = response
        .text()
        .await
        .map_err(|_| RuneError::unavailable("League Client response was unreadable."))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| RuneError::unavailable("League Client sent an unexpected response."))
}

// ---------------------------------------------------------------------------
// Session + region mapping
// ---------------------------------------------------------------------------

struct RuneContext {
    phase: String,
    context: Option<session::ChampSelectContext>,
}

async fn rune_context() -> Result<RuneContext, RuneError> {
    let lcu = lcu().await?;
    let phase: String = lcu_get(&lcu, PHASE_PATH).await?;
    if phase != "ChampSelect" {
        return Ok(RuneContext {
            phase,
            context: None,
        });
    }
    let text = lcu_get_text(&lcu, SESSION_PATH).await?;
    let Some(mut context) = session::parse_champ_select(&text)? else {
        return Ok(RuneContext {
            phase,
            context: None,
        });
    };
    if let Ok(gameflow) = lcu_get_text(&lcu, GAMEFLOW_PATH).await {
        if let Ok(info) = session::parse_gameflow(&gameflow) {
            context.map_id = info.map_id;
            context.queue_id = info.queue_id;
            if !info.game_mode.trim().is_empty() {
                context.game_mode = info.game_mode;
            }
        }
    }
    if context.game_mode.trim().is_empty() {
        context.game_mode = default_game_mode(context.mode()).to_string();
    }
    context.clear_enemy_without_lane();
    Ok(RuneContext {
        phase,
        context: Some(context),
    })
}

/// The Riot `gameMode` used when the gameflow does not report one, from the
/// op.gg mode slug.
fn default_game_mode(mode: &str) -> &'static str {
    match mode {
        opgg::MODE_ARAM => "ARAM",
        opgg::MODE_ARENA => "CHERRY",
        _ => "CLASSIC",
    }
}

/// Maps a Riot region code (e.g. "EUW") to an op.gg region slug.
pub fn region_slug(region: &str) -> Option<&'static str> {
    match region.trim().to_ascii_uppercase().as_str() {
        "EUW" | "EUW1" => Some("euw"),
        "EUN" | "EUNE" | "EUN1" => Some("eune"),
        "NA" | "NA1" => Some("na"),
        "KR" => Some("kr"),
        "JP" | "JP1" => Some("jp"),
        "BR" | "BR1" => Some("br"),
        "LA1" | "LAN" => Some("lan"),
        "LA2" | "LAS" => Some("las"),
        "OC" | "OC1" | "OCE" => Some("oce"),
        "TR" | "TR1" => Some("tr"),
        "RU" => Some("ru"),
        "VN" | "VN2" => Some("vn"),
        "TW" | "TW2" => Some("tw"),
        "TH" | "TH2" => Some("th"),
        "SG" | "SG2" => Some("sg"),
        "PH" | "PH2" => Some("ph"),
        "ME1" => Some("me"),
        _ => None,
    }
}

async fn region_for(lcu: &Lcu) -> String {
    #[derive(Deserialize)]
    struct RegionLocale {
        #[serde(default)]
        region: String,
    }
    if let Ok(text) = lcu_get_text(lcu, REGION_PATH).await {
        if let Ok(locale) = serde_json::from_str::<RegionLocale>(&text) {
            if let Some(slug) = region_slug(&locale.region) {
                return slug.to_string();
            }
        }
    }
    opgg::DEFAULT_REGION.to_string()
}

async fn position_for(_lcu: &Lcu, context: &session::ChampSelectContext) -> &'static str {
    if let Some(position) = context.position() {
        return position;
    }
    // A roleless ranked context is commonly Practice Tool. Keep the current
    // default Swapper has always inferred there, while allowing the user to
    // choose a different lane from the rune screen.
    "jungle"
}

// ---------------------------------------------------------------------------
// Shared caches and the applied page
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedView {
    pub name: String,
    pub champion_id: i64,
    pub primary_page_id: i64,
    pub secondary_page_id: i64,
    pub keystone: i64,
    pub primary_runes: Vec<i64>,
    pub secondary_runes: Vec<i64>,
    pub shards: Vec<i64>,
    pub preset_index: Option<usize>,
    /// The League page id Swapper owns, so callers can persist ownership.
    pub page_id: Option<i64>,
    /// Whether this page came from the auto-apply setting, so the UI can label
    /// the notice and keep manual applies unlabelled.
    #[serde(default)]
    pub auto_applied: bool,
}

/// Why an op.gg champion lookup produced nothing, remembered so a locked
/// champion select does not retry op.gg on every poll. The two cases are kept
/// apart because an empty answer replays as an empty result, while a failed
/// lookup replays as the provider error (or the last-good data).
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupMiss {
    /// op.gg answered, but had no rune pages for this champion and role.
    Empty,
    /// op.gg could not be reached or answered with an error.
    Failed,
}

struct Shared {
    groups: HashMap<String, (Instant, opgg::ChampionData)>,
    /// Failed or empty op.gg lookups, so a locked champion select does not
    /// retry op.gg on every poll.
    group_failures: HashMap<String, (Instant, GroupMiss)>,
    catalog: Option<(Instant, perks::PerkCatalog)>,
    names: Option<(Instant, HashMap<i64, String>)>,
    icons: HashMap<i64, Vec<u8>>,
    /// The client's position SVGs, keyed by plugin asset name.
    role_icons: HashMap<String, Vec<u8>>,
    /// The ranked crest SVGs, keyed by crest asset name.
    rank_icons: HashMap<String, Vec<u8>>,
    /// The item catalog, its id-to-icon-path index and its id-to-name map, plus
    /// the CommunityDragon mirror used when the LCU is unreachable.
    items: Option<(Instant, Arc<items::Catalog>)>,
    item_icons: HashMap<i64, Vec<u8>>,
    item_fallback: Option<(Instant, Arc<items::Catalog>)>,
    /// The summoner-spell catalog and its icons.
    spells: Option<(Instant, Vec<spells::Spell>)>,
    spell_icons: HashMap<i64, Vec<u8>>,
    applied: Option<AppliedView>,
    /// pros' solo-queue games from probuildstats, cached per champion/role/page.
    pro_builds: probuilds::MatchCache,
    /// Per-keystone item builds from lolalytics, cached per filter.
    lolalytics: lolalytics::BuildCache,
    /// Lane matchup builds from lolalytics, cached per champion/enemy/lane.
    matchups: lolalytics::BuildCache<matchup::Matchup>,
    /// Per-champion matchup tables from lolalytics.
    counters: lolalytics::BuildCache<counters::Counters>,
    /// Champion overviews and lane tier lists from lolalytics.
    overviews: lolalytics::BuildCache<overview::Overview>,
    tierlists: lolalytics::BuildCache<tierlist::TierList>,
    champion_icons: HashMap<i64, Vec<u8>>,
}

fn shared() -> MutexGuard<'static, Shared> {
    static STATE: OnceLock<Mutex<Shared>> = OnceLock::new();
    STATE
        .get_or_init(|| {
            Mutex::new(Shared {
                groups: HashMap::new(),
                group_failures: HashMap::new(),
                catalog: None,
                names: None,
                icons: HashMap::new(),
                role_icons: HashMap::new(),
                rank_icons: HashMap::new(),
                items: None,
                item_fallback: None,
                item_icons: HashMap::new(),
                spells: None,
                spell_icons: HashMap::new(),
                applied: None,
                pro_builds: probuilds::MatchCache::default(),
                lolalytics: lolalytics::BuildCache::default(),
                matchups: lolalytics::BuildCache::default(),
                counters: lolalytics::BuildCache::default(),
                overviews: lolalytics::BuildCache::default(),
                tierlists: lolalytics::BuildCache::default(),
                champion_icons: HashMap::new(),
            })
        })
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_riot_regions_to_opgg_slugs() {
        assert_eq!(region_slug("EUW"), Some("euw"));
        assert_eq!(region_slug("eun1"), Some("eune"));
        assert_eq!(region_slug("NA1"), Some("na"));
        assert_eq!(region_slug("KR"), Some("kr"));
        assert_eq!(region_slug("unknown-region"), None);
    }

    #[test]
    fn idle_gates_are_dropped_but_held_ones_are_kept() {
        let flights: Flights<usize> = Flights::new();
        let held = flights.gate(&0);
        for key in 1..(GATE_PRUNE_AT * 3) {
            drop(flights.gate(&key));
        }
        let len = flights.gates.lock().unwrap().len();
        assert!(len <= GATE_PRUNE_AT, "gates grew to {len}");
        assert!(Arc::ptr_eq(&held, &flights.gate(&0)));
    }

    #[test]
    fn flights_give_one_gate_per_key() {
        let flights: Flights<&str> = Flights::new();
        assert!(Arc::ptr_eq(&flights.gate(&"a"), &flights.gate(&"a")));
        assert!(!Arc::ptr_eq(&flights.gate(&"a"), &flights.gate(&"b")));
    }
}
