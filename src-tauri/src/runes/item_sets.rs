//! Creates champion-specific item sets in the League Client's in-game shop.

use reqwest::Method;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{KeystoneBuildView, RuneError};

const CURRENT_SUMMONER_PATH: &str = "/lol-summoner/v1/current-summoner";
const ITEM_SETS_PATH: &str = "/lol-item-sets/v1/item-sets";
const MAX_ITEMS: usize = 7;
const MAX_OPTIONS: usize = 7;
const MAX_OPTION_CANDIDATES: usize = 256;
const MAX_TITLE_CHARS: usize = 50;
/// The champion part of a title is clipped to this many characters.
const MAX_CHAMPION_CHARS: usize = 28;
/// Shop block names. The build list is a final inventory in no purchase order;
/// the first three items are one observed combination, the rest are the most
/// common pick per later slot.
const BUILD_BLOCK: &str = "Core + Popular Picks";
const BUILD_BLOCK_UNCORED: &str = "Popular Picks";
/// A pro game's completed items, as listed; the data has no purchase order here.
const FINAL_ITEMS_BLOCK: &str = "Final Items";
const ALTERNATIVES_BLOCK: &str = "Alternatives";
/// Every item set Swapper creates starts with this, which is how it finds its
/// own sets to replace or clean up without touching the player's.
const TITLE_PREFIX: &str = "Swapper: ";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentSummoner {
    summoner_id: i64,
}

fn clean(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn item_values(item_ids: &[i64]) -> Result<Vec<Value>, RuneError> {
    if item_ids.is_empty() || item_ids.len() > MAX_ITEMS || item_ids.iter().any(|id| *id <= 0) {
        return Err(RuneError::conflict(
            "This build does not contain a valid item list.",
        ));
    }
    Ok(item_ids
        .iter()
        .map(|id| json!({ "id": id.to_string(), "count": 1 }))
        .collect())
}

fn block(label: &str, items: Vec<Value>) -> Value {
    json!({
        "hideIfSummonerSpell": "",
        "items": items,
        "showIfSummonerSpell": "",
        "type": label
    })
}

fn item_set_body(
    champion_id: i64,
    champion_name: &str,
    blocks: Vec<Value>,
) -> Result<(String, Value), RuneError> {
    if !(1..=10_000).contains(&champion_id) {
        return Err(RuneError::conflict(
            "Choose a valid champion before importing items.",
        ));
    }
    if blocks.is_empty() {
        return Err(RuneError::conflict(
            "This build does not contain any item groups.",
        ));
    }
    let champion_name = clean(champion_name, MAX_CHAMPION_CHARS);
    let champion_name = if champion_name.is_empty() {
        "Champion"
    } else {
        champion_name.as_str()
    };
    let title = clean(
        &format!("{TITLE_PREFIX}{champion_name}"),
        MAX_TITLE_CHARS,
    );
    // POST .../sets takes the item set itself as the body. Wrapping it (for
    // example in {"itemSet": …}) is ignored and League saves a blank
    // "New Item Set" instead.
    let body = json!({
        "associatedChampions": [champion_id],
        "associatedMaps": [],
        "blocks": blocks,
        "map": "any",
        "mode": "any",
        "preferredItemSlots": [],
        "sortrank": 0,
        "startedFrom": "blank",
        "title": title,
        "type": "custom",
        "uid": Uuid::new_v4().to_string()
    });
    Ok((title, body))
}

fn generic_item_set_body(
    champion_id: i64,
    champion_name: &str,
    item_ids: &[i64],
) -> Result<(String, Value), RuneError> {
    let items = item_values(item_ids)?;
    item_set_body(champion_id, champion_name, vec![block(FINAL_ITEMS_BLOCK, items)])
}

fn preset_item_set_body(
    champion_id: i64,
    champion_name: &str,
    build: &KeystoneBuildView,
) -> Result<(String, Value), RuneError> {
    if build.items.is_empty()
        || build.items.len() > MAX_ITEMS
        || build.items.iter().any(|item| item.id <= 0)
    {
        return Err(RuneError::conflict(
            "This build does not contain a valid full build.",
        ));
    }
    if build.starters.len() > MAX_ITEMS
        || build
            .starters
            .iter()
            .any(|item| item.id <= 0 || !(1..=20).contains(&item.count))
    {
        return Err(RuneError::conflict(
            "This build does not contain valid starter items.",
        ));
    }
    if build.options.len() > MAX_OPTION_CANDIDATES {
        return Err(RuneError::conflict(
            "This build contains too many item alternatives.",
        ));
    }

    // The ability max order rides on the first block's name: the shop has no
    // other place for it, and the set title stays just "Swapper: Champion".
    let skills = build_label(build.skill_priority.as_deref());
    let named = |name: &str| match &skills {
        Some(skills) => format!("{name} · {skills}"),
        None => name.to_string(),
    };
    let mut blocks = Vec::with_capacity(3);
    if !build.starters.is_empty() {
        let items = build
            .starters
            .iter()
            .map(|item| json!({ "id": item.id.to_string(), "count": item.count }))
            .collect();
        blocks.push(block(&named("Starter Items"), items));
    }
    // `items` is a final inventory, not a purchase order, and only its leading
    // core was observed as one combination (see `KeystoneBuildView::core_len`).
    let build_name = if build.core_len > 0 { BUILD_BLOCK } else { BUILD_BLOCK_UNCORED };
    blocks.push(block(
        &if blocks.is_empty() { named(build_name) } else { build_name.to_string() },
        build
            .items
            .iter()
            .map(|item| json!({ "id": item.id.to_string(), "count": 1 }))
            .collect(),
    ));

    // LoLalytics reports independent later-slot marginals. Round-robin their
    // highest-support alternatives so one slot cannot crowd out the other
    // two, then keep them all in one block.
    let by_slot: Vec<Vec<&super::view::ItemOptionView>> = (4..=6)
        .map(|slot| {
            let mut candidates: Vec<_> = build
                .options
                .iter()
                .filter(|candidate| candidate.slot == slot)
                .collect();
            candidates.sort_by(|left, right| right.games.cmp(&left.games));
            candidates
        })
        .collect();
    let mut options: Vec<Value> = Vec::new();
    let mut used = Vec::new();
    let max_rounds = by_slot.iter().map(Vec::len).max().unwrap_or(0);
    'rounds: for index in 0..max_rounds {
        for candidates in &by_slot {
            let Some(option) = candidates.get(index) else {
                continue;
            };
            if option.id <= 0
                || build.items.iter().any(|item| item.id == option.id)
                || build.starters.iter().any(|item| item.id == option.id)
                || used.contains(&option.id)
            {
                continue;
            }
            used.push(option.id);
            options.push(json!({ "id": option.id.to_string(), "count": 1 }));
            if options.len() == MAX_OPTIONS {
                break 'rounds;
            }
        }
    }
    if !options.is_empty() {
        blocks.push(block(ALTERNATIVES_BLOCK, options));
    }

    item_set_body(champion_id, champion_name, blocks)
}

/// The ability max order as a block-name suffix ("Max Q > W > E"), when
/// lolalytics reported one.
fn build_label(skill_priority: Option<&str>) -> Option<String> {
    let steps: Vec<String> = skill_priority?
        .trim()
        .chars()
        .map(|ability| ability.to_ascii_uppercase().to_string())
        .collect();
    (!steps.is_empty()).then(|| format!("Max {}", steps.join(" > ")))
}

fn is_swapper_set(set: &Value) -> bool {
    set["title"]
        .as_str()
        .is_some_and(|title| title.starts_with(TITLE_PREFIX))
}

/// The player's item set list as League returned it, normalized so it always
/// has an `itemSets` array. Every other field (accountId, …) is kept as-is.
fn normalized(existing: Value) -> Value {
    let mut existing = if existing.is_object() { existing } else { json!({}) };
    if !existing["itemSets"].is_array() {
        existing["itemSets"] = json!([]);
    }
    existing
}

/// The player's list with `item_set` added and any earlier Swapper set
/// dropped, so imports never pile up. The player's own sets are kept.
fn with_item_set_appended(existing: Value, item_set: Value, now_ms: i64) -> Value {
    let mut updated = normalized(existing);
    let sets = updated["itemSets"].as_array_mut().expect("normalized to an array");
    sets.retain(|set| !is_swapper_set(set));
    sets.push(item_set);
    updated["timestamp"] = json!(now_ms);
    updated
}

/// The player's list without Swapper's sets, or `None` when there were none
/// (so nothing needs writing).
fn without_swapper_item_sets(existing: Value, now_ms: i64) -> Option<Value> {
    let mut updated = normalized(existing);
    let sets = updated["itemSets"].as_array_mut().expect("normalized to an array");
    let before = sets.len();
    sets.retain(|set| !is_swapper_set(set));
    if sets.len() == before {
        return None;
    }
    updated["timestamp"] = json!(now_ms);
    Some(updated)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

async fn item_sets_path(lcu: &super::Lcu) -> Result<String, RuneError> {
    let summoner: CurrentSummoner = super::lcu_get(lcu, CURRENT_SUMMONER_PATH).await?;
    if summoner.summoner_id <= 0 {
        return Err(RuneError::unavailable(
            "League did not report the current summoner id.",
        ));
    }
    Ok(format!("{ITEM_SETS_PATH}/{}/sets", summoner.summoner_id))
}

async fn put_item_sets(lcu: &super::Lcu, path: &str, sets: &Value) -> Result<(), RuneError> {
    let response = lcu.send(Method::PUT, path, Some(sets)).await?;
    if response.status().is_success() {
        return Ok(());
    }
    Err(RuneError::conflict(format!(
        "League rejected the item set (HTTP {}).",
        response.status().as_u16()
    )))
}

/// Removes the item sets Swapper imported, e.g. once a game ends. The
/// player's own sets are left alone; nothing is written when there are none.
pub async fn remove_imported_sets() -> Result<(), RuneError> {
    let lcu = super::lcu().await?;
    let path = item_sets_path(&lcu).await?;
    let existing: Value = super::lcu_get(&lcu, &path).await?;
    match without_swapper_item_sets(existing, now_ms()) {
        Some(cleaned) => put_item_sets(&lcu, &path, &cleaned).await,
        None => Ok(()),
    }
}

/// Adds one item set to the player's list. League answers POST .../sets with
/// 204 but saves nothing, so Swapper reads the whole list, appends, and PUTs it
/// back — the same thing the client's own item set editor does.
async fn post_item_set(title: String, body: Value) -> Result<String, RuneError> {
    let lcu = super::lcu().await?;
    let path = item_sets_path(&lcu).await?;
    let existing: Value = super::lcu_get(&lcu, &path).await?;
    put_item_sets(&lcu, &path, &with_item_set_appended(existing, body, now_ms())).await?;
    Ok(title)
}

/// Adds one item set, replacing Swapper's previous one but never the player's.
pub async fn import_build(
    champion_id: i64,
    champion_name: &str,
    item_ids: &[i64],
) -> Result<String, RuneError> {
    let (title, body) = generic_item_set_body(champion_id, champion_name, item_ids)?;
    post_item_set(title, body).await
}

/// Imports a preset build as starter, final-inventory and alternatives blocks.
pub async fn import_keystone_build(
    champion_id: i64,
    champion_name: &str,
    build: &KeystoneBuildView,
) -> Result<String, RuneError> {
    let (title, body) = preset_item_set_body(champion_id, champion_name, build)?;
    post_item_set(title, body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runes::view::{ItemOptionView, ItemStackView, ItemView};

    #[test]
    fn win_summary_imports_its_own_full_item_set() {
        let builds = crate::runes::lolalytics::parse_builds(include_str!(
            "../../tests/fixtures/lolalytics-ahri-mid-8112.html"
        ))
        .unwrap();
        let win = builds.win.unwrap();
        let view = crate::runes::view::keystone_build_view(
            win.build,
            false,
            None,
            &std::collections::HashMap::new(),
        );
        let (_, body) = preset_item_set_body(103, "Ahri", &view).unwrap();
        assert_eq!(
            body["blocks"][0]["items"],
            json!([
                {"id":"1056","count":1}, {"id":"2003","count":2}
            ])
        );
        assert_eq!(
            body["blocks"][1]["items"],
            json!([
                {"id":"2503","count":1}, {"id":"3020","count":1}, {"id":"3100","count":1},
                {"id":"3089","count":1}, {"id":"3157","count":1}, {"id":"3041","count":1}
            ])
        );
        assert_eq!(body["blocks"][2]["items"], json!([{"id":"4645","count":1}]));
        assert!(body["blocks"][0]["type"]
            .as_str()
            .unwrap()
            .contains("Max Q > W > E"));
    }

    fn sample_build() -> KeystoneBuildView {
        KeystoneBuildView {
            starters: vec![
                ItemStackView {
                    id: 1054,
                    name: "Doran's Shield".into(),
                    count: 1,
                },
                ItemStackView {
                    id: 2003,
                    name: "Health Potion".into(),
                    count: 2,
                },
            ],
            items: vec![
                ItemView {
                    id: 6692,
                    name: "Eclipse".into(),
                },
                ItemView {
                    id: 3047,
                    name: "Plated Steelcaps".into(),
                },
                ItemView {
                    id: 3161,
                    name: "Spear of Shojin".into(),
                },
            ],
            options: vec![
                ItemOptionView {
                    id: 3156,
                    name: "Maw of Malmortius".into(),
                    slot: 4,
                    games: 8,
                    win_pct: None,
                },
                ItemOptionView {
                    id: 6695,
                    name: "Serpent's Fang".into(),
                    slot: 5,
                    games: 7,
                    win_pct: None,
                },
                ItemOptionView {
                    id: 3071,
                    name: "Black Cleaver".into(),
                    slot: 5,
                    games: 6,
                    win_pct: None,
                },
            ],
            games: 100,
            core_games: 80,
            core_len: 3,
            core_win_pct: Some(53.0),
            stale: false,
            updated_at: None,
            skill_priority: None,
            skill_order: None,
        }
    }

    #[test]
    fn preset_item_set_keeps_the_requested_block_order_and_starter_stacks() {
        let (title, body) = preset_item_set_body(799, "Ambessa", &sample_build())
        .unwrap();
        assert!(title.starts_with("Swapper: Ambessa"));
        let blocks = body["blocks"].as_array().unwrap();
        let labels: Vec<&str> = blocks
            .iter()
            .map(|block| block["type"].as_str().unwrap())
            .collect();
        assert_eq!(
            labels,
            [
                "Starter Items",
                "Core + Popular Picks",
                "Alternatives",
            ]
        );
        assert_eq!(blocks[0]["items"][1]["id"], "2003");
        assert_eq!(blocks[0]["items"][1]["count"], 2);
        assert_eq!(blocks[1]["items"].as_array().unwrap().len(), 3);
        // One situational block, highest-support first, never one row per purpose.
        let ids: Vec<&str> = blocks[2]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["3156", "6695", "3071"]);
    }

    #[test]
    fn a_build_without_an_observed_core_is_not_called_one() {
        let mut build = sample_build();
        build.core_len = 0;
        let (_, body) = preset_item_set_body(799, "Ambessa", &build).unwrap();
        assert_eq!(body["blocks"][1]["type"], "Popular Picks");
    }

    #[test]
    fn a_build_without_alternatives_has_no_empty_block() {
        let mut build = sample_build();
        build.options.clear();
        let (_, body) = preset_item_set_body(799, "Ambessa", &build).unwrap();
        assert_eq!(body["blocks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn appending_keeps_the_players_sets_and_account() {
        let existing = json!({
            "accountId": 42,
            "timestamp": 0,
            "itemSets": [{ "title": "Mine", "uid": "a" }]
        });
        let added = json!({ "title": "Swapper: Ahri", "uid": "b" });

        let updated = with_item_set_appended(existing, added, 1_700_000_000_000);

        assert_eq!(updated["accountId"], 42);
        assert_eq!(updated["timestamp"], 1_700_000_000_000_i64);
        let titles: Vec<&str> = updated["itemSets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|set| set["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["Mine", "Swapper: Ahri"]);
    }

    #[test]
    fn the_title_is_only_the_champion_and_the_max_order_names_the_first_block() {
        let mut build = sample_build();
        build.skill_priority = Some("wqe".into());
        let (title, body) = preset_item_set_body(799, "Ambessa", &build).unwrap();
        assert_eq!(title, "Swapper: Ambessa");
        assert_eq!(body["blocks"][0]["type"], "Starter Items · Max W > Q > E");
        assert_eq!(body["blocks"][1]["type"], "Core + Popular Picks");
        // Without starters the label moves to the build block.
        build.starters.clear();
        let (_, body) = preset_item_set_body(799, "Ambessa", &build).unwrap();
        assert_eq!(body["blocks"][0]["type"], "Core + Popular Picks · Max W > Q > E");
        assert_eq!(build_label(None), None);
    }

    #[test]
    fn a_new_import_replaces_the_previous_swapper_set() {
        let existing = json!({
            "itemSets": [
                { "title": "Swapper: Ahri · Recommended build", "uid": "old" },
                { "title": "Mine", "uid": "a" }
            ]
        });
        let updated = with_item_set_appended(
            existing,
            json!({ "title": "Swapper: Lux · Recommended build", "uid": "new" }),
            1,
        );
        let uids: Vec<&str> = updated["itemSets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|set| set["uid"].as_str().unwrap())
            .collect();
        assert_eq!(uids, ["a", "new"]);
    }

    #[test]
    fn cleanup_removes_only_swapper_sets() {
        let existing = json!({
            "accountId": 42,
            "itemSets": [
                { "title": "Swapper: Ahri · Recommended build" },
                { "title": "My Swapper build" }
            ]
        });
        let cleaned = without_swapper_item_sets(existing, 5).expect("a Swapper set was removed");
        assert_eq!(cleaned["accountId"], 42);
        assert_eq!(cleaned["itemSets"].as_array().unwrap().len(), 1);
        assert_eq!(cleaned["itemSets"][0]["title"], "My Swapper build");
    }

    #[test]
    fn cleanup_without_swapper_sets_writes_nothing() {
        let existing = json!({ "itemSets": [{ "title": "Mine" }] });
        assert!(without_swapper_item_sets(existing, 5).is_none());
    }

    #[test]
    fn appending_to_an_account_without_sets_starts_the_list() {
        let updated = with_item_set_appended(json!({ "accountId": 7 }), json!({ "uid": "b" }), 1);
        assert_eq!(updated["itemSets"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn generic_pro_build_keeps_its_single_build_block() {
        let (_, body) = generic_item_set_body(799, "Ambessa", &[6692, 3047]).unwrap();
        let blocks = body["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "Final Items");
    }

    #[test]
    fn malformed_preset_build_is_rejected_before_import() {
        let mut build = sample_build();
        build.items.clear();
        assert!(
            preset_item_set_body(799, "Ambessa", &build).is_err()
        );
        assert!(generic_item_set_body(799, "Ambessa", &[0]).is_err());
    }

    #[test]
    fn options_are_limited_and_balanced_across_later_slots() {
        let mut build = sample_build();
        build.options = [4u8, 5, 6]
            .into_iter()
            .flat_map(|slot| {
                (0..4).map(move |index| ItemOptionView {
                    id: 4000 + slot as i64 * 10 + index,
                    name: format!("Other item {}", slot as i64 * 10 + index),
                    slot,
                    games: 100 - index as u64,
                    win_pct: None,
                })
            })
            .collect();
        let (_, body) = preset_item_set_body(799, "Ambessa", &build).unwrap();
        let blocks = body["blocks"].as_array().unwrap();
        let options = blocks
            .iter()
            .find(|block| block["type"] == "Alternatives")
            .unwrap()["items"]
            .as_array()
            .unwrap();
        let ids: Vec<&str> = options
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            ["4040", "4050", "4060", "4041", "4051", "4061", "4042"]
        );

        build.options.resize(MAX_OPTION_CANDIDATES + 1, build.options[0].clone());
        assert!(preset_item_set_body(799, "Ambessa", &build).is_err());
    }
}
