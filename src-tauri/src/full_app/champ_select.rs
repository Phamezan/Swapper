//! Hover and lock-in from the full app's champion select page, through the
//! same LCU actions the phone remote uses.
use serde_json::Value;

/// Shows the champion as your pick (or pick intent before your turn).
#[tauri::command]
pub async fn full_app_hover_champion(champion_id: i64) -> Result<(), String> {
    crate::remote::prepick_champion(champion_id).await
}

/// Locks in only when it is your pick turn and the hovered champion is the one
/// the page confirmed, so a stale button can never lock a ban or another pick.
#[tauri::command]
pub async fn full_app_lock_in(champion_id: i64) -> Result<(), String> {
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let session: Value = crate::runes::lcu_get(&client, "/lol-champ-select/v1/session")
        .await
        .map_err(|e| e.to_string())?;
    let expected = lock_ready(&session, champion_id, "pick")?;
    crate::remote::lock_confirmed_pick(expected).await
}

/// Hovers a champion as your ban, only on your ban turn.
#[tauri::command]
pub async fn full_app_hover_ban(champion_id: i64) -> Result<(), String> {
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let session: Value = crate::runes::lcu_get(&client, "/lol-champ-select/v1/session")
        .await
        .map_err(|e| e.to_string())?;
    if !turn_action(&session).is_ok_and(|action| action["type"] == "ban") {
        return Err("It is not your ban turn.".into());
    }
    crate::remote::select_champion(champion_id).await
}

/// Bans only when it is your ban turn and the hovered ban is the one the page confirmed.
#[tauri::command]
pub async fn full_app_ban(champion_id: i64) -> Result<(), String> {
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let session: Value = crate::runes::lcu_get(&client, "/lol-champ-select/v1/session")
        .await
        .map_err(|e| e.to_string())?;
    let expected = lock_ready(&session, champion_id, "ban")?;
    crate::remote::lock_confirmed_pick(expected).await
}

/// Your in-progress pick or ban action.
fn turn_action(session: &Value) -> Result<&Value, String> {
    let local = session["localPlayerCellId"].as_i64();
    session["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .find(|action| {
            action["actorCellId"].as_i64() == local
                && action["completed"] != true
                && action["isInProgress"] == true
        })
        .ok_or_else(|| "It is not your turn yet.".to_string())
}

fn lock_ready(session: &Value, champion_id: i64, kind: &'static str) -> Result<crate::remote::PickConfirmation, String> {
    if session["timer"]["phase"] != "BAN_PICK" {
        return Err(format!("Wait for your {kind} turn."));
    }
    let local = session["localPlayerCellId"].as_i64();
    let action = turn_action(session).map_err(|_| format!("It is not your {kind} turn yet."))?;
    if action["type"] != kind {
        return Err(if kind == "ban" { "It is your pick turn, not your ban." } else { "It is your ban turn, not your pick." }.into());
    }
    if action["championId"].as_i64() != Some(champion_id) || champion_id <= 0 {
        return Err("Your hovered champion changed. Check it and try again.".into());
    }
    Ok(crate::remote::PickConfirmation {
        game_id: session["gameId"].as_i64().ok_or("Draft identity is unavailable.")?,
        cell_id: local.ok_or("Your draft slot is unavailable.")?,
        action_id: action["id"].as_i64().ok_or("Draft action is unavailable.")?,
        champion_id,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(phase: &str, kind: &str, champion: i64, in_progress: bool) -> Value {
        json!({"gameId": 42, "localPlayerCellId": 3, "timer": {"phase": phase}, "actions": [[
            {"actorCellId": 1, "type": "pick", "championId": 99, "completed": false, "isInProgress": true},
            {"id": 7, "actorCellId": 3, "type": kind, "championId": champion, "completed": false, "isInProgress": in_progress}
        ]]})
    }

    #[test]
    fn locks_only_your_in_progress_pick_of_the_confirmed_champion() {
        assert!(lock_ready(&session("BAN_PICK", "pick", 103, true), 103, "pick").is_ok());
        assert!(lock_ready(&session("PLANNING", "pick", 103, true), 103, "pick").is_err());
        assert!(lock_ready(&session("BAN_PICK", "ban", 103, true), 103, "pick").is_err());
        assert!(lock_ready(&session("BAN_PICK", "pick", 103, false), 103, "pick").is_err());
        assert!(lock_ready(&session("BAN_PICK", "pick", 238, true), 103, "pick").is_err());
        assert!(lock_ready(&session("BAN_PICK", "pick", 0, true), 0, "pick").is_err());
    }

    #[test]
    fn bans_only_your_in_progress_ban_of_the_confirmed_champion() {
        let ban = lock_ready(&session("BAN_PICK", "ban", 103, true), 103, "ban").expect("your ban turn");
        assert_eq!((ban.kind, ban.action_id, ban.champion_id), ("ban", 7, 103));
        assert!(lock_ready(&session("BAN_PICK", "pick", 103, true), 103, "ban").is_err(), "a pick is never confirmed as a ban");
        assert!(lock_ready(&session("BAN_PICK", "ban", 238, true), 103, "ban").is_err());
        assert!(lock_ready(&session("BAN_PICK", "ban", 103, false), 103, "ban").is_err());
    }
}
