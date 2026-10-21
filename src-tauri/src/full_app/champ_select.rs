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
    lock_ready(&session, champion_id)?;
    crate::remote::lock_champion().await
}

fn lock_ready(session: &Value, champion_id: i64) -> Result<(), String> {
    if session["timer"]["phase"] != "BAN_PICK" {
        return Err("Wait for your pick turn to lock in.".into());
    }
    let local = session["localPlayerCellId"].as_i64();
    let action = session["actions"]
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
        .ok_or("It is not your pick turn yet.")?;
    if action["type"] != "pick" {
        return Err("It is your ban turn, not your pick.".into());
    }
    if action["championId"].as_i64() != Some(champion_id) || champion_id <= 0 {
        return Err("Your hovered champion changed. Check it and try again.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(phase: &str, kind: &str, champion: i64, in_progress: bool) -> Value {
        json!({"localPlayerCellId": 3, "timer": {"phase": phase}, "actions": [[
            {"actorCellId": 1, "type": "pick", "championId": 99, "completed": false, "isInProgress": true},
            {"actorCellId": 3, "type": kind, "championId": champion, "completed": false, "isInProgress": in_progress}
        ]]})
    }

    #[test]
    fn locks_only_your_in_progress_pick_of_the_confirmed_champion() {
        assert!(lock_ready(&session("BAN_PICK", "pick", 103, true), 103).is_ok());
        assert!(lock_ready(&session("PLANNING", "pick", 103, true), 103).is_err());
        assert!(lock_ready(&session("BAN_PICK", "ban", 103, true), 103).is_err());
        assert!(lock_ready(&session("BAN_PICK", "pick", 103, false), 103).is_err());
        assert!(lock_ready(&session("BAN_PICK", "pick", 238, true), 103).is_err());
        assert!(lock_ready(&session("BAN_PICK", "pick", 0, true), 0).is_err());
    }
}
