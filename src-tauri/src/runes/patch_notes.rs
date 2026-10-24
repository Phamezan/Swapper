//! Current and previous League of Legends patch notes from Riot's official site.
//!
//! Discovery: the game-updates listing is newest-first; its first two distinct
//! `league-of-legends-patch-<major>-<minor>-notes` links are the supported patches.
//! (Riot numbers articles `26.21` while Data Dragon says `16.21`; never derive
//! one from the other.) The article page embeds its data in `__NEXT_DATA__`:
//! an `articleMasthead` blade (title, banner, publish date) and a
//! `patchNotesRichText` blade whose HTML uses stable classes
//! (`patch-change-block`, `change-title`, `change-detail-title`). That HTML is
//! parsed into plain text here — no markup ever reaches the webview.
//!
//! The parsed result is kept in a last-good disk cache, so the page opens
//! instantly and offline after the first successful load.

use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};

use super::cache::LastGoodCache;
use super::provider::now_ms;

const LISTING_URL: &str = "https://www.leagueoflegends.com/en-us/news/game-updates/";
const ARTICLE_BASE: &str = "https://www.leagueoflegends.com/en-us/news/game-updates";
const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_LISTING_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARTICLE_BYTES: usize = 6 * 1024 * 1024;
const CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const CACHE_KEY: &str = "latest";
/// Patch number, article slug, and the listing's key art for that article (when it has one).
type PatchLink = (String, String, Option<String>);

// Bounds for anything parsed out of the network response.
const MAX_SECTIONS: usize = 40;
const MAX_ENTRIES: usize = 160;
const MAX_GROUPS: usize = 48;
const MAX_LINES: usize = 80;
const MAX_PARAGRAPHS: usize = 120;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchLine {
    /// The stat or ability part before the colon, when the line has one.
    pub label: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    /// The full line when it is not an old -> new comparison.
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchGroup {
    /// Ability or sub-heading ("Q - Piltover Peacemaker"), when present.
    pub title: Option<String>,
    pub lines: Vec<PatchLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchEntry {
    /// Champion, item or sub-topic name; empty for a heading-less block.
    pub name: String,
    /// Resolved from the shared item catalog on response; old caches omit it.
    #[serde(default)]
    pub item_id: Option<i64>,
    /// Riot's short "why" paragraph, when the block has one.
    pub context: Option<String>,
    pub groups: Vec<PatchGroup>,
    /// Loose paragraphs that are not part of a change list.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSection {
    pub title: String,
    pub entries: Vec<PatchEntry>,
    pub paragraphs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchChoice {
    pub version: String,
    /// The article's key art from the news listing, for the patch picker.
    pub banner_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchNotes {
    /// Riot's patch number as printed in the article, e.g. "26.21".
    pub patch: String,
    /// Current, then previous. Attached on response, not tied to an article cache.
    #[serde(default)]
    pub available_patches: Vec<PatchChoice>,
    pub title: String,
    pub description: String,
    /// ISO publish timestamp from the article masthead.
    pub date: Option<String>,
    pub banner_url: Option<String>,
    pub article_url: String,
    /// The article's opening paragraphs.
    pub intro: Vec<String>,
    pub sections: Vec<PatchSection>,
    /// True when this is the cached copy and a refresh just failed.
    pub stale: bool,
    pub fetched_at: Option<i64>,
}

// ---------------------------------------------------------------------------
// Fetching
// ---------------------------------------------------------------------------

fn cache() -> &'static LastGoodCache<PatchNotes> {
    static CACHE: OnceLock<LastGoodCache<PatchNotes>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = crate::vault::data_root()
            .ok()
            .map(|root| root.join("runes-cache").join("patch-notes.json"));
        // One rollover slot preserves the old current as it becomes previous.
        LastGoodCache::open(path, CACHE_TTL, 3, 2 * 1024 * 1024)
    })
}

fn listing_cache() -> &'static LastGoodCache<[PatchLink; 2]> {
    static CACHE: OnceLock<LastGoodCache<[PatchLink; 2]>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = crate::vault::data_root().ok()
            .map(|root| root.join("runes-cache").join("patch-notes-index.json"));
        LastGoodCache::open(path, CACHE_TTL, 1, 4096)
    })
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(TIMEOUT)
        .user_agent("Swapper/1.0 (patch notes reader)")
        .build()
        .map_err(|_| "Could not build the patch notes client.".to_string())
}

async fn get_text(client: &reqwest::Client, url: &str, max_bytes: usize) -> Result<String, String> {
    String::from_utf8(get_bytes(client, url, max_bytes).await?).map_err(|_| "Patch notes returned an unexpected format.".to_string())
}

async fn get_bytes(client: &reqwest::Client, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                "Patch notes timed out.".to_string()
            } else {
                "Patch notes are unavailable right now.".to_string()
            }
        })?;
    if !response.status().is_success() {
        return Err("Patch notes are unavailable right now.".to_string());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Patch notes are unavailable right now.".to_string())?
    {
        if bytes.len() + chunk.len() > max_bytes {
            return Err("Patch notes returned an unexpected format.".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Key art for the patch picker, asked of Riot's image CDN at picker size.
const BANNER_SIZE: &str = "w=800&fm=jpg&q=80";
const MAX_BANNER_BYTES: usize = 512 * 1024;

fn banner_path(version: &str) -> Option<std::path::PathBuf> {
    // Versions come from the listing as digits and one dot; anything else never becomes a path.
    let safe = !version.is_empty() && version.len() <= 8 && version.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    safe.then(|| crate::vault::data_root().ok().map(|root| root.join("patch-banners").join(format!("{version}.jpg")))).flatten()
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() > 3 && bytes.len() <= MAX_BANNER_BYTES && bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// The listing's key art for one of the two selectable patches as a data URL
/// (the app's CSP allows no remote images). Cached on disk; a patch's art never changes.
pub async fn banner(version: &str) -> Result<String, String> {
    let to_url = |bytes: &[u8]| format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
    let path = banner_path(version).ok_or("Unknown patch.")?;
    if let Some(bytes) = std::fs::read(&path).ok().filter(|bytes| is_jpeg(bytes)) {
        return Ok(to_url(&bytes));
    }
    let (links, _) = patch_links(false).await?;
    let url = links.iter().find(|link| link.0 == version).and_then(|link| link.2.as_deref()).ok_or("This patch has no key art.")?;
    let sized = format!("{url}{}{BANNER_SIZE}", if url.contains('?') { "&" } else { "?" });
    let bytes = get_bytes(&client()?, &sized, MAX_BANNER_BYTES).await?;
    if !is_jpeg(&bytes) {
        return Err("Patch key art returned an unexpected format.".to_string());
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
        // New art means a new patch: drop art for patches that are no longer selectable.
        prune_banners(parent, &links.iter().map(|link| link.0.as_str()).collect::<Vec<_>>());
    }
    let _ = std::fs::write(&path, &bytes);
    Ok(to_url(&bytes))
}

/// Removes cached key art (`<version>.jpg`) for every version not in `keep`; other files are left alone.
fn prune_banners(dir: &std::path::Path, keep: &[&str]) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.flatten().map(|entry| entry.path()) {
        let stale = path.extension().is_some_and(|ext| ext == "jpg")
            && path.file_stem().and_then(|stem| stem.to_str()).is_some_and(|version| !keep.contains(&version));
        if stale {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The newest patch in Riot's notes listing, for the sidebar's NEW badge.
pub async fn newest_patch() -> Result<String, String> {
    Ok(patch_links(false).await?.0[0].0.clone())
}

/// Only current and previous are selectable. Previous articles never expire;
/// current articles and the discovery index refresh after six hours.
pub async fn latest_patch_notes(force: bool, version: Option<String>) -> Result<PatchNotes, String> {
    let (links, index_stale) = patch_links(force).await?;
    let index = selected_patch(&links, version.as_deref())?;
    let (patch, slug, _) = &links[index];
    let mut notes = cached_patch(cache(), patch, index == 0, force, || fetch_patch(patch, slug)).await?;
    notes.available_patches = links.iter().map(|link| PatchChoice { version: link.0.clone(), banner_url: link.2.clone() }).collect();
    notes.stale |= index_stale;
    let items = super::items::names().await;
    attach_item_ids(&mut notes, items.as_ref());
    Ok(notes)
}

fn item_name_key(name: &str) -> String {
    // Spacing is ignored: the notes say "Bastion Breaker", the catalog "Bastionbreaker".
    name.chars().filter(|c| !matches!(c, '\'' | '"' | '‘' | '’' | '“' | '”' | '`' | 'ʼ'))
        .filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase()
}

fn item_id_for(name: &str, items: &std::collections::HashMap<i64, String>) -> Option<i64> {
    let wanted = item_name_key(name);
    if wanted.is_empty() { return None; }
    items.iter().filter(|(id, name)| **id > 0 && item_name_key(name) == wanted).map(|(id, _)| *id).min()
}

fn attach_item_ids(notes: &mut PatchNotes, items: &std::collections::HashMap<i64, String>) {
    for section in &mut notes.sections {
        if section.title.split_whitespace().next().is_some_and(|word| word.eq_ignore_ascii_case("items") || word.eq_ignore_ascii_case("item")) {
            for entry in &mut section.entries { entry.item_id = item_id_for(&entry.name, items); }
        }
    }
}

fn selected_patch(links: &[PatchLink; 2], version: Option<&str>) -> Result<usize, String> {
    match version {
        None => Ok(0),
        Some(version) => links.iter().position(|link| link.0 == version)
            .ok_or_else(|| "Only the current and previous patches are available.".to_string()),
    }
}

async fn cached_patch<F, Fut>(
    cache: &LastGoodCache<PatchNotes>, patch: &str, current: bool, force: bool, fetch: F,
) -> Result<PatchNotes, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<PatchNotes, String>>,
{
    if let Some(hit) = cache.lookup(patch) {
        if !current || (hit.fresh && !force) {
            return Ok(hit.value);
        }
    }
    match fetch().await {
        Ok(notes) => {
            cache.store(patch.to_string(), notes.clone());
            Ok(notes)
        }
        Err(error) => match cache.lookup(patch) {
            Some(hit) => {
                let mut notes = hit.value;
                notes.stale = true;
                notes.fetched_at = Some(hit.fetched_at);
                Ok(notes)
            }
            None => Err(error),
        },
    }
}

async fn patch_links(force: bool) -> Result<([PatchLink; 2], bool), String> {
    cached_links(listing_cache(), force, fetch_patch_links).await
}

async fn cached_links<F, Fut>(cache: &LastGoodCache<[PatchLink; 2]>, force: bool, fetch: F) -> Result<([PatchLink; 2], bool), String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<[PatchLink; 2], String>>,
{
    if !force {
        if let Some(hit) = cache.lookup(CACHE_KEY).filter(|hit| hit.fresh) {
            return Ok((hit.value, false));
        }
    }
    match fetch().await {
        Ok(links) => { cache.store(CACHE_KEY.to_string(), links.clone()); Ok((links, false)) }
        Err(error) => cache.lookup(CACHE_KEY).map(|hit| (hit.value, true)).ok_or(error),
    }
}

async fn fetch_patch_links() -> Result<[PatchLink; 2], String> {
    let client = client()?;
    let listing = get_text(&client, LISTING_URL, MAX_LISTING_BYTES).await?;
    let mut links = latest_patch_links(&listing)
        .ok_or_else(|| "Patch notes returned an unexpected format.".to_string())?;
    let listing_data = next_data(&listing);
    for link in &mut links {
        link.2 = listing_data.as_ref().and_then(|data| listing_banner(data, &link.1));
    }
    Ok(links)
}

/// The key art the news listing shows for the article at `slug`.
fn listing_banner(listing: &serde_json::Value, slug: &str) -> Option<String> {
    fn find<'a>(value: &'a serde_json::Value, slug: &str) -> Option<&'a serde_json::Value> {
        match value {
            serde_json::Value::Object(map) => {
                let url = map.get("action").and_then(|a| a["payload"]["url"].as_str());
                if url.is_some_and(|url| url.trim_end_matches('/').ends_with(slug)) {
                    return Some(value);
                }
                map.values().find_map(|v| find(v, slug))
            }
            serde_json::Value::Array(items) => items.iter().find_map(|v| find(v, slug)),
            _ => None,
        }
    }
    let item = find(listing, slug)?;
    let url = item["media"]["url"].as_str().or_else(|| item["imageMedia"]["url"].as_str())?;
    (url.starts_with("https://") && url.len() <= 512).then(|| url.to_string())
}

async fn fetch_patch(patch: &str, slug: &str) -> Result<PatchNotes, String> {
    let client = client()?;
    let article_url = format!("{ARTICLE_BASE}/{slug}/");
    let article = get_text(&client, &article_url, MAX_ARTICLE_BYTES).await?;
    parse_article(&article, patch, &article_url)
        .ok_or_else(|| "Patch notes returned an unexpected format.".to_string())
}

// ---------------------------------------------------------------------------
// Discovery + article extraction
// ---------------------------------------------------------------------------

/// The first two distinct patch-note links, preserving the listing's order.
pub fn latest_patch_links(listing_html: &str) -> Option<[PatchLink; 2]> {
    const MARKER: &str = "league-of-legends-patch-";
    let mut rest = listing_html;
    let mut links: Vec<PatchLink> = Vec::new();
    while let Some(at) = rest.find(MARKER) {
        let tail = &rest[at + MARKER.len()..];
        let mut parts = tail.split('-');
        let (Some(major), Some(minor)) = (parts.next(), parts.next()) else {
            rest = tail;
            continue;
        };
        let digits = |part: &str| !part.is_empty() && part.len() <= 3 && part.bytes().all(|b| b.is_ascii_digit());
        // The slug continues past "notes" (quote, slash), so only its start is checked.
        let is_notes = parts.next().is_some_and(|part| {
            part == "notes" || part.strip_prefix("notes").is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_alphanumeric()))
        });
        if digits(major) && digits(minor) && is_notes {
            let major: u32 = major.parse().ok()?;
            let minor: u32 = minor.parse().ok()?;
            if (20..=40).contains(&major) && (1..=30).contains(&minor) {
                let patch = format!("{major}.{minor}");
                if !links.iter().any(|link| link.0 == patch) {
                    links.push((patch, format!("league-of-legends-patch-{major}-{minor}-notes"), None));
                    if links.len() == 2 { break; }
                }
            }
        }
        rest = tail;
    }
    links.try_into().ok()
}

/// The JSON inside the page's `__NEXT_DATA__` script tag.
fn next_data(html: &str) -> Option<serde_json::Value> {
    let marker = html.find("__NEXT_DATA__")?;
    let open = html[marker..].find('>')? + marker + 1;
    let close = html[open..].find("</script>")? + open;
    serde_json::from_str(&html[open..close]).ok()
}

pub fn parse_article(html: &str, patch: &str, article_url: &str) -> Option<PatchNotes> {
    let data = next_data(html)?;
    let page = data.get("props")?.get("pageProps")?.get("page")?;
    let blades = page.get("blades")?.as_array()?;

    let mut title = String::new();
    let mut description = String::new();
    let mut date = None;
    let mut banner_url = None;
    let mut body_html = None;
    for blade in blades {
        match blade.get("type").and_then(|t| t.as_str()) {
            Some("articleMasthead") => {
                title = blade.get("title")?.as_str().unwrap_or_default().to_string();
                description = blade
                    .get("description")
                    .and_then(|d| d.get("body"))
                    .and_then(|b| b.as_str())
                    .map(|html| clean_text(&strip_tags(html)))
                    .unwrap_or_default();
                date = blade
                    .get("publishDate")
                    .and_then(|d| d.as_str())
                    .map(str::to_string);
                banner_url = blade
                    .get("banner")
                    .and_then(|b| b.get("url"))
                    .and_then(|u| u.as_str())
                    .filter(|u| u.starts_with("https://"))
                    .map(|u| cap(u, 500));
            }
            Some("patchNotesRichText") => {
                body_html = blade
                    .get("richText")
                    .and_then(|r| r.get("body"))
                    .and_then(|b| b.as_str())
                    .map(str::to_string);
            }
            _ => {}
        }
    }
    let body_html = body_html?;
    if title.is_empty() {
        title = format!("League of Legends Patch {patch} Notes");
    }
    let (intro, sections) = parse_body(&body_html);
    if sections.is_empty() {
        return None;
    }
    Some(PatchNotes {
        patch: patch.to_string(),
        available_patches: Vec::new(),
        title: cap(&title, 200),
        description: cap(&description, 400),
        date,
        banner_url,
        article_url: article_url.to_string(),
        intro,
        sections,
        stale: false,
        fetched_at: Some(now_ms()),
    })
}

// ---------------------------------------------------------------------------
// The rich-text HTML -> sections/entries/groups/lines parser.
// A tiny tag scanner: only block tags (h2/h3/h4/blockquote/p/li) capture text,
// inline tags are dropped, and every string is length-capped.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Capture {
    None,
    H2,
    H3Title,
    H3Other,
    H4,
    H4Skin,
    Context,
    Para,
    Item,
}

fn parse_body(html: &str) -> (Vec<String>, Vec<PatchSection>) {
    let mut intro: Vec<String> = Vec::new();
    let mut sections: Vec<PatchSection> = Vec::new();
    let mut capture = Capture::None;
    let mut text = String::new();

    let on_text = |chunk: &str, capture: Capture, text: &mut String| {
        if capture != Capture::None {
            text.push_str(chunk);
        }
    };

    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let Some(end) = html[i..].find('>') else { break };
            let tag = &html[i + 1..i + end];
            let closing = tag.starts_with('/');
            let raw_name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
            let name = raw_name.to_ascii_lowercase();
            if !closing && name == "br" && capture != Capture::None {
                text.push('\n');
            }
            if closing {
                let done = matches!((capture, name.as_str()),
                    (Capture::H2, "h2") | (Capture::H3Title, "h3") | (Capture::H3Other, "h3")
                    | (Capture::H4, "h4") | (Capture::H4Skin, "h4") | (Capture::Context, "blockquote")
                    | (Capture::Para, "p") | (Capture::Item, "li")
                );
                if done {
                    finish(capture, &text, &mut intro, &mut sections);
                    capture = Capture::None;
                    text.clear();
                }
            } else if capture == Capture::None {
                capture = match name.as_str() {
                    "h2" => Capture::H2,
                    "h3" if tag.contains("change-title") => Capture::H3Title,
                    "h3" => Capture::H3Other,
                    "h4" if tag.contains("skin-title") => Capture::H4Skin,
                    "h4" => Capture::H4,
                    "blockquote" if tag.contains("context") => Capture::Context,
                    "p" => Capture::Para,
                    "li" => Capture::Item,
                    _ => Capture::None,
                };
                text.clear();
            }
            i += end + 1;
        } else {
            let Some(next) = html[i..].find('<') else {
                on_text(&html[i..], capture, &mut text);
                break;
            };
            on_text(&html[i..i + next], capture, &mut text);
            i += next;
        }
    }
    (intro, sections)
}

fn finish(
    capture: Capture,
    raw: &str,
    intro: &mut Vec<String>,
    sections: &mut Vec<PatchSection>,
) {
    let text = clean_text(raw);
    match capture {
        Capture::H2 => {
            if !text.is_empty() && sections.len() < MAX_SECTIONS {
                sections.push(PatchSection { title: cap(&text, 120), entries: Vec::new(), paragraphs: Vec::new() });
            }
        }
        Capture::H3Title => {
            if text.is_empty() {
                return;
            }
            if let Some(section) = sections.last_mut() {
                if section.entries.len() < MAX_ENTRIES {
                    section.entries.push(PatchEntry {
                        item_id: None,
                        name: cap(&text, 120),
                        context: None,
                        groups: Vec::new(),
                        notes: Vec::new(),
                    });
                }
            }
        }
        Capture::H3Other | Capture::H4 => {
            if text.is_empty() {
                return;
            }
            push_group(sections, Some(cap(&text, 160)));
        }
        Capture::H4Skin => {
            if !text.is_empty() {
                if let Some(section) = sections.last_mut() {
                    if section.paragraphs.len() < MAX_PARAGRAPHS {
                        section.paragraphs.push(cap(&text, 200));
                    }
                }
            }
        }
        Capture::Context => {
            for para in split_paragraphs(raw) {
                if sections.is_empty() {
                    if intro.len() < 6 {
                        intro.push(para);
                    }
                } else if let Some(entry) = current_entry(sections) {
                    if entry.context.is_none() {
                        entry.context = Some(para);
                    } else if entry.notes.len() < MAX_PARAGRAPHS {
                        entry.notes.push(para);
                    }
                } else if let Some(section) = sections.last_mut() {
                    if section.paragraphs.len() < MAX_PARAGRAPHS {
                        section.paragraphs.push(para);
                    }
                }
            }
        }
        Capture::Para => {
            if text.is_empty() {
                return;
            }
            if sections.is_empty() {
                return; // The byline/designer credits before the first heading.
            }
            if let Some(entry) = current_entry(sections) {
                if entry.notes.len() < MAX_PARAGRAPHS {
                    entry.notes.push(cap(&text, 1200));
                }
            } else if let Some(section) = sections.last_mut() {
                if section.paragraphs.len() < MAX_PARAGRAPHS {
                    section.paragraphs.push(cap(&text, 1200));
                }
            }
        }
        Capture::Item => {
            if text.is_empty() {
                return;
            }
            let Some(section) = sections.last_mut() else { return };
            if section.entries.is_empty() {
                section.entries.push(PatchEntry { name: String::new(), item_id: None, context: None, groups: Vec::new(), notes: Vec::new() });
            }
            let Some(entry) = section.entries.last_mut() else { return };
            if entry.groups.is_empty() {
                entry.groups.push(PatchGroup { title: None, lines: Vec::new() });
            }
            if let Some(group) = entry.groups.last_mut() {
                if group.lines.len() < MAX_LINES {
                    group.lines.push(parse_line(&text));
                }
            }
        }
        Capture::None => {}
    }
}

fn current_entry(sections: &mut [PatchSection]) -> Option<&mut PatchEntry> {
    sections.last_mut()?.entries.last_mut()
}

fn push_group(sections: &mut [PatchSection], title: Option<String>) {
    let Some(section) = sections.last_mut() else { return };
    if section.entries.is_empty() {
        if section.entries.len() >= MAX_ENTRIES {
            return;
        }
        section.entries.push(PatchEntry { name: String::new(), item_id: None, context: None, groups: Vec::new(), notes: Vec::new() });
    }
    if let Some(entry) = section.entries.last_mut() {
        if entry.groups.len() < MAX_GROUPS {
            entry.groups.push(PatchGroup { title, lines: Vec::new() });
        }
    }
}

fn parse_line(text: &str) -> PatchLine {
    let text = cap(text, 600);
    if let Some((left, right)) = text.split_once('⇒') {
        let (label, before) = split_label(left.trim());
        return PatchLine {
            label,
            before: Some(cap(before, 300)),
            after: Some(cap(right.trim(), 300)),
            text: None,
        };
    }
    let (label, rest) = split_label(&text);
    match label {
        Some(label) if !rest.is_empty() => PatchLine {
            label: Some(label),
            before: None,
            after: Some(cap(rest, 300)),
            text: None,
        },
        _ => PatchLine { label: None, before: None, after: None, text: Some(text) },
    }
}

/// "Mana Growth: 40/Level" -> (Some("Mana Growth"), "40/Level"). A label is
/// only split off when it is short and the remainder is non-empty.
fn split_label(text: &str) -> (Option<String>, &str) {
    if let Some((head, tail)) = text.split_once(':') {
        let head = head.trim();
        if !head.is_empty() && head.chars().count() <= 48 && !tail.trim().is_empty() {
            return (Some(head.to_string()), tail.trim());
        }
    }
    (None, text)
}

// ---------------------------------------------------------------------------
// Text cleanup
// ---------------------------------------------------------------------------

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Splits a captured block on line breaks into clean paragraphs.
fn split_paragraphs(raw: &str) -> Vec<String> {
    decode_entities(raw)
        .split('\n')
        .map(clean_text)
        .filter(|p| !p.is_empty())
        .map(|p| cap(&p, 1200))
        .collect()
}

fn clean_text(raw: &str) -> String {
    let decoded = decode_entities(raw);
    let mut out = String::with_capacity(decoded.len());
    let mut last_space = true; // trims the left edge too
    for ch in decoded.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim_end().to_string()
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let entity = &tail[1..end];
        let decoded: Option<String> = match entity {
            "amp" => Some("&".into()),
            "lt" => Some("<".into()),
            "gt" => Some(">".into()),
            "quot" => Some("\"".into()),
            "apos" | "#39" | "#x27" => Some("'".into()),
            "nbsp" => Some(" ".into()),
            "rsquo" | "#8217" => Some("\u{2019}".into()),
            "ldquo" | "#8220" => Some("\u{201c}".into()),
            "rdquo" | "#8221" => Some("\u{201d}".into()),
            "mdash" | "#8212" => Some("\u{2014}".into()),
            "hellip" | "#8230" => Some("\u{2026}".into()),
            _ => entity
                .strip_prefix('#')
                .and_then(|n| {
                    n.strip_prefix('x')
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                        .or_else(|| n.parse::<u32>().ok())
                })
                .and_then(char::from_u32)
                .map(|c| c.to_string()),
        };
        match decoded {
            Some(value) => {
                out.push_str(&value);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn cap(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pruning_keeps_selectable_patch_art_and_unrelated_files() {
        let dir = std::env::temp_dir().join(format!("swapper-banner-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["26.19.jpg", "26.20.jpg", "26.21.jpg", "notes.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        prune_banners(&dir, &["26.21", "26.20"]);
        let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        left.sort();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(left, ["26.20.jpg", "26.21.jpg", "notes.txt"]);
    }

    #[test]
    fn item_names_match_case_quotes_apostrophes_and_whitespace() {
        let items = std::collections::HashMap::from([
            (3089, "Rabadon's Deathcap".into()), (3053, "Sterak’s Gage".into()),
            (0, "Unknown".into()), (9999, "Rabadon's Deathcap".into()), (2520, "Bastionbreaker".into()),
        ]);
        assert_eq!(item_id_for("  “RABADON’S”\t  DEATHCAP  ", &items), Some(3089));
        assert_eq!(item_id_for("Sterak's\u{a0}Gage", &items), Some(3053));
        assert_eq!(item_id_for("Steraks Gage", &items), Some(3053));
        assert_eq!(item_id_for("Bastion Breaker", &items), Some(2520));
        for name in ["Brand New Item", "Unknown", "", "Rabadons Deathcap Extra"] {
            assert_eq!(item_id_for(name, &items), None);
        }
    }

    #[test]
    fn item_enrichment_is_optional_response_metadata_and_old_caches_still_load() {
        let mut notes = fixture_notes("26.21");
        notes.sections.push(PatchSection { title: "Items".into(), paragraphs: vec![], entries: vec![
            PatchEntry { name: "Dawncore".into(), item_id: None, context: None, groups: vec![], notes: vec![] },
            PatchEntry { name: "New Item".into(), item_id: None, context: None, groups: vec![], notes: vec![] },
        ] });
        let mut old_cache = serde_json::to_value(&notes).unwrap();
        old_cache["sections"][0]["entries"][0].as_object_mut().unwrap().remove("itemId");
        let _: PatchNotes = serde_json::from_value(old_cache).unwrap();
        attach_item_ids(&mut notes, &std::collections::HashMap::from([(6621, "Dawncore".into()), (1, "Azir".into())]));
        assert_eq!(notes.sections[0].entries[0].item_id, None);
        assert_eq!(notes.sections.last().unwrap().entries[0].item_id, Some(6621));
        assert_eq!(notes.sections.last().unwrap().entries[1].item_id, None);
    }

    const FIXTURE_LISTING: &str = r#"<html><body>
        <a href="/en-us/news/game-updates/league-of-legends-patch-9-9-notes">old</a>
        <a href="/en-us/news/game-updates/league-of-legends-patch-26-21-notes">Patch 26.21</a>
        <a href="/en-us/news/game-updates/league-of-legends-patch-26-20-notes">Patch 26.20</a>
    </body></html>"#;

    fn fixture_article() -> String {
        let body = r#"<div id="patch-notes-container"><blockquote class="blockquote context"><p>Hello patch people.<br><br>Second intro paragraph with &amp; entity.</p></blockquote><header class="header-primary"><h2 id="patch-champions">Champions</h2></header><div class="content-border"><div class="patch-change-block white-stone accent-before"><div><p><a class="reference-link" href="https://leagueoflegends.com/en-us/champions/azir/"><img src="x.png"></a></p><h3 class="change-title" id="patch-azir"><a href="https://www.leagueoflegends.com/en-us/champions/azir/">Azir</a></h3><blockquote class="blockquote context"><p>We are buffing Azir&#39;s mana.</p></blockquote><hr class="divider"><h4 class="change-detail-title">Base Stats</h4><ul><li><strong>Mana Growth</strong>: 40/Level &#8658; <strong>50/Level</strong></li><li><strong>New</strong>: Now gains a shield.</li></ul></div></div></div><header class="header-primary"><h2 id="patch-bugfixes">Bugfixes &amp; QoL Changes</h2></header><div class="content-border"><div class="white-stone accent-before"><div><ul><li>Fixed a bug where a tooltip lied.</li></ul></div></div></div></div>"#;
        let next = serde_json::json!({"props":{"pageProps":{"page":{"blades":[
            {"type":"riotbar"},
            {"type":"articleMasthead","title":"League of Legends Patch 26.21 Notes",
             "description":{"type":"html","body":"Chunky patch."},
             "publishDate":"2026-10-20T18:00:00.000Z",
             "banner":{"type":"image","url":"https://example.com/banner.jpg"}},
            {"type":"patchNotesRichText","fragmentId":"26.21","richText":{"type":"html","body":body}}
        ]}}}});
        format!(
            "<html><script id=\"__NEXT_DATA__\" type=\"application/json\">{}</script></html>",
            next
        )
    }

    #[test]
    fn the_two_newest_distinct_patch_links_win_and_garbage_is_skipped() {
        let listing = FIXTURE_LISTING.replace(
            "<a href=\"/en-us/news/game-updates/league-of-legends-patch-26-20-notes\">",
            "<a href=\"/en-us/news/game-updates/league-of-legends-patch-26-21-notes\">duplicate</a><a href=\"/en-us/news/game-updates/league-of-legends-patch-26-99-notes\">bad</a><a href=\"/en-us/news/game-updates/league-of-legends-patch-26-20-notes\">",
        );
        assert_eq!(latest_patch_links(&listing), Some([
            ("26.21".into(), "league-of-legends-patch-26-21-notes".into(), None),
            ("26.20".into(), "league-of-legends-patch-26-20-notes".into(), None),
        ]));
        assert_eq!(latest_patch_links("<p>no links</p>"), None);
        assert_eq!(latest_patch_links("league-of-legends-patch-26-21-notes league-of-legends-patch-26-21-notes"), None);
        assert_eq!(latest_patch_links("league-of-legends-patch-27-1-notes/ league-of-legends-patch-26-24-notes/ league-of-legends-patch-26-23-notes/"), Some([
            ("27.1".into(), "league-of-legends-patch-27-1-notes".into(), None),
            ("26.24".into(), "league-of-legends-patch-26-24-notes".into(), None),
        ]));
    }

    #[test]
    fn listing_key_art_is_found_per_article_and_only_https() {
        let listing = serde_json::json!({"props": {"pageProps": {"page": {"blades": [{"items": [
            {"title": "26.21", "action": {"payload": {"url": "/en-us/news/game-updates/league-of-legends-patch-26-21-notes"}},
             "media": {"url": "https://cmsassets.example/2621.jpg"}},
            {"title": "26.20", "action": {"payload": {"url": "/en-us/news/game-updates/league-of-legends-patch-26-20-notes/"}},
             "imageMedia": {"url": "https://cmsassets.example/2620.jpg"}},
            {"title": "bad", "action": {"payload": {"url": "/en-us/news/game-updates/league-of-legends-patch-26-19-notes"}},
             "media": {"url": "javascript:alert(1)"}}
        ]}]}}}});
        assert_eq!(listing_banner(&listing, "league-of-legends-patch-26-21-notes").as_deref(), Some("https://cmsassets.example/2621.jpg"));
        assert_eq!(listing_banner(&listing, "league-of-legends-patch-26-20-notes").as_deref(), Some("https://cmsassets.example/2620.jpg"));
        assert_eq!(listing_banner(&listing, "league-of-legends-patch-26-19-notes"), None);
        assert_eq!(listing_banner(&listing, "league-of-legends-patch-26-18-notes"), None);
    }

    #[test]
    fn banner_cache_paths_and_bytes_are_validated() {
        assert!(banner_path("26.21").is_some_and(|p| p.ends_with("patch-banners/26.21.jpg")));
        assert!(banner_path("../26.21").is_none());
        assert!(banner_path("").is_none());
        assert!(is_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0, 0]));
        assert!(!is_jpeg(b"<html>"));
        assert!(!is_jpeg(&vec![0xFF; MAX_BANNER_BYTES + 1]));
    }

    fn fixture_notes(patch: &str) -> PatchNotes {
        parse_article(&fixture_article(), patch, "https://example.com/patch/").unwrap()
    }

    #[test]
    fn only_current_and_previous_versions_can_be_selected() {
        let links = latest_patch_links(FIXTURE_LISTING).unwrap();
        assert_eq!(selected_patch(&links, None), Ok(0));
        assert_eq!(selected_patch(&links, Some("26.21")), Ok(0));
        assert_eq!(selected_patch(&links, Some("26.20")), Ok(1));
        assert!(selected_patch(&links, Some("26.19")).is_err());
        assert!(selected_patch(&links, Some("../26.21")).is_err());
    }

    #[tokio::test]
    async fn listing_cache_refreshes_and_falls_back_to_its_last_good_pair() {
        let cache = LastGoodCache::open(None, CACHE_TTL, 1, 4096);
        let links = latest_patch_links(FIXTURE_LISTING).unwrap();
        cache.store_at(CACHE_KEY.into(), links.clone(), now_ms());
        let (fresh, stale) = cached_links(&cache, false, || async { panic!("fresh listing must not refetch") }).await.unwrap();
        assert_eq!(fresh, links);
        assert!(!stale);
        let (fallback, stale) = cached_links(&cache, true, || async { Err("Offline".into()) }).await.unwrap();
        assert_eq!(fallback, links);
        assert!(stale);
        let rolled = [("26.22".into(), "new-slug".into(), None), links[0].clone()];
        let (refreshed, stale) = cached_links(&cache, true, || async { Ok(rolled.clone()) }).await.unwrap();
        assert_eq!(refreshed, rolled);
        assert!(!stale);
        assert_eq!(cache.lookup(CACHE_KEY).unwrap().value, rolled);
    }

    #[tokio::test]
    async fn current_expires_but_previous_is_reused_even_when_forced() {
        let cache = LastGoodCache::open(None, CACHE_TTL, 3, 2 * 1024 * 1024);
        let old = now_ms() - CACHE_TTL.as_millis() as i64 - 1;
        cache.store_at("26.21".into(), fixture_notes("26.21"), old);
        cache.store_at("26.20".into(), fixture_notes("26.20"), old);
        let previous = cached_patch(&cache, "26.20", false, true, || async {
            panic!("a previous patch must not be downloaded again")
        }).await.unwrap();
        assert_eq!(previous.patch, "26.20");
        assert!(!previous.stale);
        let current = cached_patch(&cache, "26.21", true, false, || async {
            let mut fresh = fixture_notes("26.21");
            fresh.description = "Refreshed current".into();
            Ok(fresh)
        }).await.unwrap();
        assert_eq!(current.description, "Refreshed current");
        assert_eq!(cache.lookup("26.20").unwrap().value.description, "Chunky patch.");
        let fresh = cached_patch(&cache, "26.21", true, false, || async {
            panic!("a fresh current patch must not be downloaded again")
        }).await.unwrap();
        assert_eq!(fresh.description, "Refreshed current");
        let forced = cached_patch(&cache, "26.21", true, true, || async {
            let mut notes = fixture_notes("26.21");
            notes.description = "Forced current refresh".into();
            Ok(notes)
        }).await.unwrap();
        assert_eq!(forced.description, "Forced current refresh");
    }

    #[tokio::test]
    async fn failed_current_refresh_returns_only_its_own_stale_copy() {
        let cache = LastGoodCache::open(None, CACHE_TTL, 3, 2 * 1024 * 1024);
        cache.store_at("26.21".into(), fixture_notes("26.21"), 123);
        cache.store_at("26.20".into(), fixture_notes("26.20"), 456);
        let stale = cached_patch(&cache, "26.21", true, true, || async { Err("Offline".into()) }).await.unwrap();
        assert_eq!(stale.patch, "26.21");
        assert!(stale.stale);
        assert_eq!(stale.fetched_at, Some(123));
        assert!(!cache.lookup("26.21").unwrap().value.stale, "do not persist a failed refresh");
        assert!(cached_patch(&cache, "26.19", false, false, || async { Err("Offline".into()) }).await.is_err(), "never substitute another version");
    }

    #[tokio::test]
    async fn per_version_cache_survives_reopening_and_patch_rollover() {
        let path = std::env::temp_dir().join(format!("swapper-patch-cache-{}-{}.json", std::process::id(), now_ms()));
        let old = now_ms() - CACHE_TTL.as_millis() as i64 - 1;
        {
            let cache = LastGoodCache::open(Some(path.clone()), CACHE_TTL, 3, 2 * 1024 * 1024);
            cache.store_at("26.21".into(), fixture_notes("26.21"), old);
            cache.store_at("26.20".into(), fixture_notes("26.20"), old + 1);
        }
        let cache = LastGoodCache::open(Some(path.clone()), CACHE_TTL, 3, 2 * 1024 * 1024);
        let previous = cached_patch(&cache, "26.20", false, false, || async {
            panic!("the persisted previous patch does not expire")
        }).await.unwrap();
        assert_eq!(previous.patch, "26.20");
        // A new current must not evict the cached current just as it becomes previous.
        cache.store_at("26.22".into(), fixture_notes("26.22"), now_ms());
        let previous = cached_patch(&cache, "26.21", false, false, || async {
            panic!("rollover must reuse the old current as previous")
        }).await.unwrap();
        assert_eq!(previous.patch, "26.21");
        assert_eq!(cache.lookup("26.22").unwrap().value.patch, "26.22");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn parses_masthead_intro_champions_and_bugfixes() {
        let notes = parse_article(&fixture_article(), "26.21", "https://example.com/a/")
            .expect("fixture parses");
        assert_eq!(notes.patch, "26.21");
        assert_eq!(notes.title, "League of Legends Patch 26.21 Notes");
        assert_eq!(notes.description, "Chunky patch.");
        assert_eq!(notes.banner_url.as_deref(), Some("https://example.com/banner.jpg"));
        assert_eq!(notes.intro.len(), 2);
        assert!(notes.intro[1].contains("& entity"));

        assert_eq!(notes.sections.len(), 2);
        let champions = &notes.sections[0];
        assert_eq!(champions.title, "Champions");
        assert_eq!(champions.entries.len(), 1);
        let azir = &champions.entries[0];
        assert_eq!(azir.name, "Azir");
        assert_eq!(azir.context.as_deref(), Some("We are buffing Azir's mana."));
        assert_eq!(azir.groups.len(), 1);
        assert_eq!(azir.groups[0].title.as_deref(), Some("Base Stats"));
        let line = &azir.groups[0].lines[0];
        assert_eq!(line.label.as_deref(), Some("Mana Growth"));
        assert_eq!(line.before.as_deref(), Some("40/Level"));
        assert_eq!(line.after.as_deref(), Some("50/Level"));

        let bugs = &notes.sections[1];
        assert_eq!(bugs.title, "Bugfixes & QoL Changes");
        assert_eq!(bugs.entries[0].groups[0].lines[0].text.as_deref(), Some("Fixed a bug where a tooltip lied."));
    }

    #[test]
    fn entities_and_labels_are_bounded() {
        assert_eq!(clean_text("  a   b &amp; c&#8217;s "), "a b & c\u{2019}s");
        let line = parse_line("Damage: 10 ⇒ 20");
        assert_eq!(line.label.as_deref(), Some("Damage"));
        let long = parse_line(&"x".repeat(5000));
        assert!(long.text.unwrap().chars().count() <= 600);
    }

    #[test]
    fn malformed_pages_yield_none_not_a_panic() {
        assert!(parse_article("<html>nothing</html>", "26.21", "https://x/").is_none());
        assert!(parse_article(r#"{"props":{}}"#, "26.21", "https://x/").is_none());
    }

    #[tokio::test]
    #[ignore = "hits the live Riot site; run manually"]
    async fn live_latest_patch_notes_parse() {
        let notes = latest_patch_notes(true, None).await.expect("live fetch");
        assert!(!notes.sections.is_empty());
        assert!(!notes.intro.is_empty());
    }
}
