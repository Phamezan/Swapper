//! Ability cooldown cheat sheet: a champion's passive and Q/W/E/R with base
//! cooldowns, costs and ranges per rank, plus summoner spell base cooldowns.
//!
//! Base fields come from the client's local game data:
//! `/lol-game-data/assets/v1/champions/{id}.json` carries per-rank arrays
//! (`cooldownCoefficients`, `costCoefficients`, `range`, `ammo`) six entries
//! long whatever the real rank count, and `maxLevel` is always 0, so the rank
//! count comes from [`ranks`]. Fetched only when a sheet opens, then cached for
//! the session. Icons are separate requests ([`ability_icon`]) so the sheet
//! JSON stays small and the frontend icon cache can share them.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use serde::Serialize;
use serde_json::Value;

#[path = "ability_tooltips.rs"]
mod tooltips;

/// A range at or above this is the client's "global or self-cast" marker.
const GLOBAL_RANGE: f64 = 25_000.0;
/// Ranges below this are self-cast placeholders (Elise R reports 20).
const MIN_RANGE: f64 = 100.0;
/// A valid ability icon is a few kilobytes; anything larger is refused.
const MAX_ICON_BYTES: usize = 256 * 1024;

static CACHE: OnceLock<Mutex<HashMap<i64, Abilities>>> = OnceLock::new();

fn cache() -> std::sync::MutexGuard<'static, HashMap<i64, Abilities>> {
    CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Abilities {
    pub champion_id: i64,
    pub passive: Passive,
    pub spells: Vec<Ability>,
    /// Filled per request from the summoner-spell catalog, never cached here.
    pub summoners: Vec<Summoner>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Passive {
    pub name: String,
    pub icon_path: Option<String>,
    pub description: String,
    pub tooltip: Option<String>,
}

/// One of Q/W/E/R. Every list holds one value per rank; `costs` and `ranges`
/// are empty when the ability has no plain cost or no meaningful range.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ability {
    pub key: String,
    pub name: String,
    pub icon_path: Option<String>,
    /// The game data's short plain description (markup stripped).
    pub description: String,
    /// Numeric CommunityDragon text, absent when enrichment is unavailable.
    pub tooltip: Option<String>,
    pub cooldowns: Vec<f64>,
    pub costs: Vec<f64>,
    pub ranges: Vec<f64>,
    /// Ammo abilities (Jhin E, Teemo R…): charges and the recharge per rank.
    pub charges: u32,
    pub recharge: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summoner {
    pub id: i64,
    pub name: String,
    pub cooldown: f64,
}

/// How many ranks an ability has. The game data does not say, so the usual
/// 5 (basic) / 3 (ultimate) is overridden for the known exceptions.
// ponytail: hand-kept exception list; revisit if a new champion gets odd ranks.
fn ranks(champion_id: i64, key: &str) -> usize {
    match (champion_id, key) {
        // Udyr: every stance has six ranks.
        (77, _) => 6,
        // Jayce, Elise, Nidalee and Karma: R has a rank from level 1.
        (126 | 60 | 76 | 43, "R") => 4,
        (_, "R") => 3,
        _ => 5,
    }
}

fn icon_path(value: &Value) -> Option<String> {
    let path = value["abilityIconPath"].as_str()?;
    valid_asset(path).then(|| path.to_string())
}

/// Only plain game-data asset paths may be fetched from the LCU.
fn valid_asset(path: &str) -> bool {
    path.starts_with("/lol-game-data/assets/") && !path.contains("..") && !path.contains('\\')
}

/// The game data's description as plain text: tags dropped (`<br>` becomes a space),
/// whitespace collapsed, capped so a malformed file cannot flood a tooltip.
fn plain(value: &Value) -> String {
    const MAX_CHARS: usize = 400;
    let raw = value.as_str().unwrap_or("").replace("<br>", " ");
    let mut text = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(MAX_CHARS) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text,
    }
}

fn numbers(value: &Value, count: usize) -> Vec<f64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .take(count)
        .map(|n| n.as_f64().unwrap_or(0.0))
        .collect()
}

fn ability(champion_id: i64, key: &str, spell: &Value) -> Ability {
    let count = ranks(champion_id, key);
    // Only "@Cost@ …" templates are a plain per-cast cost; "No Cost", energy
    // refunds and per-second drains use other templates.
    let costs = if spell["cost"].as_str().unwrap_or("").contains("@Cost@") {
        numbers(&spell["costCoefficients"], count)
    } else {
        Vec::new()
    };
    let costs = if costs.iter().all(|cost| *cost <= 0.0) {
        Vec::new()
    } else {
        costs
    };
    let ranges = numbers(&spell["range"], count);
    let ranges = if ranges
        .iter()
        .all(|range| (MIN_RANGE..GLOBAL_RANGE).contains(range))
    {
        ranges
    } else {
        Vec::new()
    };
    let charges = spell["ammo"]["maxAmmo"]
        .as_array()
        .and_then(|ammo| ammo.first())
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    Ability {
        key: key.to_string(),
        name: spell["name"].as_str().unwrap_or("").to_string(),
        icon_path: icon_path(spell),
        description: plain(&spell["description"]),
        tooltip: None,
        cooldowns: numbers(&spell["cooldownCoefficients"], count),
        costs,
        ranges,
        charges,
        recharge: if charges > 0 {
            numbers(&spell["ammo"]["ammoRechargeTime"], count)
        } else {
            Vec::new()
        },
    }
}

/// Builds the sheet from a champion game-data file.
pub fn parse(champion_id: i64, champion: &Value) -> Abilities {
    let spells = champion["spells"]
        .as_array()
        .into_iter()
        .flatten()
        .take(4)
        .zip(["Q", "W", "E", "R"])
        .map(|(spell, key)| ability(champion_id, key, spell))
        .collect();
    Abilities {
        champion_id,
        passive: Passive {
            name: champion["passive"]["name"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            icon_path: icon_path(&champion["passive"]),
            description: plain(&champion["passive"]["description"]),
            tooltip: None,
        },
        spells,
        summoners: Vec::new(),
    }
}

/// The requested summoner spells with their base cooldowns, in request order.
pub fn summoners(catalog: &[crate::runes::spells::Spell], ids: &[i64]) -> Vec<Summoner> {
    ids.iter()
        .filter_map(|id| catalog.iter().find(|spell| spell.id == *id))
        .map(|spell| Summoner {
            id: spell.id,
            name: spell.name.clone(),
            cooldown: spell.cooldown,
        })
        .collect()
}

async fn load(champion_id: i64) -> Result<Abilities, String> {
    if let Some(found) = cache().get(&champion_id) {
        return Ok(found.clone());
    }
    let champion: Value = crate::runes::game_data::json(&format!(
        "/lol-game-data/assets/v1/champions/{champion_id}.json"
    ))
    .await
    .map_err(|e| e.to_string())?;
    let mut parsed = parse(champion_id, &champion);
    tooltips::enrich(&mut parsed, &champion).await;
    if parsed.spells.len() == 4 {
        cache().insert(champion_id, parsed.clone());
    }
    Ok(parsed)
}

/// The cheat sheet for one champion, with `spell_ids`' base cooldowns when given.
#[tauri::command]
pub async fn champion_abilities(
    champion_id: i64,
    spell_ids: Option<Vec<i64>>,
) -> Result<Abilities, String> {
    if champion_id <= 0 {
        return Err("Unknown champion.".into());
    }
    let mut sheet = load(champion_id).await?;
    let ids = spell_ids.unwrap_or_default();
    if !ids.is_empty() {
        // Spells are a bonus: a failed catalog read still shows the abilities.
        if let Ok(catalog) = crate::runes::spells::catalog().await {
            sheet.summoners = summoners(&catalog, &ids);
        }
    }
    Ok(sheet)
}

/// An ability or passive icon as a data URL. The frontend caches it per path.
#[tauri::command]
pub async fn ability_icon(path: String) -> Result<String, String> {
    if !valid_asset(&path) {
        return Err("Not a game-data icon.".into());
    }
    let bytes = crate::runes::game_data::bytes(&path)
        .await
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_ICON_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Unexpected icon data.".into());
    }
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn descriptions_are_plain_capped_text() {
        let raw = serde_json::json!("After killing 9 minions, Ahri heals.<br>After a <b>takedown</b>   she heals more.");
        assert_eq!(super::plain(&raw), "After killing 9 minions, Ahri heals. After a takedown she heals more.");
        assert_eq!(super::plain(&serde_json::json!(null)), "");
        let long = super::plain(&serde_json::json!("x ".repeat(500)));
        assert!(long.chars().count() <= 401 && long.ends_with('…'));
    }

    use super::*;

    /// Trimmed real payloads (Ahri, Jhin, Zed, Jayce, Akali, Aurelion Sol, Aatrox, Elise)
    /// from `/lol-game-data/assets/v1/champions/{id}.json`; text fields cut.
    const FIXTURE: &str = include_str!("../../tests/fixtures/champion-abilities.json");

    fn champion(id: i64) -> Abilities {
        let all: Value = serde_json::from_str(FIXTURE).expect("fixture should parse");
        parse(id, &all[id.to_string()])
    }

    #[test]
    fn reads_passive_and_four_abilities_with_icons() {
        let ahri = champion(103);
        assert_eq!(ahri.passive.name, "Essence Theft");
        assert!(ahri
            .passive
            .icon_path
            .as_deref()
            .unwrap()
            .ends_with("Icons_Ahri_Passive.png"));
        let keys: Vec<_> = ahri.spells.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["Q", "W", "E", "R"]);
        assert!(ahri.spells.iter().all(|s| s.icon_path.is_some()));
        assert_eq!(ahri.spells[0].name, "Orb of Deception");
    }

    #[test]
    fn cooldowns_cut_to_five_basic_ranks_and_three_ultimate_ranks() {
        let ahri = champion(103);
        assert_eq!(ahri.spells[1].cooldowns, [9.0, 8.0, 7.0, 6.0, 5.0]);
        assert_eq!(ahri.spells[3].cooldowns, [140.0, 120.0, 100.0]);
        let zed = champion(238);
        assert_eq!(zed.spells[2].cooldowns, [5.0, 4.5, 4.0, 3.5, 3.0]);
        // The trailing zeros in Jhin's R array are not ranks.
        assert_eq!(champion(202).spells[3].cooldowns, [120.0, 105.0, 90.0]);
    }

    #[test]
    fn form_swappers_get_a_fourth_ultimate_rank() {
        let jayce = champion(126);
        assert_eq!(jayce.spells[3].cooldowns.len(), 4);
        assert_eq!(jayce.spells[0].cooldowns, [16.0, 14.0, 12.0, 10.0, 8.0]);
        assert_eq!(ranks(77, "Q"), 6);
        assert_eq!(ranks(1, "R"), 3);
    }

    #[test]
    fn costs_only_for_plain_per_cast_costs() {
        assert_eq!(
            champion(103).spells[0].costs,
            [55.0, 65.0, 75.0, 85.0, 95.0]
        );
        // "No Cost" ultimates, Akali's energy refund and Aurelion Sol's per-second
        // drain have no plain cost.
        assert!(champion(238).spells[3].costs.is_empty());
        assert!(champion(84).spells[1].costs.is_empty());
        assert!(champion(136).spells[0].costs.is_empty());
        // Aatrox spends nothing at all.
        assert!(champion(266).spells.iter().all(|s| s.costs.is_empty()));
    }

    #[test]
    fn ranges_drop_global_and_self_cast_markers() {
        assert_eq!(champion(103).spells[2].ranges, [975.0; 5]);
        // Aatrox Q, E and R report 25000 (no real range).
        let aatrox = champion(266);
        assert!(aatrox.spells[0].ranges.is_empty());
        assert_eq!(aatrox.spells[1].ranges, [825.0; 5]);
        // Elise R reports a 20-unit placeholder; her other ranges are real.
        let elise = champion(60);
        assert!(elise.spells[3].ranges.is_empty());
        assert_eq!(elise.spells[2].ranges, [1075.0; 5]);
    }

    #[test]
    fn ammo_abilities_carry_charges_and_recharge() {
        let jhin_e = &champion(202).spells[2];
        assert_eq!(jhin_e.charges, 2);
        assert_eq!(jhin_e.recharge, [24.0, 21.5, 19.0, 16.5, 14.0]);
        let ahri_q = &champion(103).spells[0];
        assert_eq!(ahri_q.charges, 0);
        assert!(ahri_q.recharge.is_empty());
    }

    #[test]
    fn malformed_payloads_degrade_to_empty_fields() {
        let empty = parse(1, &serde_json::json!({}));
        assert!(empty.spells.is_empty());
        assert_eq!(empty.passive.name, "");
        let odd = parse(
            1,
            &serde_json::json!({ "spells": [{ "abilityIconPath": "https://evil/q.png", "cooldownCoefficients": [3] }] }),
        );
        assert_eq!(odd.spells[0].icon_path, None);
        assert_eq!(odd.spells[0].cooldowns, [3.0]);
    }

    #[test]
    fn icon_paths_stay_inside_game_data() {
        assert!(valid_asset("/lol-game-data/assets/ASSETS/q.png"));
        assert!(!valid_asset("/lol-game-data/assets/../secret"));
        assert!(!valid_asset("/lol-game-data/assets/\\secret"));
        assert!(!valid_asset("/lol-summoner/v1/current-summoner"));
    }

    #[test]
    fn summoners_keep_request_order_and_skip_unknown_ids() {
        let catalog = crate::runes::spells::parse(
            r#"[{"id":4,"name":"Flash","cooldown":300},{"id":14,"name":"Ignite","cooldown":180}]"#,
        )
        .unwrap();
        let picked = summoners(&catalog, &[14, 999, 4]);
        assert_eq!(
            picked,
            [
                Summoner {
                    id: 14,
                    name: "Ignite".into(),
                    cooldown: 180.0
                },
                Summoner {
                    id: 4,
                    name: "Flash".into(),
                    cooldown: 300.0
                },
            ]
        );
    }
}
