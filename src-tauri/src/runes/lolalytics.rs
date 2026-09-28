//! lolalytics champion build pages, filtered by keystone.
//!
//! For a champion, lane, rank bracket and keystone, lolalytics renders the
//! items players build together with that keystone. The page is server-side
//! rendered and needs no auth, cookies or special headers, but it is HTML
//! (~0.5 MB) rather than JSON, so it is parsed defensively and cached.
//!
//! The Qwik page embeds its serialized state in a `<script type="qwik/json">`
//! block that holds both the "Highest Win Build" and "Most Common Build"
//! summaries. That structured copy is preferred: it yields the common core,
//! later-slot item marginals, and starting set. Since later slots are marginal
//! counts rather than a joint six-item sequence, the displayed path is
//! assembled from the strongest unused choice in each slot. When the payload
//! is missing or changed, the
//! visible markup (the `Core Build` / `Item 4` / `Item 5` / `Item 6` anchors
//! with `cdn5.lolalytics.com/item64/{id}.webp` icons) is parsed instead.
//!
//! lolalytics is third-party aggregate data and is not affiliated with Swapper.
//! The endpoint is undocumented and can change.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::provider::ProviderError;

/// The build page. `lane` and `tier` are slugs; `keystone` is a Riot perk id.
const BASE_URL: &str = "https://lolalytics.com/lol";
/// A normal browser user agent; lolalytics does not require one, but it is
/// polite and matches what a person's browser sends.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
/// A build page is a few hundred KB; allow a little more than the tiny op.gg
/// and u.gg calls.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
/// A parsed build is reused for this long, matching lolalytics' own cache.
pub const SUCCESS_TTL: Duration = Duration::from_secs(30 * 60);
/// A failed or empty lookup is remembered this long before retrying.
pub const FAILURE_TTL: Duration = Duration::from_secs(5 * 60);
/// Default rank bracket, the same slug lolalytics uses when none is sent.
pub const DEFAULT_TIER: &str = "emerald_plus";

/// One item stack in the starting set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub id: i64,
    pub count: u32,
}

/// A later-slot candidate and the evidence LoLalytics reports for that slot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemCandidate {
    pub id: i64,
    pub slot: u8,
    pub games: u64,
    pub win_pct: Option<f64>,
}

/// One keystone's common item path and its observed alternatives.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeystoneBuild {
    /// Starting item set, preserving stacked consumable counts where reported.
    pub starters: Vec<ItemStack>,
    /// The core plus the most-supported unused choice in later slots.
    pub items: Vec<i64>,
    /// Unselected candidates, deduplicated and sorted by slot support.
    pub options: Vec<ItemCandidate>,
    /// Sample size reported for the starting item set.
    pub games: u64,
    /// Sample size for the core combination.
    pub core_games: u64,
    /// Win rate for the core combination.
    pub core_win_pct: Option<f64>,
}

/// The champion slug lolalytics uses: lower-case, no spaces or punctuation.
///
/// Most names slugify directly (`Kai'Sa` -> `kaisa`, `Dr. Mundo` -> `drmundo`).
/// A few use a shorter site slug, so they are overridden explicitly.
pub fn champion_slug(name: &str) -> Option<String> {
    let mut slug = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        }
    }
    if slug.is_empty() {
        return None;
    }
    let slug = match slug.as_str() {
        // Nunu & Willump and Renata Glasc use shorter slugs on the site.
        "nunuwillump" => "nunu",
        "renataglasc" => "renata",
        // Defensive: the client names the champion Wukong, but its alias and
        // some game-data files call it MonkeyKing.
        "monkeyking" => "wukong",
        other => other,
    };
    Some(slug.to_string())
}

/// Maps one of Swapper's role slugs to a lolalytics lane slug. Modes without a
/// lane (`none`) return `None`, which hides the build row.
pub fn lane(position: &str) -> Option<&'static str> {
    match position.trim().to_ascii_lowercase().as_str() {
        "top" => Some("top"),
        "jungle" => Some("jungle"),
        "mid" | "middle" => Some("middle"),
        "adc" | "bottom" => Some("bottom"),
        "support" | "utility" => Some("support"),
        _ => None,
    }
}

/// The rank brackets lolalytics accepts. Swapper's own brackets are already
/// these slugs, so an unknown value falls back to the default.
pub fn tier_slug(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "all" => "all",
        "gold_plus" => "gold_plus",
        "platinum_plus" => "platinum_plus",
        "emerald_plus" => "emerald_plus",
        "diamond_plus" => "diamond_plus",
        "master_plus" => "master_plus",
        "challenger" => "challenger",
        _ => DEFAULT_TIER,
    }
}

/// The build page URL for a keystone.
pub fn url(champion_slug: &str, lane: &str, tier: &str, keystone: i64) -> String {
    format!(
        "{BASE_URL}/{champion_slug}/build/?lane={lane}&tier={}&keystone={keystone}",
        tier_slug(tier)
    )
}

/// Cache key for one champion, lane, bracket and keystone.
pub fn cache_key(champion_slug: &str, lane: &str, tier: &str, keystone: i64) -> String {
    format!("{champion_slug}|{lane}|{}|{keystone}", tier_slug(tier))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the common item path and slot alternatives from a lolalytics page.
/// A page without the expected data yields `None` rather than an error, so one
/// card is simply left without a build.
pub fn parse_build(html: &str) -> Option<KeystoneBuild> {
    parse_qwik(html).or_else(|| parse_anchors(html))
}

/// The compacted state inside `<script type="qwik/json">`.
#[derive(Deserialize)]
struct Qwik {
    #[serde(default)]
    objs: Vec<Value>,
}

/// Parses the structured Qwik state, preferring the "Most Common Build".
fn parse_qwik(html: &str) -> Option<KeystoneBuild> {
    let payload = qwik_json(html)?;
    let state: Qwik = serde_json::from_str(payload).ok()?;
    let objs = &state.objs;
    // The page summary is an object with exactly `pick` and `win` keys whose
    // values are the two build summaries.
    for entry in objs {
        let Some(map) = entry.as_object() else {
            continue;
        };
        if map.len() > 3 || !map.contains_key("pick") || !map.contains_key("win") {
            continue;
        }
        let pick = resolve(map.get("pick")?, objs, 0);
        if let Some(build) = build_from_summary(&pick) {
            return Some(build);
        }
    }
    None
}

/// The JSON string inside the page's Qwik state script.
fn qwik_json(html: &str) -> Option<&str> {
    let marker = "qwik/json";
    let at = html.find(marker)?;
    let start = html[at..].find('{')? + at;
    let end = html[start..].find("</script>")? + start;
    Some(&html[start..end])
}

/// Resolves one Qwik reference value. Objects and arrays hold references to
/// other `objs` entries as base-36 indices; leaf strings and numbers are used
/// as-is.
fn resolve(value: &Value, objs: &[Value], depth: u8) -> Value {
    if depth > 40 {
        return Value::Null;
    }
    match value {
        Value::String(text) => match base36_index(text) {
            Some(index) if index < objs.len() => resolve_entry(&objs[index], objs, depth + 1),
            _ => value.clone(),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve(item, objs, depth + 1))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, item) in map {
                out.insert(key.clone(), resolve(item, objs, depth + 1));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Resolves an `objs` entry: a leaf string is a literal, while an object or
/// array has its values resolved.
fn resolve_entry(entry: &Value, objs: &[Value], depth: u8) -> Value {
    match entry {
        Value::String(_) => entry.clone(),
        other => resolve(other, objs, depth),
    }
}

/// A base-36 index when the string is a plain lower-case base-36 number.
fn base36_index(text: &str) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    if !text
        .bytes()
        .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
    {
        return None;
    }
    usize::from_str_radix(text, 36).ok()
}

/// Turns a resolved build summary into a [`KeystoneBuild`]. The core is the
/// source's common combination; later slots are selected by support while
/// avoiding duplicate items. The remaining per-slot candidates are retained
/// separately so the importer can put them in an Options block.
fn build_from_summary(summary: &Value) -> Option<KeystoneBuild> {
    let items = summary.get("items")?;
    let core = items.get("core")?.get("set")?.as_array()?;
    let mut ids: Vec<i64> = Vec::new();
    for id in core.iter().filter_map(as_i64) {
        if id > 0 && !ids.contains(&id) {
            ids.push(id);
            if ids.len() == 3 {
                break;
            }
        }
    }
    if ids.len() < 3 {
        return None;
    }

    let slot_candidates = [
        item_candidates(items, "item4", 4),
        item_candidates(items, "item5", 5),
        item_candidates(items, "item6", 6),
    ];
    for candidates in &slot_candidates {
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| !ids.contains(&candidate.id))
        {
            ids.push(candidate.id);
        }
    }

    let mut options: Vec<ItemCandidate> = Vec::new();
    for candidate in slot_candidates.into_iter().flatten() {
        if ids.contains(&candidate.id) {
            continue;
        }
        if let Some(existing) = options.iter_mut().find(|item| item.id == candidate.id) {
            if candidate.games > existing.games {
                *existing = candidate;
            }
        } else {
            options.push(candidate);
        }
    }
    options.sort_by(|left, right| {
        right
            .games
            .cmp(&left.games)
            .then_with(|| left.slot.cmp(&right.slot))
            .then_with(|| {
                right
                    .win_pct
                    .unwrap_or_default()
                    .total_cmp(&left.win_pct.unwrap_or_default())
            })
    });

    let start = items.get("start");
    let starters = start.map(parse_starters).unwrap_or_default();
    let games = items
        .get("start")
        .and_then(|start| start.get("n"))
        .and_then(as_u64)
        .unwrap_or(0);
    let core_games = items
        .get("core")
        .and_then(|core| core.get("n"))
        .and_then(as_u64)
        .unwrap_or(0);
    let core_win_pct = items
        .get("core")
        .and_then(|core| core.get("wr"))
        .and_then(Value::as_f64);
    Some(KeystoneBuild {
        starters,
        items: ids,
        options,
        games,
        core_games,
        core_win_pct,
    })
}

fn parse_starters(start: &Value) -> Vec<ItemStack> {
    let ids = start
        .get("setUnique")
        .and_then(Value::as_array)
        .or_else(|| start.get("set").and_then(Value::as_array));
    let Some(ids) = ids else {
        return Vec::new();
    };
    let counts = start.get("count").and_then(Value::as_array);
    ids.iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let id = as_i64(value)?;
            if id <= 0 {
                return None;
            }
            let count = counts
                .and_then(|counts| counts.get(index))
                .and_then(as_u64)
                .unwrap_or(1)
                .clamp(1, u32::MAX as u64) as u32;
            Some(ItemStack { id, count })
        })
        .collect()
}

fn item_candidates(items: &Value, key: &str, slot: u8) -> Vec<ItemCandidate> {
    let Some(candidates) = items.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut result: Vec<ItemCandidate> = candidates
        .iter()
        .filter_map(|candidate| {
            let id = candidate.get("id").and_then(as_i64)?;
            if id <= 0 {
                return None;
            }
            Some(ItemCandidate {
                id,
                slot,
                games: candidate.get("n").and_then(as_u64).unwrap_or(0),
                win_pct: candidate.get("wr").and_then(Value::as_f64),
            })
        })
        .collect();
    result.sort_by(|left, right| {
        right.games.cmp(&left.games).then_with(|| {
            right
                .win_pct
                .unwrap_or_default()
                .total_cmp(&left.win_pct.unwrap_or_default())
        })
    });
    result
}

fn as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|number| number as i64))
}

fn as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_f64().map(|number| number.max(0.0) as u64))
}

/// Fallback parser for the visible markup, used when the Qwik state is absent
/// or unrecognised. It can recover the starting items and common item path, but
/// the markup does not provide reliable per-candidate sample details.
fn parse_anchors(html: &str) -> Option<KeystoneBuild> {
    let start_at = html.find("Starting Items");
    let core_at = html.find("Core Build")?;
    let item4_at = core_at + html[core_at..].find("Item 4")?;
    let item5_at = item4_at + html[item4_at..].find("Item 5")?;
    let item6_at = item5_at + html[item5_at..].find("Item 6")?;
    let mut items: Vec<i64> = Vec::new();
    for id in item_ids(&html[core_at..item4_at]) {
        if !items.contains(&id) {
            items.push(id);
            if items.len() == 3 {
                break;
            }
        }
    }
    if items.is_empty() {
        return None;
    }
    let boundary = (item6_at + 6000).min(html.len());
    for (from, to) in [
        (item4_at, item5_at),
        (item5_at, item6_at),
        (item6_at, boundary),
    ] {
        if let Some(id) = item_ids(&html[from..to])
            .into_iter()
            .find(|id| !items.contains(id))
        {
            items.push(id);
        }
    }
    let start_fragment = start_at
        .filter(|start| *start < core_at)
        .map(|start| &html[start..core_at]);
    let games = start_fragment.map(games_between).unwrap_or(0);
    let starters = start_fragment
        .map(item_ids)
        .unwrap_or_default()
        .into_iter()
        .map(|id| ItemStack { id, count: 1 })
        .collect();
    Some(KeystoneBuild {
        starters,
        items,
        options: Vec::new(),
        games,
        core_games: 0,
        core_win_pct: None,
    })
}

/// The item ids in a markup fragment, in order, collapsing the adjacent pair
/// each icon produces (`srcset` plus `src`).
fn item_ids(fragment: &str) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    let mut rest = fragment;
    while let Some(at) = rest.find("item64/") {
        rest = &rest[at + "item64/".len()..];
        let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
        if digits.is_empty() {
            continue;
        }
        if let Ok(id) = digits.parse::<i64>() {
            if ids.last() != Some(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

/// The game count from the starting-items row, e.g. `70,627 Games`.
fn games_between(fragment: &str) -> u64 {
    let text = strip_tags(fragment);
    let Some(at) = text.find("Games") else {
        return 0;
    };
    let before = &text[..at];
    let digits: String = before
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_digit() || *ch == ',')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.replace(',', "").parse().unwrap_or(0)
}

/// Drops HTML tags and comments so text patterns can be matched directly.
fn strip_tags(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut rest = fragment;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        match rest[at..].find('>') {
            Some(end) => rest = &rest[at + end + 1..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Cache and client
// ---------------------------------------------------------------------------

/// A cached lookup result.
pub enum Cached {
    Fresh(KeystoneBuild),
    Unavailable,
    Miss,
}

/// Successes and failures for keystone builds, keyed by champion/lane/tier/
/// keystone.
#[derive(Default)]
pub struct BuildCache {
    success: std::collections::HashMap<String, (Instant, KeystoneBuild)>,
    failures: std::collections::HashMap<String, Instant>,
}

impl BuildCache {
    pub fn lookup(&self, key: &str) -> Cached {
        self.lookup_at(key, Instant::now())
    }

    pub fn lookup_at(&self, key: &str, now: Instant) -> Cached {
        if let Some((at, build)) = self.success.get(key) {
            if now.saturating_duration_since(*at) < SUCCESS_TTL {
                return Cached::Fresh(build.clone());
            }
        }
        if let Some(at) = self.failures.get(key) {
            if now.saturating_duration_since(*at) < FAILURE_TTL {
                return Cached::Unavailable;
            }
        }
        Cached::Miss
    }

    pub fn store(&mut self, key: String, build: KeystoneBuild) {
        self.failures.remove(&key);
        self.success.insert(key, (Instant::now(), build));
    }

    pub fn fail(&mut self, key: String) {
        self.failures.insert(key, Instant::now());
    }
}

pub struct LolalyticsClient {
    client: reqwest::Client,
}

impl LolalyticsClient {
    pub fn new() -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(3))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| ProviderError::Unavailable)?;
        Ok(Self { client })
    }

    /// Fetches and parses one keystone's build page. `Ok(None)` means the page
    /// loaded but carried no build for that filter.
    pub async fn build(
        &self,
        champion_slug: &str,
        lane: &str,
        tier: &str,
        keystone: i64,
    ) -> Result<Option<KeystoneBuild>, ProviderError> {
        let response = self
            .client
            .get(url(champion_slug, lane, tier, keystone))
            .header(reqwest::header::ACCEPT, "text/html")
            .send()
            .await
            .map_err(|e| ProviderError::from_reqwest(&e))?;
        if !response.status().is_success() {
            return Err(ProviderError::from_status(response.status()));
        }
        let body = response
            .text()
            .await
            .map_err(|_| ProviderError::Unavailable)?;
        Ok(parse_build(&body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHRI: &str = include_str!("../../tests/fixtures/lolalytics-ahri-mid-8112.html");
    const JINX: &str = include_str!("../../tests/fixtures/lolalytics-jinx-adc-8008.html");

    fn no_duplicates(ids: &[i64]) -> bool {
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len() == ids.len()
    }

    #[test]
    fn parses_two_real_keystone_pages_into_their_builds() {
        let ahri = parse_build(AHRI).expect("the Ahri page should give a build");
        // Item 5 and Item 6 both have Rabadon's (3089) at the top; the second
        // slot falls through to its next option (Void Staff, 3135).
        assert_eq!(ahri.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
        assert_eq!(ahri.games, 70627);
        assert!(
            no_duplicates(&ahri.items),
            "Ahri build has duplicates: {:?}",
            ahri.items
        );

        let jinx = parse_build(JINX).expect("the Jinx page should give a build");
        assert_eq!(jinx.items, vec![2523, 3006, 3085, 3031, 3036, 3026]);
        assert_eq!(jinx.games, 133854);
        assert!(jinx.games > 0);
        assert!(no_duplicates(&jinx.items));
        assert_ne!(ahri.items, jinx.items);
    }

    #[test]
    fn a_slot_whose_top_option_is_used_falls_through_to_its_next_option() {
        // Item 5 and Item 6 share the same top item (3089); Item 6's next
        // option (3135) must be chosen so the build has no duplicate legendaries.
        let html = r#"<script type="qwik/json">{"objs":[
            {"pick":"1","win":"2"},
            {"items":"3"},
            {"items":"4"},
            {"core":"5","item4":"6","item5":"7","item6":"8","start":"9"},
            {"core":"5"},
            {"set":[3118,3020,4645]},
            [{"id":3157}],
            [{"id":3089}],
            [{"id":3089},{"id":3135}],
            {"n":10}
        ]}</script>"#;
        let build = parse_build(html).expect("the hand-built state should parse");
        assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
        assert!(no_duplicates(&build.items));

        // When a slot has no unused option at all, it is simply left out.
        let html = r#"<script type="qwik/json">{"objs":[
            {"pick":"1","win":"2"},
            {"items":"3"},
            {"items":"4"},
            {"core":"5","item4":"6","item5":"7","item6":"8","start":"9"},
            {"core":"5"},
            {"set":[3118,3020,4645]},
            [{"id":3157}],
            [{"id":3089}],
            [{"id":3089}],
            {"n":10}
        ]}</script>"#;
        let build = parse_build(html).expect("the hand-built state should parse");
        assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089]);
    }

    #[test]
    fn parses_starter_stacks_core_evidence_and_slot_options() {
        let html = r#"<script type="qwik/json">{"objs":[
            {"pick":"1","win":"2"},
            {"items":"3"},
            {"items":"3"},
            {"core":"4","item4":"5","item5":"6","item6":"7","start":"8"},
            {"set":[3118,3020,4645],"n":200,"wr":54.2},
            [{"id":3157,"n":100,"wr":55},{"id":3161,"n":20,"wr":52}],
            [{"id":3089,"n":90,"wr":57},{"id":3124,"n":30,"wr":51}],
            [{"id":3089,"n":80,"wr":58},{"id":3135,"n":45,"wr":53}],
            {"set":[1054,2003,2003],"setUnique":[1054,2003],"count":[1,2],"n":1000}
        ]}</script>"#;
        let build = parse_build(html).expect("the hand-built state should parse");
        assert_eq!(build.starters, vec![ItemStack { id: 1054, count: 1 }, ItemStack { id: 2003, count: 2 }]);
        assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
        assert_eq!(build.games, 1000);
        assert_eq!(build.core_games, 200);
        assert_eq!(build.core_win_pct, Some(54.2));
        assert_eq!(build.options.len(), 2);
        assert!(build.options.iter().any(|option| option.id == 3161 && option.slot == 4));
        assert!(build.options.iter().any(|option| option.id == 3124 && option.slot == 5));
    }

    #[test]
    fn a_page_without_the_anchors_or_state_gives_no_build() {
        assert!(parse_build("<html><body>Just a moment…</body></html>").is_none());
        assert!(parse_build("").is_none());
        // A Qwik payload that is not valid JSON must not panic.
        assert!(parse_build(r#"<script type="qwik/json">{not json</script>"#).is_none());
    }

    #[test]
    fn resolves_base36_references_and_reads_the_most_common_build() {
        // A hand-built but well-formed Qwik state: objs[0] is the pick/win
        // summary, and every index is base-36 (a = 10). The `pick` branch is
        // the most common build.
        let html = r#"<script type="qwik/json">{"objs":[
            {"pick":"1","win":"2"},
            {"items":"3"},
            {"items":"4"},
            {"core":"5","item4":"6","item5":"7","item6":"8","start":"9"},
            {"core":"5","item4":"a"},
            {"set":[3118,3020,4645]},
            [{"id":3157}],
            [{"id":3089}],
            [{"id":3089},{"id":3135}],
            {"n":70627},
            [{"id":100}]
        ]}</script>"#;
        let build = parse_build(html).expect("the hand-built state should parse");
        assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
        assert_eq!(build.games, 70627);
    }

    #[test]
    fn maps_champion_names_to_lolalytics_slugs() {
        assert_eq!(champion_slug("Ahri").as_deref(), Some("ahri"));
        assert_eq!(champion_slug("Lee Sin").as_deref(), Some("leesin"));
        assert_eq!(champion_slug("Kai'Sa").as_deref(), Some("kaisa"));
        assert_eq!(champion_slug("Dr. Mundo").as_deref(), Some("drmundo"));
        assert_eq!(champion_slug("Cho'Gath").as_deref(), Some("chogath"));
        assert_eq!(champion_slug("Jarvan IV").as_deref(), Some("jarvaniv"));
        assert_eq!(champion_slug("Kog'Maw").as_deref(), Some("kogmaw"));
        assert_eq!(champion_slug("LeBlanc").as_deref(), Some("leblanc"));
        assert_eq!(champion_slug("Rek'Sai").as_deref(), Some("reksai"));
        assert_eq!(champion_slug("Tahm Kench").as_deref(), Some("tahmkench"));
        assert_eq!(
            champion_slug("Twisted Fate").as_deref(),
            Some("twistedfate")
        );
        assert_eq!(champion_slug("Xin Zhao").as_deref(), Some("xinzhao"));
        assert_eq!(
            champion_slug("Aurelion Sol").as_deref(),
            Some("aurelionsol")
        );
        assert_eq!(champion_slug("Master Yi").as_deref(), Some("masteryi"));
        assert_eq!(
            champion_slug("Miss Fortune").as_deref(),
            Some("missfortune")
        );
        assert_eq!(champion_slug("Bel'Veth").as_deref(), Some("belveth"));
        assert_eq!(champion_slug("K'Sante").as_deref(), Some("ksante"));
        // Site-specific shorter slugs.
        assert_eq!(champion_slug("Wukong").as_deref(), Some("wukong"));
        assert_eq!(champion_slug("MonkeyKing").as_deref(), Some("wukong"));
        assert_eq!(champion_slug("Nunu & Willump").as_deref(), Some("nunu"));
        assert_eq!(champion_slug("Renata Glasc").as_deref(), Some("renata"));
        assert_eq!(champion_slug(""), None);
        assert_eq!(champion_slug("  "), None);
    }

    #[test]
    fn maps_positions_to_lolalytics_lanes() {
        assert_eq!(lane("top"), Some("top"));
        assert_eq!(lane("JUNGLE"), Some("jungle"));
        assert_eq!(lane("mid"), Some("middle"));
        assert_eq!(lane("middle"), Some("middle"));
        assert_eq!(lane("adc"), Some("bottom"));
        assert_eq!(lane("support"), Some("support"));
        assert_eq!(lane("none"), None);
        assert_eq!(lane("aram"), None);
        assert_eq!(lane(""), None);
    }

    #[test]
    fn maps_rank_brackets_and_defaults_unknown_ones() {
        assert_eq!(tier_slug("all"), "all");
        assert_eq!(tier_slug("EMERALD_PLUS"), "emerald_plus");
        assert_eq!(tier_slug("diamond_plus"), "diamond_plus");
        assert_eq!(tier_slug("grandmaster_plus"), DEFAULT_TIER);
        assert_eq!(tier_slug(""), DEFAULT_TIER);
    }

    #[test]
    fn the_cache_key_carries_the_keystone_and_bracket() {
        let base = cache_key("ahri", "middle", "emerald_plus", 8112);
        assert_eq!(base, "ahri|middle|emerald_plus|8112");
        assert_ne!(base, cache_key("ahri", "middle", "emerald_plus", 8229));
        assert_ne!(base, cache_key("ahri", "middle", "diamond_plus", 8112));
        assert_ne!(base, cache_key("ahri", "top", "emerald_plus", 8112));
        // An unknown bracket is normalized into the key.
        assert_eq!(
            cache_key("ahri", "middle", "nonsense", 8112),
            cache_key("ahri", "middle", DEFAULT_TIER, 8112)
        );
    }

    #[test]
    fn caches_successes_for_thirty_minutes_and_failures_for_five() {
        let now = Instant::now();
        let mut cache = BuildCache::default();
        let key = cache_key("ahri", "middle", "emerald_plus", 8112);
        assert!(matches!(cache.lookup_at(&key, now), Cached::Miss));

        cache.store(
            key.clone(),
            KeystoneBuild {
                items: vec![1, 2, 3],
                games: 10,
                ..KeystoneBuild::default()
            },
        );
        assert!(matches!(cache.lookup_at(&key, now), Cached::Fresh(_)));
        assert!(matches!(
            cache.lookup_at(&key, now + SUCCESS_TTL / 2),
            Cached::Fresh(_)
        ));
        assert!(matches!(
            cache.lookup_at(&key, now + SUCCESS_TTL + Duration::from_secs(1)),
            Cached::Miss
        ));

        let failed = cache_key("ahri", "middle", "emerald_plus", 8229);
        cache.fail(failed.clone());
        assert!(matches!(
            cache.lookup_at(&failed, Instant::now()),
            Cached::Unavailable
        ));
    }

    #[test]
    fn builds_the_endpoint_url_with_the_keystone() {
        assert_eq!(
            url("ahri", "middle", "emerald_plus", 8112),
            "https://lolalytics.com/lol/ahri/build/?lane=middle&tier=emerald_plus&keystone=8112"
        );
        // An unknown bracket is normalized in the URL too.
        assert_eq!(
            url("leesin", "jungle", "bogus", 8010),
            "https://lolalytics.com/lol/leesin/build/?lane=jungle&tier=emerald_plus&keystone=8010"
        );
    }
}
