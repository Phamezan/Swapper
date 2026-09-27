//! Summoner spells: the game-data catalog, mode filtering, icons, and the
//! champion-select write.
//!
//! The list comes from the client's own `/lol-game-data/assets/v1/summoner-spells.json`
//! and is filtered the way the client does: a spell is offered only in the game
//! modes it lists, and only once the player's level unlocks it. The write is the
//! `PATCH /lol-champ-select/v1/session/my-selection` call used by LeagueAkari
//! (MIT, `src/shared/http-api-axios-helper/league-client/champ-select.ts`).

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use reqwest::Method;
use serde::Deserialize;

use super::RuneError;

/// Flash's spell id, used to keep it on the key the player already uses.
pub const SPELL_FLASH: i64 = 4;

const SPELLS_PATH: &str = "/lol-game-data/assets/v1/summoner-spells.json";
const MY_SELECTION_PATH: &str = "/lol-champ-select/v1/session/my-selection";
const SUMMONER_PATH: &str = "/lol-summoner/v1/current-summoner";

/// Serializes summoner-spell catalog loads, and one gate per spell id so
/// concurrent icon requests share a fetch.
static CATALOG_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static SPELL_FLIGHTS: OnceLock<super::Flights<i64>> = OnceLock::new();

fn catalog_lock() -> &'static tokio::sync::Mutex<()> {
    CATALOG_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn icon_gate(id: i64) -> Arc<tokio::sync::Mutex<()>> {
    SPELL_FLIGHTS.get_or_init(super::Flights::new).gate(&id)
}

/// One summoner spell from the client's game data.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spell {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub game_modes: Vec<String>,
    #[serde(default)]
    pub summoner_level: i64,
    #[serde(default)]
    pub icon_path: String,
}

impl Spell {
    /// Modern playable spells only. The file still carries legacy duplicates
    /// (ids 71+) and a nameless placeholder that the client never offers.
    pub fn is_playable(&self) -> bool {
        self.id > 0 && self.id < 100 && !self.name.trim().is_empty()
    }
}

/// Parses the summoner-spells game-data file.
pub fn parse(body: &str) -> Result<Vec<Spell>, RuneError> {
    serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read summoner spell data: {e}")))
}

/// The spells the picker offers for a game mode, as the client does: each is
/// listed for the mode and unlocked at the player's level.
///
/// A blank mode means CLASSIC. A mode with no spells of its own (for example
/// Arena's CHERRY) falls back to the CLASSIC set, so the picker is never empty.
/// `player_level` of `0` or less skips the level check.
pub fn available(spells: &[Spell], game_mode: &str, player_level: i64) -> Vec<Spell> {
    let mode = if game_mode.trim().is_empty() {
        "CLASSIC"
    } else {
        game_mode.trim()
    };
    let filter = |mode: &str| -> Vec<Spell> {
        spells
            .iter()
            .filter(|spell| {
                spell.is_playable()
                    && spell
                        .game_modes
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(mode))
                    && (player_level <= 0 || spell.summoner_level <= player_level)
            })
            .cloned()
            .collect()
    };
    let list = filter(mode);
    if list.is_empty() && !mode.eq_ignore_ascii_case("CLASSIC") {
        filter("CLASSIC")
    } else {
        list
    }
}

/// The pair to apply, keeping the existing layout where possible.
///
/// When the recommended pair contains Flash and the player already has Flash on
/// a key, Flash stays on that key. Otherwise, if the recommended pair is the
/// reverse of the player's current pair, it is flipped so their order survives.
pub fn keep_flash_side(recommended: [i64; 2], current: [i64; 2]) -> [i64; 2] {
    let current_flash = current.iter().position(|id| *id == SPELL_FLASH);
    let recommended_flash = recommended.iter().position(|id| *id == SPELL_FLASH);
    if let (Some(target), Some(have)) = (current_flash, recommended_flash) {
        if target != have {
            return [recommended[1], recommended[0]];
        }
        return recommended;
    }
    if recommended[0] == current[1] || recommended[1] == current[0] {
        [recommended[1], recommended[0]]
    } else {
        recommended
    }
}

/// The new pair after picking `spell` for `slot` (`0` is D, `1` is F). Picking
/// the spell already in the other slot swaps them, like the client.
pub fn pick_pair(current: [i64; 2], slot: usize, spell: i64) -> [i64; 2] {
    let slot = slot.min(1);
    let other = 1 - slot;
    let mut next = current;
    if spell != 0 && current[other] == spell {
        next[slot] = spell;
        next[other] = current[slot];
    } else {
        next[slot] = spell;
    }
    next
}

/// Body accepted by `PATCH /lol-champ-select/v1/session/my-selection`.
pub fn my_selection_body(pair: [i64; 2]) -> serde_json::Value {
    serde_json::json!({ "spell1Id": pair[0], "spell2Id": pair[1] })
}

/// Turns a two-entry id list into a pair, ignoring malformed or missing ids.
pub fn pair_from_ids(ids: &[i64]) -> Option<[i64; 2]> {
    if ids.len() >= 2 && ids[0] > 0 && ids[1] > 0 {
        Some([ids[0], ids[1]])
    } else {
        None
    }
}

/// The summoner-spell catalog, cached for the session. Concurrent misses wait
/// on one fetch.
pub async fn catalog() -> Result<Vec<Spell>, RuneError> {
    if let Some(spells) = cached_spells() {
        return Ok(spells);
    }
    let _guard = catalog_lock().lock().await;
    if let Some(spells) = cached_spells() {
        return Ok(spells);
    }
    let lcu = super::lcu().await?;
    let text = super::lcu_get_text(&lcu, SPELLS_PATH).await?;
    let spells = parse(&text)?;
    super::shared().spells = Some((Instant::now(), spells.clone()));
    Ok(spells)
}

fn cached_spells() -> Option<Vec<Spell>> {
    super::shared()
        .spells
        .as_ref()
        .filter(|(at, _)| at.elapsed() < super::CATALOG_TTL)
        .map(|(_, spells)| spells.clone())
}

/// The local player's level, or `0` when it cannot be read (which skips the
/// level filter rather than hiding every spell).
pub async fn player_level() -> i64 {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Summoner {
        #[serde(default)]
        summoner_level: i64,
    }
    let Ok(lcu) = super::lcu().await else {
        return 0;
    };
    super::lcu_get::<Summoner>(&lcu, SUMMONER_PATH)
        .await
        .map(|summoner| summoner.summoner_level)
        .unwrap_or(0)
}

/// The PNG bytes for a summoner spell icon. Concurrent requests for the same id
/// share one fetch.
pub async fn icon(id: i64) -> Result<Vec<u8>, RuneError> {
    if let Some(bytes) = super::shared().spell_icons.get(&id) {
        return Ok(bytes.clone());
    }
    let gate = icon_gate(id);
    let _guard = gate.lock().await;
    if let Some(bytes) = super::shared().spell_icons.get(&id) {
        return Ok(bytes.clone());
    }
    let spells = catalog().await?;
    let path = spells
        .iter()
        .find(|spell| spell.id == id)
        .map(|spell| spell.icon_path.clone())
        .filter(|path| path.starts_with("/lol-game-data/assets/") && !path.contains(".."))
        .ok_or_else(|| RuneError::not_found("Unknown summoner spell."))?;
    let lcu = super::lcu().await?;
    let bytes = super::lcu_get_bytes(&lcu, &path).await?;
    super::shared().spell_icons.insert(id, bytes.clone());
    Ok(bytes)
}

/// The local player's current spell pair, `[D, F]`.
pub async fn current_pair() -> Result<[i64; 2], RuneError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct MySelection {
        #[serde(default)]
        spell1_id: i64,
        #[serde(default)]
        spell2_id: i64,
    }
    let lcu = super::lcu().await?;
    let selection: MySelection = super::lcu_get(&lcu, MY_SELECTION_PATH).await?;
    Ok([selection.spell1_id, selection.spell2_id])
}

/// Rejects a pair that League would refuse, before any network call.
pub fn validate_pair(pair: [i64; 2]) -> Result<(), RuneError> {
    if pair[0] <= 0 || pair[1] <= 0 {
        return Err(RuneError::conflict("Choose two summoner spells."));
    }
    if pair[0] == pair[1] {
        return Err(RuneError::conflict("Choose two different summoner spells."));
    }
    Ok(())
}

/// Writes a spell pair to champion select.
pub async fn apply_pair(pair: [i64; 2]) -> Result<(), RuneError> {
    validate_pair(pair)?;
    let known = catalog().await?;
    let valid = |id: i64| known.iter().any(|spell| spell.id == id && spell.is_playable());
    if !valid(pair[0]) || !valid(pair[1]) {
        return Err(RuneError::conflict("That summoner spell is not available."));
    }
    let lcu = super::lcu().await?;
    let body = my_selection_body(pair);
    let response = lcu
        .send(Method::PATCH, MY_SELECTION_PATH, Some(&body))
        .await?;
    if !response.status().is_success() {
        return Err(RuneError::conflict(format!(
            "League rejected the summoner spells (HTTP {}).",
            response.status().as_u16()
        )));
    }
    Ok(())
}

/// Applies one spell to a slot, swapping when it is already in the other slot.
pub async fn apply_pick(slot: usize, spell_id: i64) -> Result<(), RuneError> {
    let current = current_pair().await?;
    apply_pair(pick_pair(current, slot, spell_id)).await
}

/// Applies a recommended pair, keeping Flash on the player's usual key.
pub async fn apply_recommended(recommended: [i64; 2]) -> Result<(), RuneError> {
    let current = current_pair().await.unwrap_or([0, 0]);
    apply_pair(keep_flash_side(recommended, current)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/summoner-spells.json");

    fn spells() -> Vec<Spell> {
        parse(FIXTURE).expect("fixture should parse")
    }

    fn ids(list: &[Spell]) -> Vec<i64> {
        list.iter().map(|spell| spell.id).collect()
    }

    #[test]
    fn parses_the_game_data_file_and_drops_legacy_entries() {
        let all = spells();
        assert!(all.len() > 10);
        // Legacy duplicates and the nameless placeholder are in the file.
        assert!(all.iter().any(|spell| spell.id == 71));
        assert!(all.iter().any(|spell| spell.id == 5 && spell.name.is_empty()));
    }

    #[test]
    fn filters_spells_by_mode_and_player_level() {
        let list = available(&spells(), "CLASSIC", 30);
        let classic = ids(&list);
        assert!(classic.contains(&4), "Flash should be offered in CLASSIC");
        assert!(classic.contains(&11), "Smite should be offered in CLASSIC");
        assert!(!classic.contains(&13), "Clarity is ARAM-only");
        assert!(!classic.contains(&71), "legacy Cleanse must never appear");
        assert!(!classic.contains(&5), "the nameless placeholder must never appear");
    }

    #[test]
    fn aram_offers_clarity_and_mark_but_not_smite() {
        let list = available(&spells(), "ARAM", 30);
        let aram = ids(&list);
        assert!(aram.contains(&13), "Clarity belongs in ARAM");
        assert!(aram.contains(&32), "Mark belongs in ARAM");
        assert!(!aram.contains(&11), "Smite must not appear in ARAM");
        assert!(aram.contains(&4));
    }

    #[test]
    fn a_mode_with_no_spells_falls_back_to_classic() {
        // Arena's CHERRY mode lists no spells in game data.
        let list = available(&spells(), "CHERRY", 30);
        assert!(ids(&list).contains(&4));
        assert!(ids(&list).contains(&14));
    }

    #[test]
    fn the_level_filter_hides_locked_spells_and_a_blank_level_skips_it() {
        // Cleanse (level 9) and Flash (level 7) are locked for a level-3 player.
        let low = ids(&available(&spells(), "CLASSIC", 3));
        assert!(!low.contains(&1), "Cleanse is locked below level 9");
        assert!(!low.contains(&4), "Flash is locked below level 7");
        assert!(low.contains(&6), "Ghost is unlocked at level 1");
        assert!(low.contains(&11), "Smite is unlocked at level 3");
        // An unknown level (0) skips the check entirely.
        let unknown = ids(&available(&spells(), "CLASSIC", 0));
        assert!(unknown.contains(&1));
        assert!(unknown.contains(&4));
    }

    #[test]
    fn keeps_flash_on_the_key_the_player_already_uses() {
        // Player has Flash on D (spell1); recommendation puts it on F.
        assert_eq!(keep_flash_side([14, 4], [4, 12]), [4, 14]);
        // Player has Flash on F (spell2); recommendation puts it on D.
        assert_eq!(keep_flash_side([4, 14], [12, 4]), [14, 4]);
        // Already aligned: nothing changes.
        assert_eq!(keep_flash_side([4, 14], [4, 12]), [4, 14]);
    }

    #[test]
    fn keeps_the_players_order_when_the_recommendation_is_reversed() {
        // No Flash in either pair, and the recommendation mirrors the player.
        assert_eq!(keep_flash_side([12, 14], [14, 12]), [14, 12]);
        // A fresh pair keeps the recommended order.
        assert_eq!(keep_flash_side([12, 14], [4, 11]), [12, 14]);
    }

    #[test]
    fn picking_a_spell_already_in_the_other_slot_swaps_them() {
        // Picking Ignite (14) for D while it sits on F swaps the pair.
        assert_eq!(pick_pair([4, 14], 0, 14), [14, 4]);
        // Picking a brand-new spell just fills the slot.
        assert_eq!(pick_pair([4, 14], 0, 12), [12, 14]);
        // Picking the spell already in this slot is a no-op.
        assert_eq!(pick_pair([4, 14], 0, 4), [4, 14]);
    }

    #[test]
    fn builds_the_my_selection_payload() {
        assert_eq!(
            my_selection_body([4, 14]),
            serde_json::json!({ "spell1Id": 4, "spell2Id": 14 })
        );
    }

    #[test]
    fn turns_a_two_id_list_into_a_pair() {
        assert_eq!(pair_from_ids(&[4, 14]), Some([4, 14]));
        assert_eq!(pair_from_ids(&[4]), None);
        assert_eq!(pair_from_ids(&[]), None);
        assert_eq!(pair_from_ids(&[0, 14]), None);
        // Extra ids beyond the pair are ignored rather than rejected.
        assert_eq!(pair_from_ids(&[4, 14, 12]), Some([4, 14]));
    }

    #[test]
    fn rejects_a_bad_pair_before_reaching_league() {
        assert!(validate_pair([0, 14]).is_err());
        assert!(validate_pair([14, 0]).is_err());
        assert!(validate_pair([4, 4]).is_err());
        assert!(validate_pair([4, 14]).is_ok());
    }
}
