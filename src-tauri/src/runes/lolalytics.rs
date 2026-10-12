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

use super::patch::{blend_choices, blend_rate, PatchInfo};
use super::provider::ProviderError;

/// The build page. `lane` and `tier` are slugs; `keystone` is a Riot perk id.
pub(crate) const BASE_URL: &str = "https://lolalytics.com/lol";
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
    /// How many leading `items` are the jointly observed core combination
    /// (0 when the page gave no joint sample, e.g. the markup fallback).
    #[serde(default)]
    pub core_len: usize,
    /// Sample size for the core combination.
    pub core_games: u64,
    /// Win rate for the core combination.
    pub core_win_pct: Option<f64>,
    /// Which ability to max first, second and third, e.g. "QWE". Missing from
    /// builds cached before it was parsed.
    #[serde(default)]
    pub skill_priority: Option<String>,
    /// The most common level 1-15 skill order, e.g. "QEWQQRQEQERE...".
    #[serde(default)]
    pub skill_order: Option<String>,
    /// Every later-slot candidate the page reported (the chosen items too), so
    /// the previous patch's table can be merged in. Empty for the markup fallback.
    #[serde(default)]
    pub slots: Vec<ItemCandidate>,
    /// Which patch this is, and whether the previous one was blended in.
    #[serde(default)]
    pub patch: Option<PatchInfo>,
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

/// `url` pinned to a named patch (`16.19`); `None` is the current patch.
pub(super) fn patched(url: String, patch: Option<&str>) -> String {
    match patch {
        Some(patch) => format!("{url}&patch={patch}"),
        None => url,
    }
}

/// The build page URL for a keystone.
pub fn url(champion_slug: &str, lane: &str, tier: &str, keystone: i64, patch: Option<&str>) -> String {
    patched(
        format!(
            "{BASE_URL}/{champion_slug}/build/?lane={lane}&tier={}&keystone={keystone}",
            tier_slug(tier)
        ),
        patch,
    )
}

/// Cache key for one champion, lane, bracket and keystone.
pub fn cache_key(champion_slug: &str, lane: &str, tier: &str, keystone: i64) -> String {
    format!("{champion_slug}|{lane}|{}|{keystone}", tier_slug(tier))
}

/// The matchup page: the same build page filtered to games against one enemy
/// champion in `vslane`.
pub fn vs_url(champion_slug: &str, enemy_slug: &str, lane: &str, tier: &str, patch: Option<&str>) -> String {
    patched(
        format!(
            "{BASE_URL}/{champion_slug}/vs/{enemy_slug}/build/?lane={lane}&vslane={lane}&tier={}",
            tier_slug(tier)
        ),
        patch,
    )
}

/// Cache key for one champion, enemy, lane and bracket.
pub fn vs_cache_key(champion_slug: &str, enemy_slug: &str, lane: &str, tier: &str) -> String {
    format!("{champion_slug}|vs|{enemy_slug}|{lane}|{}", tier_slug(tier))
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
pub(super) struct Qwik {
    #[serde(default)]
    pub(super) objs: Vec<Value>,
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
        if let Some(mut build) = build_from_summary(&pick) {
            let tables = page_tables(objs);
            build.skill_priority = pick
                .get("skillpriority")
                .and_then(|priority| priority.get("id"))
                .and_then(Value::as_str)
                .and_then(skill_priority)
                .or_else(|| tables.as_ref().and_then(page_skill_priority));
            build.skill_order = super::matchup::parse_skill_order(&pick);
            if let Some(tables) = &tables {
                widen_options(&mut build, tables);
            }
            return Some(build);
        }
    }
    None
}

/// The JSON string inside the page's Qwik state script.
pub(super) fn qwik_json(html: &str) -> Option<&str> {
    let marker = "qwik/json";
    let at = html.find(marker)?;
    let start = html[at..].find('{')? + at;
    let end = html[start..].find("</script>")? + start;
    Some(&html[start..end])
}

/// Resolves one Qwik reference value. Objects and arrays hold references to
/// other `objs` entries as base-36 indices; leaf strings and numbers are used
/// as-is.
///
/// Shared references expand once per use, so a crafted or changed page could
/// blow up exponentially; each top-level call visits at most
/// [`MAX_RESOLVE_NODES`] values and yields `Null` past that.
pub(super) fn resolve(value: &Value, objs: &[Value], depth: u8) -> Value {
    let mut budget = MAX_RESOLVE_NODES;
    resolve_within(value, objs, depth, &mut budget)
}

/// Far above what a real page needs (a full pick summary is a few thousand).
const MAX_RESOLVE_NODES: usize = 100_000;

fn resolve_within(value: &Value, objs: &[Value], depth: u8, budget: &mut usize) -> Value {
    if depth > 40 || *budget == 0 {
        return Value::Null;
    }
    *budget -= 1;
    match value {
        Value::String(text) => match base36_index(text) {
            Some(index) if index < objs.len() => {
                resolve_entry(&objs[index], objs, depth + 1, budget)
            }
            _ => value.clone(),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve_within(item, objs, depth + 1, budget))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, item) in map {
                out.insert(key.clone(), resolve_within(item, objs, depth + 1, budget));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Resolves an `objs` entry: a leaf string is a literal, while an object or
/// array has its values resolved.
fn resolve_entry(entry: &Value, objs: &[Value], depth: u8, budget: &mut usize) -> Value {
    match entry {
        Value::String(_) => entry.clone(),
        other => resolve_within(other, objs, depth, budget),
    }
}

/// A number lolalytics sometimes sends as a string ("51.03").
pub(super) fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

/// Items below this share of a slot's games are left out of Options, so a
/// handful of off-meta picks cannot crowd out real alternatives.
const MIN_OPTION_SHARE_PCT: u64 = 2;

/// Whether a candidate has enough of its slot's games to be recommended; one
/// off-meta pick with a tiny sample (and often a huge win rate) must not
/// become part of a build or crowd out real alternatives.
fn is_supported(weight: f64, slot_total: f64) -> bool {
    weight * 100.0 >= slot_total * MIN_OPTION_SHARE_PCT as f64
}

/// A later-slot candidate with the weight that orders and selects it: its games
/// on a plain page, the patch-blended weight after [`blend_build`].
struct Ranked {
    candidate: ItemCandidate,
    weight: f64,
}

/// Completes `ids` (the core) with the strongest unused supported choice of
/// each later slot, and returns the remaining supported candidates, best first,
/// as the options. Each slot is ordered best first.
fn select_path(ids: &mut Vec<i64>, slots: Vec<Vec<Ranked>>) -> Vec<ItemCandidate> {
    let totals: Vec<f64> = slots
        .iter()
        .map(|slot| slot.iter().map(|ranked| ranked.weight).sum())
        .collect();
    for (slot, total) in slots.iter().zip(&totals) {
        if let Some(ranked) = slot
            .iter()
            .find(|ranked| !ids.contains(&ranked.candidate.id) && is_supported(ranked.weight, *total))
        {
            ids.push(ranked.candidate.id);
        }
    }
    let mut options: Vec<Ranked> = Vec::new();
    for (slot, total) in slots.into_iter().zip(totals) {
        for ranked in slot {
            if ids.contains(&ranked.candidate.id) || !is_supported(ranked.weight, total) {
                continue;
            }
            match options
                .iter_mut()
                .find(|item| item.candidate.id == ranked.candidate.id)
            {
                Some(existing) if ranked.weight > existing.weight => *existing = ranked,
                Some(_) => {}
                None => options.push(ranked),
            }
        }
    }
    options.sort_by(|left, right| {
        right
            .weight
            .total_cmp(&left.weight)
            .then_with(|| left.candidate.slot.cmp(&right.candidate.slot))
            .then_with(|| {
                right
                    .candidate
                    .win_pct
                    .unwrap_or_default()
                    .total_cmp(&left.candidate.win_pct.unwrap_or_default())
            })
    });
    options.into_iter().map(|ranked| ranked.candidate).collect()
}

/// Blends the previous patch's build into the current one: later-slot choices
/// and their win rates are merged with [`blend_choices`], and the path and
/// options are re-chosen from the merged weights. Candidates keep the current
/// patch's game counts. The core, starters and skill order stay the current
/// patch's (the page only reports its single most common one); the core's win
/// rate is blended when both patches have the same core.
pub fn blend_build(mut now: KeystoneBuild, prev: Option<&KeystoneBuild>, info: PatchInfo) -> KeystoneBuild {
    now.patch = Some(info);
    let Some(prev) = prev else {
        return now;
    };
    if now.core_len == 0 || now.slots.is_empty() {
        return now;
    }
    let sorted_core = |build: &KeystoneBuild| {
        let mut core = build.items[..build.core_len.min(build.items.len())].to_vec();
        core.sort_unstable();
        core
    };
    if let (Some(rate), Some(prev_rate)) = (now.core_win_pct, prev.core_win_pct) {
        if sorted_core(&now) == sorted_core(prev) {
            now.core_win_pct = Some(blend_rate(rate, now.core_games, prev_rate));
        }
    }
    let table = |build: &KeystoneBuild, slot: u8| -> Vec<(i64, u64, Option<f64>)> {
        build
            .slots
            .iter()
            .filter(|candidate| candidate.slot == slot)
            .map(|candidate| (candidate.id, candidate.games, candidate.win_pct))
            .collect()
    };
    let ranked: Vec<Vec<Ranked>> = [4u8, 5, 6]
        .into_iter()
        .map(|slot| {
            let now_table = table(&now, slot);
            let mut merged: Vec<Ranked> = blend_choices(&now_table, &table(prev, slot))
                .into_iter()
                .map(|(id, rate)| Ranked {
                    candidate: ItemCandidate {
                        id,
                        slot,
                        games: now_table
                            .iter()
                            .find(|(other, _, _)| *other == id)
                            .map_or(0, |(_, games, _)| *games),
                        win_pct: (rate.win_pct > 0.0).then_some(rate.win_pct),
                    },
                    weight: rate.weight,
                })
                .collect();
            merged.sort_by(|left, right| right.weight.total_cmp(&left.weight));
            merged
        })
        .collect();
    let mut items = now.items[..now.core_len.min(now.items.len())].to_vec();
    now.options = select_path(&mut items, ranked);
    now.items = items;
    now
}

/// `"QWE"`-style max order, when it names each basic ability exactly once.
fn skill_priority(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_uppercase();
    let mut letters: Vec<char> = text.chars().collect();
    letters.sort_unstable();
    (letters == ['E', 'Q', 'W']).then_some(text)
}

/// The page-wide build tables: per-slot item rows and the skill order list,
/// resolved. Lolalytics numbers these tables from 0, so `item3` is the 4th
/// item slot.
fn page_tables(objs: &[Value]) -> Option<Value> {
    objs.iter().find_map(|entry| {
        let map = entry.as_object()?;
        if !["item3", "item4", "item5"]
            .iter()
            .all(|key| map.contains_key(*key))
        {
            return None;
        }
        let resolved = resolve(entry, objs, 0);
        resolved["item3"]
            .as_array()
            .and_then(|rows| rows.first())
            .is_some_and(Value::is_array)
            .then_some(resolved)
    })
}

/// The most played max order from the page-wide skill order table.
fn page_skill_priority(tables: &Value) -> Option<String> {
    tables["skillOrder"]
        .as_array()?
        .first()?
        .get(0)?
        .as_str()
        .and_then(skill_priority)
}

/// Adds every reasonably played later-slot item from the page-wide tables to
/// the build's options. Rows are `[id, win %, pick %, games, …]`.
fn widen_options(build: &mut KeystoneBuild, tables: &Value) {
    for (slot, key) in [(4u8, "item3"), (5, "item4"), (6, "item5")] {
        let Some(rows) = tables[key].as_array() else {
            continue;
        };
        let candidates: Vec<ItemCandidate> = rows
            .iter()
            .filter_map(|row| {
                let id = row.get(0).and_then(as_i64)?;
                (id > 0).then_some(ItemCandidate {
                    id,
                    slot,
                    games: row.get(3).and_then(as_u64).unwrap_or(0),
                    win_pct: row.get(1).and_then(Value::as_f64),
                })
            })
            .collect();
        let total: f64 = candidates.iter().map(|candidate| candidate.games as f64).sum();
        for candidate in candidates {
            upsert_slot(&mut build.slots, &candidate);
            if !is_supported(candidate.games as f64, total)
                || build.items.contains(&candidate.id)
                || build.starters.iter().any(|stack| stack.id == candidate.id)
            {
                continue;
            }
            match build
                .options
                .iter_mut()
                .find(|option| option.id == candidate.id)
            {
                Some(existing) if candidate.games > existing.games => *existing = candidate,
                Some(_) => {}
                None => build.options.push(candidate),
            }
        }
    }
    sort_options(&mut build.options);
}

/// Records a candidate in the per-slot table, keeping the larger sample when the
/// slot already has the item.
fn upsert_slot(slots: &mut Vec<ItemCandidate>, candidate: &ItemCandidate) {
    match slots
        .iter_mut()
        .find(|existing| existing.id == candidate.id && existing.slot == candidate.slot)
    {
        Some(existing) if candidate.games > existing.games => *existing = candidate.clone(),
        Some(_) => {}
        None => slots.push(candidate.clone()),
    }
}

fn sort_options(options: &mut [ItemCandidate]) {
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
    let core_len = ids.len();
    let slots: Vec<ItemCandidate> = slot_candidates.iter().flatten().cloned().collect();
    let ranked = slot_candidates
        .into_iter()
        .map(|candidates| {
            candidates
                .into_iter()
                .map(|candidate| Ranked {
                    weight: candidate.games as f64,
                    candidate,
                })
                .collect()
        })
        .collect();
    let options = select_path(&mut ids, ranked);

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
        core_len,
        core_games,
        core_win_pct,
        skill_priority: None,
        skill_order: None,
        slots,
        patch: None,
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

pub(super) fn as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|number| number as i64))
}

pub(super) fn as_u64(value: &Value) -> Option<u64> {
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
        core_len: 0,
        core_games: 0,
        core_win_pct: None,
        skill_priority: None,
        skill_order: None,
        slots: Vec::new(),
        patch: None,
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
pub enum Cached<T = KeystoneBuild> {
    Fresh(T),
    Unavailable,
    Miss,
}

/// Entries kept per cache; past this the oldest are dropped, so a long
/// session cannot grow the cache without bound.
const MAX_CACHE_ENTRIES: usize = 256;

/// Successes and failures for keystone builds, keyed by champion/lane/tier/
/// keystone. Matchup pages use the same cache with their own value type.
pub struct BuildCache<T = KeystoneBuild> {
    success: std::collections::HashMap<String, (Instant, T)>,
    failures: std::collections::HashMap<String, Instant>,
}

impl<T> Default for BuildCache<T> {
    fn default() -> Self {
        Self {
            success: std::collections::HashMap::new(),
            failures: std::collections::HashMap::new(),
        }
    }
}

impl<T: Clone> BuildCache<T> {
    pub fn lookup(&self, key: &str) -> Cached<T> {
        self.lookup_at(key, Instant::now())
    }

    pub fn lookup_at(&self, key: &str, now: Instant) -> Cached<T> {
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

    pub fn store(&mut self, key: String, build: T) {
        self.failures.remove(&key);
        self.success.insert(key, (Instant::now(), build));
        self.prune(Instant::now());
    }

    pub fn fail(&mut self, key: String) {
        self.failures.insert(key, Instant::now());
        self.prune(Instant::now());
    }

    /// Drops expired entries, then the oldest ones past [`MAX_CACHE_ENTRIES`].
    fn prune(&mut self, now: Instant) {
        self.success
            .retain(|_, (at, _)| now.saturating_duration_since(*at) < SUCCESS_TTL);
        self.failures
            .retain(|_, at| now.saturating_duration_since(*at) < FAILURE_TTL);
        while self.success.len() > MAX_CACHE_ENTRIES {
            let Some(oldest) = self
                .success
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.success.remove(&oldest);
        }
        while self.failures.len() > MAX_CACHE_ENTRIES {
            let Some(oldest) = self
                .failures
                .iter()
                .min_by_key(|(_, at)| **at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.failures.remove(&oldest);
        }
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
        patch: Option<&str>,
    ) -> Result<Option<KeystoneBuild>, ProviderError> {
        let body = self
            .page(url(champion_slug, lane, tier, keystone, patch))
            .await?;
        Ok(parse_build(&body))
    }

    /// The HTML of one lolalytics page.
    pub(super) async fn page(&self, url: String) -> Result<String, ProviderError> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "text/html")
            .send()
            .await
            .map_err(|e| ProviderError::from_reqwest(&e))?;
        if !response.status().is_success() {
            return Err(ProviderError::from_status(response.status()));
        }
        response
            .text()
            .await
            .map_err(|_| ProviderError::Unavailable)
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
        assert_eq!(
            build.starters,
            vec![
                ItemStack { id: 1054, count: 1 },
                ItemStack { id: 2003, count: 2 }
            ]
        );
        assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
        assert_eq!(build.games, 1000);
        assert_eq!(build.core_games, 200);
        assert_eq!(build.core_win_pct, Some(54.2));
        assert_eq!(build.options.len(), 2);
        assert!(build
            .options
            .iter()
            .any(|option| option.id == 3161 && option.slot == 4));
        assert!(build
            .options
            .iter()
            .any(|option| option.id == 3124 && option.slot == 5));
    }

    /// A page with the build summary plus the page-wide per-slot tables and
    /// skill data, shaped like lolalytics' Qwik state.
    const PAGE_WITH_TABLES: &str = r#"<script type="qwik/json">{"objs":[
            {"pick":"1","win":"2"},
            {"items":"3","skillpriority":"9"},
            {"items":"3"},
            {"core":"4","item4":"5","item5":"6","item6":"7","start":"8"},
            {"set":[3118,3020,4645],"n":200,"wr":54.2},
            [{"id":3157,"n":100,"wr":55}],
            [{"id":3089,"n":90,"wr":57}],
            [{"id":3135,"n":45,"wr":53}],
            {"set":[1056,2003],"setUnique":[1056,2003],"count":[1,1],"n":1000},
            {"id":"QWE","n":800,"wr":52.4},
            {"item3":"b","item4":"c","item5":"d","skillOrder":"e"},
            [[3157,55.7,33.7,1000,31],[3041,79.5,7.2,300,27],[3100,67.9,3.3,150,30],[3102,56.6,1.9,5,29]],
            [[3089,60.8,31.7,600,31],[3135,54.4,8.3,200,35],[3137,58.4,3.8,80,34]],
            [[3165,63.1,3.3,100,40],[3089,62.8,20.8,400,34]],
            [["QWE",52.4,91.6,800],["QEW",52.5,7.8,70]]
        ]}</script>"#;

    #[test]
    fn reads_the_level_by_level_skill_order_from_a_real_build_page() {
        let order = parse_build(AHRI).unwrap().skill_order.expect("the Ahri page has a skill order");
        assert_eq!(order.len(), 15);
        assert!(order.chars().all(|c| "QWER".contains(c)), "{order}");
    }

    #[test]
    fn reads_the_skill_priority_from_the_build_summary() {
        let build = parse_build(PAGE_WITH_TABLES).expect("the page should parse");
        assert_eq!(build.skill_priority.as_deref(), Some("QWE"));
    }

    #[test]
    fn an_unreadable_skill_priority_is_left_out() {
        let html = PAGE_WITH_TABLES.replace(r#""id":"QWE""#, r#""id":"QXZ""#);
        let build = parse_build(&html).expect("the page should parse");
        // Falls back to the page-wide skill order.
        assert_eq!(build.skill_priority.as_deref(), Some("QWE"));
        let html = html.replace(r#"["QWE",52.4"#, r#"["Q",52.4"#);
        assert_eq!(parse_build(&html).unwrap().skill_priority, None);
    }

    #[test]
    fn the_full_slot_tables_widen_the_options() {
        let build = parse_build(PAGE_WITH_TABLES).expect("the page should parse");
        assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
        let ids: Vec<i64> = build.options.iter().map(|option| option.id).collect();
        // Mejai's, Lich Bane, Morellonomicon, Void Staff… but never an item
        // already in the build, and not Banshee's with 5 of 1455 games.
        for expected in [3041, 3100, 3137, 3165] {
            assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
        }
        assert!(!ids.contains(&3102), "rare pick kept: {ids:?}");
        for built in &build.items {
            assert!(!ids.contains(built), "build item {built} in options");
        }
        let void_staff = build
            .options
            .iter()
            .find(|option| option.id == 3165)
            .unwrap();
        assert_eq!((void_staff.slot, void_staff.games), (6, 100));
    }

    #[test]
    fn only_the_core_is_a_joint_sample_and_a_tiny_slot_pick_is_never_recommended() {
        // Slot 6's only unused item has 5 of 905 games (0.6%) but a 90% win
        // rate: it must neither join the build nor the options.
        let html = r#"<script type="qwik/json">{"objs":[
            {"pick":"1","win":"2"},
            {"items":"3"},
            {"items":"3"},
            {"core":"4","item4":"5","item5":"6","item6":"7","start":"8"},
            {"set":[3118,3020,4645],"n":200,"wr":54.2},
            [{"id":3157,"n":100,"wr":55}],
            [{"id":3089,"n":90,"wr":57}],
            [{"id":3089,"n":900,"wr":58},{"id":3135,"n":5,"wr":90}],
            {"set":[1056],"setUnique":[1056],"count":[1],"n":1000}
        ]}</script>"#;
        let build = parse_build(html).expect("the hand-built state should parse");
        assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089]);
        assert_eq!(build.core_len, 3);
        assert!(build.options.iter().all(|option| option.id != 3135));
    }

    #[test]
    fn the_markup_fallback_reports_no_joint_core() {
        assert_eq!(parse_build(AHRI).unwrap().core_len, 3);
        let anchors = r#"Core Build <img src="x/item64/3118.webp"> Item 4 <img src="x/item64/3157.webp"> Item 5 <img src="x/item64/3089.webp"> Item 6 <img src="x/item64/3135.webp">"#;
        assert_eq!(parse_build(anchors).unwrap().core_len, 0);
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
    fn the_cache_is_bounded_and_drops_expired_entries() {
        let mut cache = BuildCache::<u32>::default();
        for index in 0..(MAX_CACHE_ENTRIES + 50) {
            cache.store(format!("k{index}"), index as u32);
            cache.fail(format!("f{index}"));
        }
        assert_eq!(cache.success.len(), MAX_CACHE_ENTRIES);
        assert_eq!(cache.failures.len(), MAX_CACHE_ENTRIES);
        // The newest survive; the oldest are gone.
        assert!(matches!(cache.lookup("k305"), Cached::Fresh(305)));
        assert!(matches!(cache.lookup("k0"), Cached::Miss));
        // Expired entries are pruned on the next write.
        cache.prune(Instant::now() + SUCCESS_TTL + Duration::from_secs(1));
        assert!(cache.success.is_empty() && cache.failures.is_empty());
    }

    #[test]
    fn resolving_shared_references_stays_within_the_node_budget() {
        // Each level references the previous one twice: 2^30 leaves unbudgeted.
        let mut objs = vec![serde_json::json!(1)];
        for level in 1..=30usize {
            objs.push(serde_json::json!([base36(level - 1), base36(level - 1)]));
        }
        let resolved = resolve(&Value::String(base36(30)), &objs, 0);
        // Terminates, and the budget cut leaves Nulls rather than a huge tree.
        assert!(resolved.is_array());
    }

    fn base36(mut number: usize) -> String {
        let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut out = vec![digits[number % 36]];
        while number >= 36 {
            number /= 36;
            out.push(digits[number % 36]);
        }
        out.reverse();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn builds_the_endpoint_url_with_the_keystone() {
        assert_eq!(
            url("ahri", "middle", "emerald_plus", 8112, None),
            "https://lolalytics.com/lol/ahri/build/?lane=middle&tier=emerald_plus&keystone=8112"
        );
        // An unknown bracket is normalized in the URL too.
        assert_eq!(
            url("leesin", "jungle", "bogus", 8010, None),
            "https://lolalytics.com/lol/leesin/build/?lane=jungle&tier=emerald_plus&keystone=8010"
        );
    }

    fn blend_info() -> PatchInfo {
        PatchInfo { patch: "16.20".into(), previous: Some("16.19".into()) }
    }

    fn with_games(build: &KeystoneBuild, games: u64) -> KeystoneBuild {
        KeystoneBuild {
            // Reversed: the current patch has not settled on an order yet.
            slots: build.slots.iter().rev().map(|c| ItemCandidate { games, ..c.clone() }).collect(),
            ..build.clone()
        }
    }

    #[test]
    fn the_real_page_keeps_every_slot_candidate() {
        let build = parse_build(AHRI).unwrap();
        assert!(build.slots.len() >= build.options.len(), "slots hold options and chosen items");
        for slot in [4u8, 5, 6] {
            assert!(build.slots.iter().any(|c| c.slot == slot), "slot {slot} is empty");
        }
    }

    #[test]
    fn a_day_zero_build_is_the_previous_patchs_path_with_current_game_counts() {
        let prev = parse_build(AHRI).unwrap();
        let now = with_games(&prev, 0);
        let blended = blend_build(KeystoneBuild { items: now.items[..now.core_len].to_vec(), ..now }, Some(&prev), blend_info());
        assert_eq!(blended.items, prev.items);
        assert!(blended.options.iter().all(|option| option.games == 0));
        assert!(blended.patch.unwrap().previous.is_some());
    }

    #[test]
    fn a_current_only_item_can_take_a_slot_and_a_big_sample_keeps_its_rate() {
        let prev = parse_build(AHRI).unwrap();
        let mut now = with_games(&prev, 4000);
        now.slots.push(ItemCandidate { id: 99_999, slot: 4, games: 9000, win_pct: Some(61.0) });
        let blended = blend_build(now, Some(&prev), blend_info());
        assert_eq!(blended.items[prev.core_len], 99_999, "the new item is the strongest slot-4 pick");
        let shown = blended.options.iter().find(|option| option.games == 4000);
        assert!(shown.is_some(), "options show current games, never the weighted ones");
    }

    #[test]
    fn the_build_url_pins_the_previous_patch() {
        assert!(url("ahri", "middle", "emerald_plus", 8112, Some("16.19")).ends_with("&keystone=8112&patch=16.19"));
        assert!(!url("ahri", "middle", "emerald_plus", 8112, None).contains("patch"));
    }
}
