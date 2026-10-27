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
use super::health::{self, Source};
use super::provider::ProviderError;

mod client;
mod markup;

pub(crate) use client::checked;
pub use client::{BuildCache, Cached, LolalyticsClient};
use markup::parse_anchors;

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

/// Both summaries from a single page. Flattening preserves old pick-only caches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeystoneBuilds {
    #[serde(flatten)]
    pub pick: KeystoneBuild,
    #[serde(default)]
    pub win: Option<WinBuild>,
    /// The "Most Common Build" runes and spells; missing from builds cached before they were parsed.
    #[serde(default)]
    pub pick_runes: Option<super::matchup::MatchupRunes>,
    #[serde(default)]
    pub pick_spells: Option<[i64; 2]>,
}

/// How often one keystone is played on a champion page (lane and bracket).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeystoneShare {
    pub id: i64,
    pub pick_pct: f64,
    pub games: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WinBuild {
    pub build: KeystoneBuild,
    pub runes: super::matchup::MatchupRunes,
    pub spells: Option<[i64; 2]>,
}

impl WinBuild {
    /// Compare with the actual op.gg preset, whose runes may differ from pick's.
    pub fn matches_most_played(
        &self,
        selection: &super::RuneSelection,
        pick: &KeystoneBuild,
    ) -> bool {
        let same_ids = |left: &[i64], right: &[i64]| {
            let mut left = left.to_vec();
            let mut right = right.to_vec();
            left.sort_unstable();
            right.sort_unstable();
            left == right
        };
        self.runes.keystone == selection.keystone
            && same_ids(&self.runes.primary_runes, &selection.primary_runes)
            && same_ids(&self.runes.secondary_runes, &selection.secondary_runes)
            // Shards belong to specific rows; their order matters.
            && self.runes.shards == selection.shards
            && self.build.core_len == 3 && pick.core_len == 3
            && self.build.items.get(..3).zip(pick.items.get(..3))
                .is_some_and(|(win, pick)| same_ids(win, pick))
    }
}

pub fn blend_builds(
    now: KeystoneBuilds,
    prev: Option<&KeystoneBuilds>,
    info: PatchInfo,
) -> KeystoneBuilds {
    KeystoneBuilds {
        pick: blend_build(now.pick, prev.map(|p| &p.pick), info.clone()),
        win: now.win.map(|win| WinBuild {
            build: blend_build(
                win.build,
                prev.and_then(|p| p.win.as_ref()).map(|w| &w.build),
                info,
            ),
            ..win
        }),
        pick_runes: now.pick_runes,
        pick_spells: now.pick_spells,
    }
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
    parse_builds(html).map(|builds| builds.pick)
}

pub fn parse_builds(html: &str) -> Option<KeystoneBuilds> {
    parse_qwik(html).or_else(|| parse_anchors(html).map(|pick| KeystoneBuilds { pick, win: None, pick_runes: None, pick_spells: None }))
}

/// The compacted state inside `<script type="qwik/json">`.
#[derive(Deserialize)]
pub(super) struct Qwik {
    #[serde(default)]
    pub(super) objs: Vec<Value>,
}

/// Parses the structured Qwik state, preferring the "Most Common Build".
fn parse_qwik(html: &str) -> Option<KeystoneBuilds> {
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
            build.skill_priority = build
                .skill_priority
                .or_else(|| tables.as_ref().and_then(page_skill_priority));
            if let Some(tables) = &tables {
                widen_options(&mut build, tables);
            }
            let win = map.get("win").and_then(|value| {
                let summary = resolve(value, objs, 0);
                Some(WinBuild {
                    build: build_from_summary(&summary)?,
                    runes: super::matchup::parse_runes(&summary)?,
                    spells: super::matchup::parse_spells(&summary),
                })
            });
            return Some(KeystoneBuilds {
                pick: build,
                win,
                pick_runes: super::matchup::parse_runes(&pick),
                pick_spells: super::matchup::parse_spells(&pick),
            });
        }
    }
    None
}

/// Every rune's share on an unfiltered champion page, from the runes table
/// (`runes.stats`: id -> [[pick %, win %, games]]). Keystones have one row;
/// callers keep the ids that are keystones. Empty when the table is missing.
pub(super) fn keystone_shares(objs: &[Value]) -> Vec<KeystoneShare> {
    let Some(holder) = objs.iter().find(|entry| {
        entry.as_object().is_some_and(|map| ["runes", "summary", "header"].iter().all(|key| map.contains_key(*key)))
    }) else {
        return Vec::new();
    };
    // Only the runes table: the holder also references the whole page, past the resolve budget.
    let runes = resolve(&holder["runes"], objs, 0);
    let Some(stats) = runes["stats"].as_object() else {
        return Vec::new();
    };
    stats
        .iter()
        .filter_map(|(id, rows)| {
            let row = rows.as_array()?.first()?.as_array()?;
            Some(KeystoneShare {
                id: id.parse().ok()?,
                pick_pct: number(row.first()?)?,
                games: as_u64(row.get(2)?)?,
            })
        })
        .collect()
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
pub fn blend_build(
    mut now: KeystoneBuild,
    prev: Option<&KeystoneBuild>,
    info: PatchInfo,
) -> KeystoneBuild {
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
    // Matchup pages (since 16.21) list the first three slots one by one as
    // `item1`..`item3` instead of a joint `core` set; those carry no joint sample.
    let core = items.get("core").and_then(|core| core.get("set")?.as_array());
    let first_slots = || -> Option<Vec<Value>> {
        ["item1", "item2", "item3"]
            .iter()
            .map(|key| items.get(*key)?.get("id").cloned())
            .collect()
    };
    let core = match core {
        Some(core) => core.clone(),
        None => first_slots()?,
    };
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
    let core_len = if items.get("core").is_some() { ids.len() } else { 0 };
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
        skill_priority: summary
            .get("skillpriority")
            .and_then(|p| p.get("id"))
            .and_then(Value::as_str)
            .and_then(skill_priority),
        skill_order: super::matchup::parse_skill_order(summary),
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

#[cfg(test)]
mod tests;
