//! Reads the current champion select so a rune page can be chosen. This mirrors
//! the LCU session shape already used by the phone pick/ban controls, but keeps
//! only what runes need so the two modules stay independent.

use std::collections::HashMap;

use serde::Deserialize;

use super::opgg::{MODE_ARAM, MODE_ARENA, MODE_RANKED, POSITION_NONE};
use super::RuneError;

pub const MAP_ARAM: i64 = 12;
pub const MAP_ARENA: i64 = 30;
pub const QUEUE_ARAM: i64 = 450;
pub const QUEUE_ARENA: i64 = 1700;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChampionSession {
    local_player_cell_id: i64,
    #[serde(default)]
    my_team: Vec<TeamMember>,
    #[serde(default)]
    actions: Vec<Vec<ChampionAction>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamMember {
    cell_id: i64,
    #[serde(default)]
    champion_id: i64,
    #[serde(default)]
    assigned_position: String,
    #[serde(default)]
    spell1_id: i64,
    #[serde(default)]
    spell2_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChampionAction {
    actor_cell_id: i64,
    #[serde(default)]
    champion_id: i64,
    #[serde(default)]
    completed: bool,
    #[serde(rename = "type", default)]
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GameflowSession {
    #[serde(default)]
    map: Option<MapInfo>,
    #[serde(default)]
    queue: Option<QueueInfo>,
    #[serde(default, rename = "gameData")]
    game_data: Option<GameData>,
}

#[derive(Debug, Clone, Deserialize)]
struct MapInfo {
    #[serde(default)]
    id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct QueueInfo {
    #[serde(default)]
    id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameData {
    #[serde(default)]
    queue: Option<GameflowQueue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameflowQueue {
    #[serde(default)]
    game_mode: String,
}

/// Everything the rune screen needs from champion select.
#[derive(Debug, Clone, PartialEq)]
pub struct ChampSelectContext {
    pub champion_id: i64,
    pub champion_name: String,
    /// Raw LCU `assignedPosition` (may be empty in blind pick).
    pub assigned_position: String,
    pub locked: bool,
    /// Whether this draft has any pick actions at all. Modes like ARAM and
    /// Arena have none, so `locked` can never become true there.
    pub has_pick_actions: bool,
    pub map_id: i64,
    pub queue_id: i64,
    /// LCU `gameData.queue.gameMode` (CLASSIC, ARAM, CHERRY, …), used to filter
    /// the summoner-spell picker. Empty when the gameflow is not readable.
    pub game_mode: String,
    /// The local player's current summoner spells, `[D, F]`. `0` when unknown.
    pub spell1_id: i64,
    pub spell2_id: i64,
}

impl ChampSelectContext {
    pub fn mode(&self) -> &'static str {
        mode_for(self.map_id, self.queue_id)
    }

    /// op.gg position, preferring the assigned role. Modes without roles use
    /// `none`.
    pub fn position(&self) -> Option<&'static str> {
        if mode_for(self.map_id, self.queue_id) != MODE_RANKED {
            return Some(POSITION_NONE);
        }
        position_from_assigned(&self.assigned_position)
    }

    /// Whether auto-apply should run for this state. Draft modes wait for the
    /// completed pick; modes without pick actions treat any assigned champion
    /// as ready, because ARAM rerolls and bench swaps replace the champion
    /// (the watcher re-applies on the new id).
    pub fn auto_apply_ready(&self) -> bool {
        if self.champion_id <= 0 {
            return false;
        }
        if self.has_pick_actions {
            self.locked
        } else {
            true
        }
    }
}

/// op.gg mode slug for a League map and queue.
pub fn mode_for(map_id: i64, queue_id: i64) -> &'static str {
    if map_id == MAP_ARAM || queue_id == QUEUE_ARAM {
        MODE_ARAM
    } else if map_id == MAP_ARENA || queue_id == QUEUE_ARENA {
        MODE_ARENA
    } else {
        MODE_RANKED
    }
}

/// Maps an LCU `assignedPosition` to an op.gg position.
pub fn position_from_assigned(assigned: &str) -> Option<&'static str> {
    match assigned.trim().to_ascii_lowercase().as_str() {
        "top" => Some("top"),
        "jungle" => Some("jungle"),
        "middle" => Some("mid"),
        "bottom" => Some("adc"),
        "utility" => Some("support"),
        _ => None,
    }
}

pub fn parse_champion_summary(body: &str) -> Result<HashMap<i64, String>, RuneError> {
    #[derive(Deserialize)]
    struct Summary {
        id: i64,
        #[serde(default)]
        name: String,
    }
    let entries: Vec<Summary> = serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read champion data: {e}")))?;
    Ok(entries.into_iter().map(|entry| (entry.id, entry.name)).collect())
}

/// Parses `/lol-champ-select/v1/session`, preferring the champion shown for the
/// local player and reporting whether the pick is locked in.
pub fn parse_champ_select(body: &str) -> Result<Option<ChampSelectContext>, RuneError> {
    let session: ChampionSession = serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read champion select: {e}")))?;
    let local = session
        .my_team
        .iter()
        .find(|member| member.cell_id == session.local_player_cell_id);
    let actions: Vec<&ChampionAction> = session
        .actions
        .iter()
        .flatten()
        .filter(|action| action.actor_cell_id == session.local_player_cell_id)
        .collect();
    let action_champion = actions
        .iter()
        .find(|action| action.kind == "pick" && action.champion_id > 0)
        .map(|action| action.champion_id);
    let champion_id = local
        .map(|member| member.champion_id)
        .filter(|id| *id > 0)
        .or(action_champion)
        .unwrap_or(0);
    if champion_id == 0 {
        return Ok(None);
    }
    let locked = actions
        .iter()
        .any(|action| action.kind == "pick" && action.completed);
    let has_pick_actions = session
        .actions
        .iter()
        .flatten()
        .any(|action| action.kind == "pick");
    Ok(Some(ChampSelectContext {
        champion_id,
        champion_name: String::new(),
        assigned_position: local
            .map(|member| member.assigned_position.clone())
            .unwrap_or_default(),
        locked,
        has_pick_actions,
        map_id: 0,
        queue_id: 0,
        game_mode: String::new(),
        spell1_id: local.map(|member| member.spell1_id).unwrap_or(0),
        spell2_id: local.map(|member| member.spell2_id).unwrap_or(0),
    }))
}

/// The map, queue and game mode from `/lol-gameflow/v1/session`.
#[derive(Debug, Clone, PartialEq)]
pub struct GameflowInfo {
    pub map_id: i64,
    pub queue_id: i64,
    pub game_mode: String,
}

/// Parses `/lol-gameflow/v1/session` for the map, queue and game-mode ids.
pub fn parse_gameflow(body: &str) -> Result<GameflowInfo, RuneError> {
    let session: GameflowSession = serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read the gameflow: {e}")))?;
    Ok(GameflowInfo {
        map_id: session.map.map(|map| map.id).unwrap_or(0),
        queue_id: session.queue.map(|queue| queue.id).unwrap_or(0),
        game_mode: session
            .game_data
            .and_then(|data| data.queue)
            .map(|queue| queue.game_mode)
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_assigned_positions_to_opgg_positions() {
        assert_eq!(position_from_assigned("top"), Some("top"));
        assert_eq!(position_from_assigned("JUNGLE"), Some("jungle"));
        assert_eq!(position_from_assigned("middle"), Some("mid"));
        assert_eq!(position_from_assigned("bottom"), Some("adc"));
        assert_eq!(position_from_assigned("utility"), Some("support"));
        assert_eq!(position_from_assigned(""), None);
        assert_eq!(position_from_assigned("fill"), None);
    }

    #[test]
    fn maps_map_and_queue_to_opgg_modes() {
        assert_eq!(mode_for(11, 420), MODE_RANKED);
        assert_eq!(mode_for(MAP_ARAM, 0), MODE_ARAM);
        assert_eq!(mode_for(0, QUEUE_ARAM), MODE_ARAM);
        assert_eq!(mode_for(MAP_ARENA, 0), MODE_ARENA);
        assert_eq!(mode_for(0, QUEUE_ARENA), MODE_ARENA);
    }

    #[test]
    fn reads_the_local_pick_and_position_from_a_session() {
        let context = parse_champ_select(
            r#"{
                "localPlayerCellId": 2,
                "myTeam": [
                    {"cellId": 1, "championId": 266, "assignedPosition": "top"},
                    {"cellId": 2, "championId": 103, "assignedPosition": "middle"}
                ],
                "actions": [[{"actorCellId": 2, "championId": 103, "completed": true, "type": "pick"}]]
            }"#,
        )
        .unwrap()
        .expect("a champion is being picked");
        assert_eq!(context.champion_id, 103);
        assert_eq!(context.assigned_position, "middle");
        assert!(context.locked);
        assert!(context.has_pick_actions);
        assert!(context.auto_apply_ready());
    }

    #[test]
    fn falls_back_to_a_pending_pick_action_before_the_champion_is_set() {
        let context = parse_champ_select(
            r#"{
                "localPlayerCellId": 4,
                "myTeam": [{"cellId": 4, "championId": 0, "assignedPosition": "utility"}],
                "actions": [[{"actorCellId": 4, "championId": 99, "completed": false, "type": "pick"}]]
            }"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(context.champion_id, 99);
        assert_eq!(context.assigned_position, "utility");
        assert!(!context.locked);
        // A draft with a pending pick is not ready to auto-apply yet.
        assert!(!context.auto_apply_ready());
    }

    #[test]
    fn a_mode_without_pick_actions_is_ready_as_soon_as_a_champion_is_set() {
        // ARAM: no pick actions, the champion arrives on the local player.
        let context = parse_champ_select(
            r#"{
                "localPlayerCellId": 0,
                "myTeam": [{"cellId": 0, "championId": 103, "assignedPosition": ""}],
                "actions": []
            }"#,
        )
        .unwrap()
        .expect("a champion is assigned");
        assert!(!context.has_pick_actions);
        assert!(!context.locked);
        assert!(context.auto_apply_ready());
    }

    #[test]
    fn a_reroll_changes_the_champion_so_auto_apply_runs_again() {
        let session = |champion: i64| {
            format!(
                r#"{{
                    "localPlayerCellId": 0,
                    "myTeam": [{{"cellId": 0, "championId": {champion}, "assignedPosition": ""}}],
                    "actions": []
                }}"#
            )
        };
        let first = parse_champ_select(&session(103)).unwrap().unwrap();
        let rerolled = parse_champ_select(&session(266)).unwrap().unwrap();
        assert!(first.auto_apply_ready() && rerolled.auto_apply_ready());
        assert_ne!(first.champion_id, rerolled.champion_id);
    }

    #[test]
    fn reports_no_context_before_any_champion_is_shown() {
        let context = parse_champ_select(
            r#"{
                "localPlayerCellId": 0,
                "myTeam": [{"cellId": 0, "championId": 0, "assignedPosition": ""}],
                "actions": []
            }"#,
        )
        .unwrap();
        assert!(context.is_none());
    }

    #[test]
    fn reads_the_map_queue_and_game_mode_from_a_gameflow_session() {
        let info = parse_gameflow(
            r#"{"map":{"id":11},"queue":{"id":420},"gameData":{"queue":{"gameMode":"CLASSIC"}}}"#,
        )
        .unwrap();
        assert_eq!((info.map_id, info.queue_id), (11, 420));
        assert_eq!(info.game_mode, "CLASSIC");
    }

    #[test]
    fn reads_the_local_players_spells_from_a_session() {
        let context = parse_champ_select(
            r#"{
                "localPlayerCellId": 2,
                "myTeam": [
                    {"cellId": 2, "championId": 103, "assignedPosition": "middle", "spell1Id": 4, "spell2Id": 14}
                ],
                "actions": []
            }"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(context.spell1_id, 4);
        assert_eq!(context.spell2_id, 14);
    }
}
