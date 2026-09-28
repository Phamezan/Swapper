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

/// Short reason labels for item effects that have a clear in-game use.
/// These labels organize the static options block; they do not claim to react
/// to the enemy draft.
pub fn option_reason(name: &str) -> Option<&'static str> {
    let normalized: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    match normalized.as_str() {
        "mawofmalmortius" | "forceofnature" | "spiritvisage" | "kaenicrookern" | "witsend" => {
            Some("Options · Against AP")
        }
        "serpentsfang" => Some("Options · Against shields"),
        "blackcleaver" | "lorddominiksregards" | "seryldasgrudge" => {
            Some("Options · Against armor")
        }
        "mortalreminder" | "chempunkchainsword" | "thornmail" => Some("Options · Against healing"),
        "hubris" | "youmuusghostblade" | "opportunity" => Some("Options · Damage pivot"),
        "steraksgage" | "guardianangel" | "deathsdance" | "randuinsomen" => {
            Some("Options · More durability")
        }
        _ => None,
    }
}

fn known_reason(label: &str) -> Option<&'static str> {
    match label {
        "Options · Against AP" => Some("Options · Against AP"),
        "Options · Against shields" => Some("Options · Against shields"),
        "Options · Against armor" => Some("Options · Against armor"),
        "Options · Against healing" => Some("Options · Against healing"),
        "Options · Damage pivot" => Some("Options · Damage pivot"),
        "Options · More durability" => Some("Options · More durability"),
        _ => None,
    }
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
    source: &str,
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
    let source = clean(source, 24);
    if source.is_empty() {
        return Err(RuneError::conflict("The item build has no name."));
    }
    let champion_name = clean(champion_name, 28);
    let champion_name = if champion_name.is_empty() {
        "Champion"
    } else {
        champion_name.as_str()
    };
    let title = clean(
        &format!("Swapper: {champion_name} · {source}"),
        MAX_TITLE_CHARS,
    );
    let body = json!({
        "itemSet": {
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
        }
    });
    Ok((title, body))
}

fn generic_item_set_body(
    champion_id: i64,
    champion_name: &str,
    source: &str,
    item_ids: &[i64],
) -> Result<(String, Value), RuneError> {
    let items = item_values(item_ids)?;
    item_set_body(
        champion_id,
        champion_name,
        source,
        vec![block("Build", items)],
    )
}

fn preset_item_set_body(
    champion_id: i64,
    champion_name: &str,
    source: &str,
    build: &KeystoneBuildView,
    option_names: &std::collections::HashMap<i64, String>,
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

    let mut blocks = Vec::with_capacity(3);
    if !build.starters.is_empty() {
        let items = build
            .starters
            .iter()
            .map(|item| json!({ "id": item.id.to_string(), "count": item.count }))
            .collect();
        blocks.push(block("Starter Items", items));
    }
    blocks.push(block(
        "Full Build",
        build
            .items
            .iter()
            .map(|item| json!({ "id": item.id.to_string(), "count": 1 }))
            .collect(),
    ));

    // LoLalytics reports independent later-slot marginals. Round-robin their
    // highest-support alternatives so one slot cannot crowd out the other
    // two from the in-game Options blocks.
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
    let mut options: Vec<(i64, String)> = Vec::new();
    let mut used = Vec::new();
    let max_rounds = by_slot.iter().map(Vec::len).max().unwrap_or(0);
    for index in 0..max_rounds {
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
            let name = option_names
                .get(&option.id)
                .map(String::as_str)
                .unwrap_or(option.name.as_str());
            let label = option_reason(name)
                .or_else(|| option.reason.as_deref().and_then(known_reason))
                .unwrap_or("Options")
                .to_string();
            used.push(option.id);
            options.push((option.id, label));
            if options.len() == MAX_OPTIONS {
                break;
            }
        }
        if options.len() == MAX_OPTIONS {
            break;
        }
    }

    // Keep the standard Options block first, then named matchup/effect groups.
    let mut grouped: Vec<(String, Vec<Value>)> = Vec::new();
    for (id, label) in options {
        if let Some((_, items)) = grouped
            .iter_mut()
            .find(|(group, _)| group.as_str() == label)
        {
            items.push(json!({ "id": id.to_string(), "count": 1 }));
        } else {
            grouped.push((label, vec![json!({ "id": id.to_string(), "count": 1 })]));
        }
    }
    grouped.sort_by_key(|(label, _)| (label != "Options", label.clone()));
    blocks.extend(
        grouped
            .into_iter()
            .map(|(label, items)| block(&label, items)),
    );

    item_set_body(champion_id, champion_name, source, blocks)
}

async fn post_item_set(title: String, body: Value) -> Result<String, RuneError> {
    let lcu = super::lcu().await?;
    let summoner: CurrentSummoner = super::lcu_get(&lcu, CURRENT_SUMMONER_PATH).await?;
    if summoner.summoner_id <= 0 {
        return Err(RuneError::unavailable(
            "League did not report the current summoner id.",
        ));
    }
    let path = format!("{ITEM_SETS_PATH}/{}/sets", summoner.summoner_id);
    let response = lcu.send(Method::POST, &path, Some(&body)).await?;
    if response.status().is_success() {
        return Ok(title);
    }
    Err(RuneError::conflict(format!(
        "League rejected the item set (HTTP {}).",
        response.status().as_u16()
    )))
}

/// Adds one item set without replacing any item sets the player already owns.
pub async fn import_build(
    champion_id: i64,
    champion_name: &str,
    source: &str,
    item_ids: &[i64],
) -> Result<String, RuneError> {
    let (title, body) = generic_item_set_body(champion_id, champion_name, source, item_ids)?;
    post_item_set(title, body).await
}

/// Imports a preset build with separate starter, full-build and option blocks.
pub async fn import_keystone_build(
    champion_id: i64,
    champion_name: &str,
    source: &str,
    build: &KeystoneBuildView,
) -> Result<String, RuneError> {
    let names = super::items::names().await;
    let (title, body) = preset_item_set_body(champion_id, champion_name, source, build, &names)?;
    post_item_set(title, body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runes::view::{ItemOptionView, ItemStackView, ItemView};

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
                    reason: None,
                },
                ItemOptionView {
                    id: 6695,
                    name: "Serpent's Fang".into(),
                    slot: 5,
                    games: 7,
                    win_pct: None,
                    reason: None,
                },
                ItemOptionView {
                    id: 3071,
                    name: "Black Cleaver".into(),
                    slot: 5,
                    games: 6,
                    win_pct: None,
                    reason: None,
                },
            ],
            games: 100,
            core_games: 80,
            core_win_pct: Some(53.0),
        }
    }

    #[test]
    fn preset_item_set_keeps_the_requested_block_order_and_starter_stacks() {
        let (title, body) = preset_item_set_body(
            799,
            "Ambessa",
            "Preset · Grasp",
            &sample_build(),
            &Default::default(),
        )
        .unwrap();
        assert!(title.starts_with("Swapper: Ambessa"));
        let blocks = body["itemSet"]["blocks"].as_array().unwrap();
        let labels: Vec<&str> = blocks
            .iter()
            .map(|block| block["type"].as_str().unwrap())
            .collect();
        assert_eq!(
            labels,
            [
                "Starter Items",
                "Full Build",
                "Options · Against AP",
                "Options · Against armor",
                "Options · Against shields",
            ]
        );
        assert_eq!(blocks[0]["items"][1]["id"], "2003");
        assert_eq!(blocks[0]["items"][1]["count"], 2);
        assert_eq!(blocks[1]["items"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn generic_pro_build_keeps_its_single_build_block() {
        let (_, body) = generic_item_set_body(799, "Ambessa", "Pro", &[6692, 3047]).unwrap();
        let blocks = body["itemSet"]["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "Build");
    }

    #[test]
    fn malformed_preset_build_is_rejected_before_import() {
        let mut build = sample_build();
        build.items.clear();
        assert!(
            preset_item_set_body(799, "Ambessa", "Preset", &build, &Default::default()).is_err()
        );
        assert!(generic_item_set_body(799, "Ambessa", "Pro", &[0]).is_err());
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
                    reason: None,
                })
            })
            .collect();
        let (_, body) = preset_item_set_body(
            799,
            "Ambessa",
            "Preset",
            &build,
            &Default::default(),
        )
        .unwrap();
        let blocks = body["itemSet"]["blocks"].as_array().unwrap();
        let options = blocks
            .iter()
            .find(|block| block["type"] == "Options")
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
        assert!(preset_item_set_body(799, "Ambessa", "Preset", &build, &Default::default()).is_err());
    }

    #[test]
    fn item_reasons_are_effect_labels_not_archetype_guesses() {
        assert_eq!(
            option_reason("Maw of Malmortius"),
            Some("Options · Against AP")
        );
        assert_eq!(
            option_reason("Serpent's Fang"),
            Some("Options · Against shields")
        );
        assert_eq!(option_reason("Hubris"), Some("Options · Damage pivot"));
        assert_eq!(option_reason("Unknown item"), None);
    }
}
