//! RuneForge client: browse/search, mod detail and download-to-temp.
//!
//! Undocumented API at runeforge.dev (verified live 2026-11):
//! - List: `GET /api/mods?page=1&pageSize=24&search=&categories[]=champion_skin
//!   &champions[]=103&sortBy=trending`. Array params use the bracket form
//!   (`categories[]=`); a plain `categories=` value returns 400. `sortBy` is
//!   one of `trending`, `download_count`, `recently_published`,
//!   `recently_updated`, `like_count`, `view_count`, `relevance`.
//! - `GET /api/mods/{uuid}` requires auth (401 anonymously), so detail comes
//!   from the site's own React Router loaders: `GET /mods/{slug}.data` and
//!   `GET /mods/{slug}/releases.data` (turbo-stream/devalue payloads).
//! - Download: `POST /api/mods/{mod}/releases/{release}/download-link` with
//!   `{artifactId}` returns a short-lived signed path. That POST and the file
//!   GET are crawler-gated, so only they present as a browser (Chrome UA +
//!   Origin/Referer). Everything else uses an honest `Swapper/<version>` UA.
//!   Keep both header sets here so they are easy to change.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

pub const BASE: &str = "https://runeforge.dev";
const HONEST_UA: &str = concat!("Swapper/", env!("CARGO_PKG_VERSION"));
const BROWSER_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
pub const UNREACHABLE: &str = "RuneForge isn't reachable right now.";
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const LIST_CACHE_TTL: Duration = Duration::from_secs(600);
const THUMB_CACHE_MAX: usize = 200;

/// RuneForge category values (from `GET /api/mods/category-counts`);
/// mirrored by the Browse filter in the frontend.
#[allow(dead_code)]
pub const CATEGORIES: &[(&str, &str)] = &[
    ("champion_skin", "Skins"),
    ("map_skin", "Maps"),
    ("ui", "HUD"),
    ("sfx", "Sounds"),
    ("vfx", "VFX"),
    ("font", "Fonts"),
    ("loading_screen", "Loading screens"),
    ("voiceover", "Voiceover"),
    ("announcer", "Announcer"),
    ("miscellaneous", "Miscellaneous"),
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfChampion {
    pub id: u64,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfMod {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub author: String,
    pub category: Option<String>,
    pub champions: Vec<String>,
    pub status: Option<String>,
    pub thumbnail_url: Option<String>,
    pub download_count: u64,
    pub view_count: u64,
    pub like_count: u64,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub licence: Option<String>,
    pub page_url: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfList {
    pub mods: Vec<RfMod>,
    pub total: u64,
    pub page: u32,
    pub champions: Vec<RfChampion>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfArtifact {
    pub id: String,
    pub filename: String,
    pub format: String,
    pub size: u64,
    pub primary: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfRelease {
    pub id: String,
    pub tag: String,
    pub game_patch: Option<String>,
    pub created_at: Option<String>,
    pub artifacts: Vec<RfArtifact>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfDetail {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub author: String,
    pub category: Option<String>,
    pub champions: Vec<String>,
    pub status: Option<String>,
    pub thumbnail_url: Option<String>,
    /// Preview images in display order (runeforge hosts only); falls back to the thumbnail.
    pub gallery: Vec<String>,
    pub download_count: u64,
    pub view_count: u64,
    pub like_count: u64,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub licence: Option<String>,
    pub page_url: String,
    pub tags: Vec<String>,
    pub releases: Vec<RfRelease>,
    pub latest_release_id: Option<String>,
}

pub struct ListQuery {
    pub page: u32,
    pub search: String,
    pub category: Option<String>,
    pub champion_id: Option<u64>,
    pub sort: String, // UI sort key: trending | downloads | newest | updated
}

pub fn sort_api_value(sort: &str) -> &'static str {
    match sort {
        "downloads" => "download_count",
        "newest" => "recently_published",
        "updated" => "recently_updated",
        _ => "trending",
    }
}

pub fn page_url(slug: &str) -> String {
    format!("{BASE}/mods/{slug}")
}

pub fn thumbnail_url(thumbnail_key: Option<&str>) -> Option<String> {
    let key = thumbnail_key.map(str::trim).filter(|k| !k.is_empty())?;
    Some(format!(
        "{BASE}/cdn-cgi/image/width=600,height=400,quality=85,format=webp,fit=contain,anim=false/https://r2-images-prod.runeforge.dev/{key}"
    ))
}

fn is_runeforge_host(url: &url::Url) -> bool {
    matches!(url.host_str(), Some(h) if h == "runeforge.dev" || h.ends_with(".runeforge.dev"))
}

/// Redirects may only stay on runeforge.dev hosts (the signed download 302s
/// to r2-prod.runeforge.dev); anything else is refused.
fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > 5 {
            attempt.error("too many redirects")
        } else if is_runeforge_host(attempt.url()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

fn honest_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(HONEST_UA)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(redirect_policy())
        .build()
        .map_err(|_| UNREACHABLE.to_string())
}

fn browser_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(BROWSER_UA)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .redirect(redirect_policy())
        .build()
        .map_err(|_| UNREACHABLE.to_string())
}

fn build_list_url(query: &ListQuery) -> String {
    // RuneForge pages are 0-based; ours start at 1. A search sorted by "trending" only
    // returns recently trending hits (19 of 1,050 for "default"), so it ranks by relevance like the site.
    let searching = !query.search.trim().is_empty();
    let sort = if searching && query.sort == "trending" { "relevance" } else { sort_api_value(&query.sort) };
    let mut url = format!("{BASE}/api/mods?page={}&pageSize=24&sortBy={sort}", query.page.max(1) - 1);
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    if !query.search.trim().is_empty() {
        url.push_str(&format!("&search={}", enc(query.search.trim())));
    }
    if let Some(cat) = query.category.as_deref().filter(|c| !c.is_empty()) {
        url.push_str(&format!("&categories[]={}", enc(cat)));
    }
    if let Some(id) = query.champion_id {
        url.push_str(&format!("&champions[]={id}"));
    }
    url
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty())
}

fn u64_field(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn tag_strings(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(tags) = v.get("tags").and_then(Value::as_array) {
        for tag in tags {
            if let Some(s) = tag.as_str() {
                out.push(s.to_string());
            } else if let Some(s) = str_field(tag, "slug").or_else(|| str_field(tag, "name")) {
                out.push(s);
            }
        }
    }
    if let Some(features) = v.get("features").and_then(Value::as_array) {
        for f in features {
            if let Some(s) = f.as_str() {
                out.push(s.to_string());
            }
        }
    }
    out
}

fn champion_names(v: &Value) -> Vec<String> {
    v.get("champions")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|c| str_field(c, "name").or_else(|| c.as_str().map(str::to_string)))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_list_mod(v: &Value) -> Option<RfMod> {
    let id = str_field(v, "id")?;
    let slug = str_field(v, "slug")?;
    let name = str_field(v, "name")?;
    let author = v
        .get("publisher")
        .and_then(|p| str_field(p, "username"))
        .unwrap_or_default();
    Some(RfMod {
        page_url: page_url(&slug),
        id,
        slug,
        name,
        author,
        category: str_field(v, "category"),
        champions: champion_names(v),
        status: str_field(v, "status"),
        thumbnail_url: thumbnail_url(str_field(v, "thumbnailKey").as_deref()),
        download_count: u64_field(v, "downloadCount"),
        view_count: u64_field(v, "viewCount"),
        like_count: u64_field(v, "likeCount"),
        published_at: str_field(v, "publishedAt").or_else(|| str_field(v, "createdAt")),
        updated_at: str_field(v, "updatedAt"),
        licence: None,
        tags: tag_strings(v),
    })
}

pub fn parse_list_body(body: &str) -> Result<(Vec<RfMod>, u64), String> {
    let root: Value = serde_json::from_str(body).map_err(|_| UNREACHABLE.to_string())?;
    let mods = root
        .get("mods")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(parse_list_mod).collect())
        .unwrap_or_default();
    Ok((mods, u64_field(&root, "total")))
}

fn parse_champions_body(body: &str) -> Vec<RfChampion> {
    let Ok(root) = serde_json::from_str::<Value>(body) else { return Vec::new() };
    let mut out: Vec<RfChampion> = root
        .get("champions")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|c| {
                    Some(RfChampion { id: c.get("id")?.as_u64()?, name: str_field(c, "name")? })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

static LIST_CACHE: Mutex<Option<HashMap<String, (Instant, RfList)>>> = Mutex::new(None);
static CHAMPIONS: Mutex<Option<Vec<RfChampion>>> = Mutex::new(None);

pub async fn list_mods(query: &ListQuery) -> Result<RfList, String> {
    let url = build_list_url(query);
    if let Ok(guard) = LIST_CACHE.lock() {
        if let Some(map) = guard.as_ref() {
            if let Some((at, cached)) = map.get(&url) {
                if at.elapsed() < LIST_CACHE_TTL {
                    let mut hit = cached.clone();
                    hit.champions = champions_cached();
                    return Ok(hit);
                }
            }
        }
    }
    let champions = champions().await;
    let client = honest_client()?;
    let body = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| UNREACHABLE.to_string())?
        .error_for_status()
        .map_err(|_| UNREACHABLE.to_string())?
        .text()
        .await
        .map_err(|_| UNREACHABLE.to_string())?;
    let (mods, total) = parse_list_body(&body)?;
    let list = RfList { mods, total, page: query.page.max(1), champions: champions.clone() };
    if let Ok(mut guard) = LIST_CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        if map.len() > 64 {
            map.clear();
        }
        map.insert(url, (Instant::now(), list.clone()));
    }
    Ok(list)
}

fn champions_cached() -> Vec<RfChampion> {
    CHAMPIONS.lock().map(|g| g.clone().unwrap_or_default()).unwrap_or_default()
}

pub async fn champions() -> Vec<RfChampion> {
    if let Ok(guard) = CHAMPIONS.lock() {
        if let Some(list) = guard.as_ref() {
            return list.clone();
        }
    }
    let Ok(client) = honest_client() else { return Vec::new() };
    let Ok(resp) = client.get(format!("{BASE}/api/champions")).header("Accept", "application/json").send().await else {
        return Vec::new();
    };
    let Ok(body) = resp.text().await else { return Vec::new() };
    let list = parse_champions_body(&body);
    if !list.is_empty() {
        if let Ok(mut guard) = CHAMPIONS.lock() {
            *guard = Some(list.clone());
        }
    }
    list
}

// ---- turbo-stream / devalue decoding (untrusted input, fail closed) ----

/// Decodes a React Router `.data` payload into plain JSON. Chunks are a flat
/// array; object keys are `_<chunk index>` and values are chunk indices,
/// with negative indices meaning "no value". Deferred `P<n>:...` lines only
/// append chunks. Returns `Null` on anything unexpected.
pub fn decode_data_payload(text: &str) -> Value {
    let mut lines = text.lines();
    let Some(first) = lines.next() else { return Value::Null };
    let Ok(mut chunks) = serde_json::from_str::<Vec<Value>>(first) else { return Value::Null };
    if chunks.is_empty() || chunks.len() > 20_000 {
        return Value::Null;
    }
    for line in lines.take(64) {
        if let Some(payload) = line.split_once(':').map(|(_, p)| p) {
            if let Ok(extra) = serde_json::from_str::<Vec<Value>>(payload) {
                if chunks.len() + extra.len() <= 20_000 {
                    chunks.extend(extra);
                }
            }
        }
    }
    decode_chunk(&chunks, 0, 0, &mut HashSet::new())
}

fn decode_chunk(chunks: &[Value], idx: i64, depth: usize, stack: &mut HashSet<usize>) -> Value {
    if idx < 0 || idx as usize >= chunks.len() || depth > 48 {
        return Value::Null;
    }
    let idx = idx as usize;
    let value = &chunks[idx];
    match value {
        Value::Object(map) => {
            if !stack.insert(idx) {
                return Value::Null;
            }
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                let name = key
                    .strip_prefix('_')
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|n| chunks.get(n))
                    .and_then(Value::as_str)
                    .unwrap_or(key.as_str())
                    .to_string();
                let decoded = match val {
                    Value::Number(n) => n
                        .as_i64()
                        .map(|i| decode_chunk(chunks, i, depth + 1, stack))
                        .unwrap_or(Value::Null),
                    other => other.clone(),
                };
                out.insert(name, decoded);
            }
            stack.remove(&idx);
            Value::Object(out)
        }
        Value::Array(items) => {
            if !stack.insert(idx) {
                return Value::Null;
            }
            let out = items
                .iter()
                .map(|item| match item {
                    Value::Number(n) => match n.as_i64() {
                        Some(i) if i >= 0 && (i as usize) < chunks.len() && matches!(chunks[i as usize], Value::Object(_) | Value::Array(_)) => {
                            decode_chunk(chunks, i, depth + 1, stack)
                        }
                        _ => item.clone(),
                    },
                    other => other.clone(),
                })
                .collect();
            stack.remove(&idx);
            Value::Array(out)
        }
        other => other.clone(),
    }
}

/// First object in the tree (depth-first) containing `key`.
fn find_object<'a>(value: &'a Value, key: &str, depth: usize) -> Option<&'a Value> {
    if depth > 24 {
        return None;
    }
    match value {
        Value::Object(map) => {
            if map.contains_key(key) {
                return Some(value);
            }
            map.values().find_map(|v| find_object(v, key, depth + 1))
        }
        Value::Array(items) => items.iter().find_map(|v| find_object(v, key, depth + 1)),
        _ => None,
    }
}

fn collect_objects(value: &Value, key: &str, depth: usize, out: &mut Vec<Value>) {
    if depth > 24 || out.len() > 64 {
        return;
    }
    match value {
        Value::Object(map) => {
            if map.contains_key(key) {
                out.push(value.clone());
            }
            for v in map.values() {
                collect_objects(v, key, depth + 1, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_objects(v, key, depth + 1, out);
            }
        }
        _ => {}
    }
}

/// The author only arrives in a deferred payload chunk, so scan every
/// chunk object for a `username` when the mod tree itself has none.
fn username_in_payload(text: &str) -> Option<String> {
    let mut lines = text.lines();
    let first = lines.next()?;
    let mut chunks: Vec<Value> = serde_json::from_str(first).ok()?;
    for line in lines.take(64) {
        if let Some(payload) = line.split_once(':').map(|(_, p)| p) {
            if let Ok(extra) = serde_json::from_str::<Vec<Value>>(payload) {
                chunks.extend(extra);
            }
        }
    }
    for (i, chunk) in chunks.iter().enumerate() {
        if chunk.is_object() {
            let decoded = decode_chunk(&chunks, i as i64, 0, &mut HashSet::new());
            if let Some(name) = str_field(&decoded, "username") {
                return Some(name);
            }
        }
    }
    None
}

/// Gallery image URLs sorted by `orderIndex`, runeforge hosts only; `[thumbnail]` when none survive.
fn gallery_urls(mod_root: &Value, thumbnail: Option<&str>) -> Vec<String> {
    let mut images: Vec<(u64, String)> = find_object(mod_root, "galleryImages", 0)
        .and_then(|o| o.get("galleryImages"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|g| {
                    let raw = str_field(g, "imageUrl")?;
                    let full = if raw.starts_with("http") { raw } else { format!("{BASE}{raw}") };
                    let parsed = url::Url::parse(&full).ok()?;
                    (parsed.scheme() == "https" && is_runeforge_host(&parsed)).then(|| (u64_field(g, "orderIndex"), full))
                })
                .collect()
        })
        .unwrap_or_default();
    images.sort_by_key(|(order, _)| *order);
    let urls: Vec<String> = images.into_iter().map(|(_, u)| u).collect();
    if urls.is_empty() { thumbnail.map(str::to_string).into_iter().collect() } else { urls }
}

pub fn parse_detail(mod_body: &str, releases_body: &str, slug: &str) -> Result<RfDetail, String> {
    let mod_root = decode_data_payload(mod_body);
    let rel_root = decode_data_payload(releases_body);
    if mod_root.is_null() && rel_root.is_null() {
        return Err(UNREACHABLE.to_string());
    }
    let meta = find_object(&mod_root, "licenseSpdxId", 0)
        .or_else(|| find_object(&rel_root, "licenseSpdxId", 0))
        .cloned()
        .unwrap_or(Value::Null);
    let details = find_object(&mod_root, "latestRelease", 0).cloned().unwrap_or(Value::Null);
    let mut release_objs = Vec::new();
    collect_objects(&rel_root, "artifacts", 0, &mut release_objs);
    if release_objs.is_empty() {
        collect_objects(&mod_root, "artifacts", 0, &mut release_objs);
    }
    let mut releases: Vec<RfRelease> = Vec::new();
    let mut seen = HashSet::new();
    for obj in &release_objs {
        let Some(id) = str_field(obj, "id") else { continue };
        if !seen.insert(id.clone()) {
            continue;
        }
        let artifacts = obj
            .get("artifacts")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|a| {
                        Some(RfArtifact {
                            id: str_field(a, "id")?,
                            filename: str_field(a, "filename")?,
                            format: str_field(a, "format").unwrap_or_default(),
                            size: u64_field(a, "fileSize"),
                            primary: a.get("isPrimary").and_then(Value::as_bool).unwrap_or(false),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        releases.push(RfRelease {
            id,
            tag: str_field(obj, "tag").unwrap_or_default(),
            game_patch: obj.get("gameVersion").and_then(|g| str_field(g, "version")),
            created_at: str_field(obj, "createdAt"),
            artifacts,
        });
    }
    let latest_release_id = details
        .get("latestRelease")
        .and_then(|r| str_field(r, "id"))
        .or_else(|| releases.first().map(|r| r.id.clone()));
    // Newest first, matching the releases page order.
    releases.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let username = find_object(&mod_root, "username", 0)
        .and_then(|o| str_field(o, "username"))
        .or_else(|| username_in_payload(mod_body))
        .unwrap_or_default();
    let id = str_field(&meta, "id")
        .or_else(|| details.get("latestRelease").and_then(|r| str_field(r, "modId")))
        .unwrap_or_default();
    let thumbnail = find_object(&mod_root, "thumbnailUrl", 0)
        .and_then(|o| str_field(o, "thumbnailUrl"))
        .map(|u| if u.starts_with("http") { u } else { format!("{BASE}{u}") })
        .or_else(|| thumbnail_url(str_field(&meta, "thumbnailKey").as_deref()));
    Ok(RfDetail {
        gallery: gallery_urls(&mod_root, thumbnail.as_deref()),
        page_url: page_url(slug),
        id,
        slug: slug.to_string(),
        name: str_field(&meta, "name").unwrap_or_default(),
        author: username,
        category: str_field(&meta, "category"),
        champions: champion_names(&details),
        status: str_field(&meta, "status"),
        thumbnail_url: thumbnail,
        download_count: u64_field(&details, "downloadCount"),
        view_count: u64_field(&details, "viewCount"),
        like_count: u64_field(&details, "likeCount"),
        published_at: str_field(&meta, "publishedAt").or_else(|| str_field(&meta, "createdAt")),
        updated_at: str_field(&meta, "updatedAt"),
        licence: str_field(&meta, "licenseSpdxId").or_else(|| str_field(&meta, "licenseName")),
        tags: tag_strings(&meta),
        releases,
        latest_release_id,
    })
}

pub async fn mod_detail(slug: &str) -> Result<RfDetail, String> {
    if slug.is_empty() || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(UNREACHABLE.to_string());
    }
    let client = honest_client()?;
    let fetch = |url: String| {
        let client = client.clone();
        async move {
            client
                .get(&url)
                .send()
                .await
                .map_err(|_| UNREACHABLE.to_string())?
                .error_for_status()
                .map_err(|_| UNREACHABLE.to_string())?
                .text()
                .await
                .map_err(|_| UNREACHABLE.to_string())
        }
    };
    let (mod_body, rel_body) = tokio::join!(
        fetch(format!("{BASE}/mods/{slug}.data")),
        fetch(format!("{BASE}/mods/{slug}/releases.data"))
    );
    parse_detail(&mod_body.unwrap_or_default(), &rel_body.unwrap_or_default(), slug)
}

fn size_allowed(bytes: u64) -> bool {
    bytes <= MAX_DOWNLOAD_BYTES
}

/// True when any tag/feature names a skinhack (a paid Riot skin replica).
pub fn is_skinhack(tags: &[String]) -> bool {
    tags.iter().any(|t| {
        let t = t.to_lowercase();
        t.contains("skinhack") || t.contains("skin-hack") || t.contains("skin_hack")
    })
}

async fn download_link(mod_id: &str, release_id: &str, artifact_id: &str, slug: &str) -> Result<String, String> {
    let client = browser_client()?;
    let resp = client
        .post(format!("{BASE}/api/mods/{mod_id}/releases/{release_id}/download-link"))
        .header("Origin", BASE)
        .header("Referer", page_url(slug))
        .json(&serde_json::json!({ "artifactId": artifact_id }))
        .send()
        .await
        .map_err(|_| UNREACHABLE.to_string())?;
    if !resp.status().is_success() {
        return Err(UNREACHABLE.to_string());
    }
    let body: Value = resp.json().await.map_err(|_| UNREACHABLE.to_string())?;
    let path = str_field(&body, "path").ok_or_else(|| UNREACHABLE.to_string())?;
    if let Ok(url) = url::Url::parse(&path) {
        if url.scheme() != "https" || !is_runeforge_host(&url) {
            return Err(UNREACHABLE.to_string());
        }
        Ok(url.to_string())
    } else if path.starts_with('/') {
        Ok(format!("{BASE}{path}"))
    } else {
        Err(UNREACHABLE.to_string())
    }
}

/// Downloads one artifact to a temp file (browser headers, size-capped).
/// Returns the temp path; the caller validates and then removes it.
pub async fn download_to_temp(
    detail: &RfDetail,
    release: &RfRelease,
    artifact: &RfArtifact,
) -> Result<PathBuf, String> {
    if !size_allowed(artifact.size) {
        return Err("That mod is too large to download.".to_string());
    }
    let signed = download_link(&detail.id, &release.id, &artifact.id, &detail.slug).await?;
    let client = browser_client()?;
    let resp = client
        .get(&signed)
        .header("Origin", BASE)
        .header("Referer", page_url(&detail.slug))
        .send()
        .await
        .map_err(|_| UNREACHABLE.to_string())?
        .error_for_status()
        .map_err(|_| UNREACHABLE.to_string())?;
    if resp.content_length().is_some_and(|n| !size_allowed(n)) {
        return Err("That mod is too large to download.".to_string());
    }
    let bytes = resp.bytes().await.map_err(|_| UNREACHABLE.to_string())?;
    if !size_allowed(bytes.len() as u64) {
        return Err("That mod is too large to download.".to_string());
    }
    let ext = Path::new(&artifact.filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("fantome")
        .to_lowercase();
    let dir = std::env::temp_dir().join("swapper-runeforge");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}-{}.{}", detail.id, artifact.id, ext));
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn thumb_cache_dir(data_root: &Path) -> PathBuf {
    data_root.join("mods").join("cache")
}

fn thumb_key(url: &str) -> String {
    crate::mods::library::sha256_hex(url.as_bytes())[..16].to_string()
}

/// Cached thumbnail as a data URL. Downloads once (honest UA), then serves
/// the file under `<data_root>/mods/cache/`, pruned to the newest files.
pub async fn thumbnail_data_url(data_root: &Path, url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(url).map_err(|_| UNREACHABLE.to_string())?;
    if parsed.scheme() != "https" || !is_runeforge_host(&parsed) {
        return Err(UNREACHABLE.to_string());
    }
    let dir = thumb_cache_dir(data_root);
    let file = dir.join(format!("thumb-{}.bin", thumb_key(url)));
    if let Ok(bytes) = std::fs::read(&file) {
        if !bytes.is_empty() && bytes.len() < 2 * 1024 * 1024 {
            return Ok(data_url(&bytes));
        }
    }
    let client = honest_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|_| UNREACHABLE.to_string())?
        .error_for_status()
        .map_err(|_| UNREACHABLE.to_string())?;
    let bytes = resp.bytes().await.map_err(|_| UNREACHABLE.to_string())?;
    if bytes.is_empty() || bytes.len() > 2 * 1024 * 1024 {
        return Err(UNREACHABLE.to_string());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _ = std::fs::write(&file, &bytes);
    prune_thumb_cache(&dir);
    Ok(data_url(&bytes))
}

pub(super) fn data_url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8") {
        "image/jpeg"
    } else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/webp"
    };
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn prune_thumb_cache(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("thumb-"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|(t, _)| *t);
    while files.len() > THUMB_CACHE_MAX {
        let (_, path) = files.remove(0);
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_fixture_parses() {
        let body = include_str!("fixtures/list.json");
        let (mods, total) = parse_list_body(body).expect("list parses");
        assert_eq!(total, 58);
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name, "League of Merry Legends: Jolly Hats Edition (PC)");
        assert_eq!(mods[0].author, "Aropatnik");
        assert_eq!(mods[0].category.as_deref(), Some("champion_skin"));
        assert!(mods[0].champions.contains(&"Ahri".to_string()));
        assert_eq!(mods[0].status.as_deref(), Some("working"));
        assert!(mods[0].thumbnail_url.as_deref().unwrap().contains("r2-images-prod.runeforge.dev"));
        assert_eq!(mods[0].download_count, 1695);
        assert_eq!(mods[0].like_count, 7);
        assert_eq!(mods[0].published_at.as_deref(), Some("2025-11-30T17:33:42.088+00:00"));
        assert!(mods[0].updated_at.is_some());
        assert_eq!(mods[0].page_url, "https://runeforge.dev/mods/league-of-merry-legends-jolly-hats-edition-pc");
    }

    #[test]
    fn list_parsing_is_defensive() {
        let (mods, total) = parse_list_body(r#"{"mods":[{"id":1},{"name":"x"},null,{"id":"a","slug":"s","name":"n","publisher":"oops","champions":"Ahri","downloadCount":"many"}],"total":"lots"}"#).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].author, "");
        assert_eq!(total, 0);
        assert!(parse_list_body("not json").is_err());
        assert_eq!(parse_list_body("{}").unwrap().0.len(), 0);
    }

    #[test]
    fn detail_fixtures_parse() {
        let detail = parse_detail(
            include_str!("fixtures/mod.data.txt"),
            include_str!("fixtures/releases.data.txt"),
            "old-mastery-icons",
        )
        .expect("detail parses");
        assert_eq!(detail.name, "Old Mastery Icons");
        assert_eq!(detail.author, "uygarexe");
        assert_eq!(detail.licence.as_deref(), Some("MIT"));
        assert_eq!(detail.status.as_deref(), Some("working"));
        assert_eq!(detail.id, "0b92a98c-0504-47dd-b1da-7eef73534e44");
        assert_eq!(detail.latest_release_id.as_deref(), Some("579d31cb-e43e-40a1-84c8-03f06b1d2a0b"));
        assert_eq!(detail.releases.len(), 1);
        let release = &detail.releases[0];
        assert_eq!(release.tag, "v1.0.2");
        assert_eq!(release.game_patch.as_deref(), Some("16.22"));
        assert_eq!(release.artifacts.len(), 1);
        assert_eq!(release.artifacts[0].filename, "Old Mastery Emotes 1.0.2.fantome");
        assert!(release.artifacts[0].primary);
    }

    #[test]
    fn detail_parsing_is_defensive() {
        assert!(parse_detail("", "", "x").is_err());
        assert!(parse_detail("garbage", "[]", "x").is_err());
        assert!(parse_detail("[]", "[]", "x").is_err());
        let detail = parse_detail(r#"[{"_1":2,"_3":4},"licenseSpdxId","MIT","name",""]"#, "[]", "some-slug").unwrap();
        assert_eq!(detail.name, "");
        assert_eq!(detail.licence.as_deref(), Some("MIT"));
        assert!(detail.releases.is_empty());
    }

    #[test]
    fn query_url_uses_verified_encodings() {
        let url = build_list_url(&ListQuery {
            page: 2,
            search: "ahri skin".into(),
            category: Some("champion_skin".into()),
            champion_id: Some(103),
            sort: "downloads".into(),
        });
        assert!(url.contains("page=1&"), "RuneForge pages are 0-based: {url}");
        assert!(url.contains("categories[]=champion_skin"), "{url}");
        assert!(url.contains("champions[]=103"), "{url}");
        assert!(url.contains("sortBy=download_count"), "{url}");
        assert!(url.contains("search=ahri+skin"), "{url}");
        assert_eq!(sort_api_value("newest"), "recently_published");
        assert_eq!(sort_api_value("updated"), "recently_updated");
        assert_eq!(sort_api_value("trending"), "trending");
        let search = build_list_url(&ListQuery { page: 1, search: "default".into(), category: None, champion_id: None, sort: "trending".into() });
        assert!(search.contains("page=0&") && search.contains("sortBy=relevance"), "{search}");
    }

    #[test]
    fn gallery_is_ordered_and_filtered() {
        let root: Value = serde_json::from_str(r#"{"mod":{"galleryImages":[
            {"imageUrl":"/cdn-cgi/image/width=1280/https://r2-images-prod.runeforge.dev/b.png","orderIndex":1},
            {"imageUrl":"https://evil.example.com/x.png","orderIndex":2},
            {"imageUrl":"/cdn-cgi/image/width=1280/https://r2-images-prod.runeforge.dev/a.png","orderIndex":0},
            {"orderIndex":3}]}}"#).unwrap();
        let urls = gallery_urls(&root, Some("https://runeforge.dev/thumb.png"));
        assert_eq!(urls.len(), 2);
        assert!(urls[0].starts_with("https://runeforge.dev/cdn-cgi/") && urls[0].ends_with("/a.png"));
        assert!(urls[1].ends_with("/b.png"));
        assert_eq!(gallery_urls(&Value::Null, Some("https://runeforge.dev/thumb.png")), vec!["https://runeforge.dev/thumb.png"]);
        assert!(gallery_urls(&Value::Null, None).is_empty());
    }

    #[test]
    fn only_runeforge_hosts_pass() {
        for ok in ["https://runeforge.dev/x", "https://r2-prod.runeforge.dev/x"] {
            assert!(is_runeforge_host(&url::Url::parse(ok).unwrap()), "{ok}");
        }
        for bad in ["https://runeforge.dev.evil.io/x", "https://evil-runeforge.dev/x", "https://example.com"] {
            assert!(!is_runeforge_host(&url::Url::parse(bad).unwrap()), "{bad}");
        }
    }

    #[test]
    fn skinhack_tags_are_detected() {
        assert!(is_skinhack(&["SkinHack".into()]));
        assert!(is_skinhack(&["skin_hack".into()]));
        assert!(!is_skinhack(&["custom_model".into(), "anime".into()]));
    }

    #[test]
    fn download_size_cap() {
        assert!(size_allowed(1024));
        assert!(size_allowed(MAX_DOWNLOAD_BYTES));
        assert!(!size_allowed(MAX_DOWNLOAD_BYTES + 1));
    }
}
