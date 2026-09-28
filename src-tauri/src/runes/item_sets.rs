//! Creates champion-specific item sets in the League Client's in-game shop.

use reqwest::Method;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::RuneError;

const CURRENT_SUMMONER_PATH: &str = "/lol-summoner/v1/current-summoner";
const ITEM_SETS_PATH: &str = "/lol-item-sets/v1/item-sets";
const MAX_ITEMS: usize = 7;
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

fn item_set_body(
    champion_id: i64,
    champion_name: &str,
    source: &str,
    item_ids: &[i64],
) -> Result<(String, serde_json::Value), RuneError> {
    if !(1..=10_000).contains(&champion_id) {
        return Err(RuneError::conflict("Choose a valid champion before importing items."));
    }
    if item_ids.is_empty() || item_ids.len() > MAX_ITEMS || item_ids.iter().any(|id| *id <= 0) {
        return Err(RuneError::conflict("This build does not contain a valid item list."));
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
    let items = item_ids
        .iter()
        .map(|id| json!({ "id": id.to_string(), "count": 1 }))
        .collect::<Vec<_>>();
    let body = json!({
        "itemSet": {
            "associatedChampions": [champion_id],
            "associatedMaps": [],
            "blocks": [{
                "hideIfSummonerSpell": "",
                "items": items,
                "showIfSummonerSpell": "",
                "type": "Build"
            }],
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

/// Adds one item set without replacing any item sets the player already owns.
pub async fn import_build(
    champion_id: i64,
    champion_name: &str,
    source: &str,
    item_ids: &[i64],
) -> Result<String, RuneError> {
    let (title, body) = item_set_body(champion_id, champion_name, source, item_ids)?;
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
