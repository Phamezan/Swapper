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
pub mod data;
pub mod opgg;
pub mod page;
pub mod perks;
pub mod probuilds;
pub mod session;
pub mod stats;
pub mod view;
pub mod watch;

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lcu::{self, LcuEndpoint};

pub use apply::{apply_selection, apply_top};
pub use data::icon;
pub use opgg::{normalize_tier, tier_slug, DEFAULT_TIER};
pub use page::{CatalogIndex, LcuPage, RuneSelection};
pub use view::{pro_builds_view, view, ProBuildsView, RunesView};
pub use watch::{auto_apply_enabled, configured_tier, current_status, spawn_watch, ChampSelectEvent};

const PHASE_PATH: &str = "/lol-gameflow/v1/gameflow-phase";
const GAMEFLOW_PATH: &str = "/lol-gameflow/v1/session";
const SESSION_PATH: &str = "/lol-champ-select/v1/session";
const PAGES_PATH: &str = "/lol-perks/v1/pages";
const CURRENT_PAGE_PATH: &str = "/lol-perks/v1/currentpage";
const INVENTORY_PATH: &str = "/lol-perks/v1/inventory";
const RECOMMENDED_POSITIONS_PATH: &str = "/lol-perks/v1/recommended-champion-positions";
const REGION_PATH: &str = "/riotclient/region-locale";
const PERKS_PATH: &str = "/lol-game-data/assets/v1/perks.json";
const PERKSTYLES_PATH: &str = "/lol-game-data/assets/v1/perkstyles.json";
const CHAMPIONS_PATH: &str = "/lol-game-data/assets/v1/champion-summary.json";

const GROUP_TTL: Duration = Duration::from_secs(15 * 60);
const CATALOG_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_ICON_BYTES: usize = 512 * 1024;

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

pub(crate) struct Lcu {
    client: Client,
    endpoint: LcuEndpoint,
}

async fn lcu() -> Result<Lcu, RuneError> {
    let endpoint = tokio::task::spawn_blocking(lcu::discover)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| RuneError::unavailable("League Client is not connected."))?;
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| RuneError::unavailable("Could not reach the League Client."))?;
    Ok(Lcu { client, endpoint })
}

fn authorized(lcu: &Lcu, method: Method, path: &str) -> reqwest::RequestBuilder {
    lcu.client
        .request(method, lcu::api_url(&lcu.endpoint, path))
        .header(reqwest::header::AUTHORIZATION, lcu::authorization(&lcu.endpoint))
}

async fn lcu_get_text(lcu: &Lcu, path: &str) -> Result<String, RuneError> {
    let response = authorized(lcu, Method::GET, path)
        .send()
        .await
        .map_err(|_| RuneError::unavailable("League Client did not respond."))?;
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
    let response = authorized(lcu, Method::GET, path)
        .send()
        .await
        .map_err(|_| RuneError::unavailable("League Client did not respond."))?;
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
    let mut request = authorized(lcu, method, path);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|_| RuneError::unavailable("League Client did not respond."))?;
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
        if let Ok((map_id, queue_id)) = session::parse_gameflow(&gameflow) {
            context.map_id = map_id;
            context.queue_id = queue_id;
        }
    }
    Ok(RuneContext {
        phase,
        context: Some(context),
    })
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

async fn position_for(lcu: &Lcu, context: &session::ChampSelectContext) -> &'static str {
    if let Some(position) = context.position() {
        return position;
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Recommended {
        #[serde(default)]
        recommended_positions: Vec<String>,
    }
    if let Ok(text) = lcu_get_text(lcu, RECOMMENDED_POSITIONS_PATH).await {
        if let Ok(by_champion) = serde_json::from_str::<HashMap<String, Recommended>>(&text) {
            if let Some(entry) = by_champion.get(&context.champion_id.to_string()) {
                for position in &entry.recommended_positions {
                    if let Some(mapped) = session::position_from_assigned(position) {
                        return mapped;
                    }
                }
            }
        }
    }
    opgg::POSITION_NONE
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
}

struct Shared {
    groups: HashMap<String, (Instant, Vec<opgg::RunePageGroup>)>,
    /// Failed or empty op.gg lookups, so a locked champion select does not
    /// retry op.gg on every poll.
    group_failures: HashMap<String, Instant>,
    catalog: Option<(Instant, perks::PerkCatalog)>,
    names: Option<(Instant, HashMap<i64, String>)>,
    icons: HashMap<i64, Vec<u8>>,
    applied: Option<AppliedView>,
    /// pros' solo-queue games from probuildstats, cached per champion/role/page.
    pro_builds: probuilds::MatchCache,
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
                applied: None,
                pro_builds: probuilds::MatchCache::default(),
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
}
