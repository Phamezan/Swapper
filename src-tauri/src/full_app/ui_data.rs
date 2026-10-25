//! Read-only full-window data from the existing LCU and rune catalogs.
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The wide page offers every reported opponent. The flyout keeps its two
/// existing fifteen-card strips and its sample-adjusted ordering.
#[tauri::command]
pub async fn full_app_matchups(
    champion_id: i64,
    position: String,
    tier: String,
) -> Result<crate::runes::ChampionCountersView, String> {
    use crate::runes::{counters, counters_view, data, lolalytics};
    let names = data::champion_names().await.map_err(|e| e.to_string())?;
    let name = names.get(&champion_id).cloned().ok_or("Unknown champion")?;
    let slug = lolalytics::champion_slug(&name).ok_or("Unknown champion")?;
    let lane = lolalytics::lane(&position);
    let found = counters::load(&slug, champion_id, lane, &tier)
        .await
        .map_err(|e| e.to_string())?;
    let Some(found) = found else {
        return Ok(crate::runes::champion_counters_view(champion_id, Some(&position), &tier).await);
    };
    let rows = found
        .value
        .rows
        .iter()
        .map(|row| counters_view::CounterRowView {
            champion_id: row.champion_id,
            name: names
                .get(&row.champion_id)
                .cloned()
                .unwrap_or_else(|| format!("Champion {}", row.champion_id)),
            win_pct: row.vs_win_pct,
            delta: row.vs_win_pct - row.all_win_pct,
            games: row.games,
            prev_games: row.prev_games,
            low_sample: row.games + row.prev_games < crate::runes::matchup::MIN_MATCHUP_GAMES,
        })
        .collect();
    Ok(crate::runes::ChampionCountersView {
        champion_id,
        champion_name: name,
        position,
        best_into: Vec::new(),
        beats: rows,
        unavailable: false,
        stale: found.stale,
        updated_at: found.fetched_at,
    })
}

#[derive(Serialize)]
pub struct RuneCatalog {
    trees: Vec<crate::runes::view::RuneTreeView>,
    shards: Vec<crate::runes::view::RuneRowView>,
}

#[tauri::command]
pub async fn full_app_rune_catalog() -> Result<RuneCatalog, String> {
    let catalog = crate::runes::data::catalog()
        .await
        .map_err(|e| e.to_string())?;
    let stats = crate::runes::stats::Aggregate::default();
    Ok(RuneCatalog {
        trees: crate::runes::view::build_trees(&catalog, &stats),
        shards: crate::runes::view::build_shards(&catalog, &stats),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftMember {
    champion_id: i64,
    position: String,
    local: bool,
    /// The pick is completed; otherwise `champion_id` is a hover or pick intent.
    locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftTimer {
    phase: String,
    remaining_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    allies: Vec<DraftMember>,
    enemies: Vec<DraftMember>,
    bans: Vec<i64>,
    ally_bans: Vec<i64>,
    enemy_bans: Vec<i64>,
    can_lock: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pickable: Option<Vec<i64>>,
    timer: Option<DraftTimer>,
}

pub(crate) fn draft_from(value: &Value) -> Draft {
    let local = value["localPlayerCellId"].as_i64();
    let actions = || {
        value["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_array)
            .flatten()
    };
    let picks = || actions().filter(|action| action["type"] == "pick");
    let members = |field: &str| {
        value[field]
            .as_array()
            .into_iter()
            .flatten()
            .map(|member| DraftMember {
                champion_id: Some(member["championId"].as_i64().unwrap_or(0))
                    .filter(|id| *id > 0)
                    .or(member["championPickIntent"].as_i64())
                    .unwrap_or(0)
                    .max(0),
                position: member["assignedPosition"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                local: field == "myTeam" && local.is_some() && member["cellId"].as_i64() == local,
                locked: picks().any(|action| {
                    action["completed"] == true
                        && member["cellId"].as_i64().is_some()
                        && action["actorCellId"] == member["cellId"]
                }),
            })
            .collect()
    };
    let bans = actions()
        .filter(|action| action["type"] == "ban" && action["completed"] == true)
        .filter_map(|action| action["championId"].as_i64())
        .filter(|id| *id > 0)
        .collect();
    let team_bans = |field: &str| {
        actions()
            .filter(|action| action["type"] == "ban" && action["completed"] == true)
            .filter(|action| {
                let actor = action["actorCellId"].as_i64();
                actor.is_some() && value[field].as_array().into_iter().flatten()
                    .any(|member| member["cellId"].as_i64() == actor)
            })
            .filter_map(|action| action["championId"].as_i64())
            .filter(|id| *id > 0)
            .collect()
    };
    let can_lock = local.is_some() && value["timer"]["phase"] == "BAN_PICK"
        && picks().any(|action| action["actorCellId"].as_i64() == local
            && action["completed"] != true && action["isInProgress"] == true);
    let timer = value["timer"]["phase"].as_str().map(|phase| DraftTimer {
        phase: phase.to_string(),
        remaining_ms: value["timer"]["adjustedTimeLeftInPhase"]
            .as_i64()
            .unwrap_or(0)
            .max(0),
    });
    Draft {
        allies: members("myTeam"),
        enemies: members("theirTeam"),
        bans,
        ally_bans: team_bans("myTeam"),
        enemy_bans: team_bans("theirTeam"),
        can_lock,
        pickable: None,
        timer,
    }
}

#[tauri::command]
pub async fn full_app_draft() -> Result<Draft, String> {
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let session: Value = crate::runes::lcu_get(&client, "/lol-champ-select/v1/session")
        .await
        .map_err(|e| e.to_string())?;
    let mut draft = draft_from(&session);
    draft.pickable = crate::runes::lcu_get(&client, "/lol-champ-select/v1/pickable-champion-ids")
        .await.ok();
    Ok(draft)
}

#[derive(Serialize, Deserialize)]
pub struct Ability {
    key: String,
    name: String,
    icon: Option<String>,
}

fn ability_path(spell: &Value) -> Option<&str> {
    let path = spell["abilityIconPath"].as_str()?;
    (path.starts_with("/lol-game-data/assets/") && !path.contains("..") && !path.contains('\\'))
        .then_some(path)
}

#[tauri::command]
pub async fn full_app_abilities(champion_id: i64) -> Result<Vec<Ability>, String> {
    if champion_id <= 0 {
        return Err("Unknown champion.".into());
    }
    let path = crate::vault::data_root()?
        .join("art")
        .join(format!("abilities-{champion_id}.json"));
    let cached_path = path.clone();
    let cached = tokio::task::spawn_blocking(move || {
        if std::fs::metadata(&cached_path).ok()?.len() > 1024 * 1024 {
            return None;
        }
        serde_json::from_slice::<Vec<Ability>>(&std::fs::read(cached_path).ok()?).ok()
    })
    .await
    .unwrap_or(None);
    if let Some(abilities) = cached {
        return Ok(abilities);
    }
    let champion: Value = crate::runes::game_data::json(&format!(
        "/lol-game-data/assets/v1/champions/{champion_id}.json"
    ))
    .await
    .map_err(|e| e.to_string())?;
    let mut abilities = Vec::new();
    for (index, spell) in champion["spells"]
        .as_array()
        .into_iter()
        .flatten()
        .take(4)
        .enumerate()
    {
        let icon = match ability_path(spell) {
            Some(asset) => crate::runes::game_data::fetch_bytes(asset)
                .await
                .ok()
                .filter(|bytes| {
                    bytes.len() <= 256 * 1024 && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                })
                .map(|bytes| {
                    format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    )
                }),
            None => None,
        };
        abilities.push(Ability {
            key: ["Q", "W", "E", "R"][index].into(),
            name: spell["name"].as_str().unwrap_or("").into(),
            icon,
        });
    }
    if abilities.len() == 4 && abilities.iter().all(|ability| ability.icon.is_some()) {
        if let Ok(bytes) = serde_json::to_vec(&abilities) {
            let _ = tokio::task::spawn_blocking(move || {
                std::fs::create_dir_all(path.parent()?).ok()?;
                let pending = path.with_extension("tmp");
                std::fs::write(&pending, bytes).ok()?;
                std::fs::rename(pending, path).ok()
            })
            .await;
        }
    }
    Ok(abilities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn draft_exposes_team_bans_and_only_the_local_pick_turn() {
        let mut session = json!({"localPlayerCellId": 2,
            "timer": {"phase": "BAN_PICK"},
            "myTeam": [{"cellId": 2}], "theirTeam": [{"cellId": 7}],
            "actions": [[
                {"actorCellId": 7, "type": "ban", "completed": true, "championId": 157},
                {"actorCellId": 2, "type": "ban", "completed": true, "championId": 777},
                {"actorCellId": 2, "type": "ban", "completed": false, "championId": 103},
                {"actorCellId": 2, "type": "pick", "completed": false, "isInProgress": true}
            ]]});
        let draft = draft_from(&session);
        assert_eq!(draft.ally_bans, vec![777]);
        assert_eq!(draft.enemy_bans, vec![157]);
        assert!(draft.can_lock);
        for (field, value) in [("isInProgress", json!(false)), ("completed", json!(true)),
            ("type", json!("ban")), ("actorCellId", json!(7))] {
            let mut other = session.clone();
            other["actions"][0][3][field] = value;
            assert!(!draft_from(&other).can_lock, "{field}");
        }
        session["timer"]["phase"] = json!("PLANNING");
        assert!(!draft_from(&session).can_lock);
        session["timer"]["phase"] = json!("BAN_PICK");
        session["localPlayerCellId"] = Value::Null;
        assert!(!draft_from(&session).can_lock);
    }
    #[test]
    fn draft_preserves_hidden_slots_and_only_completed_bans() {
        let draft = draft_from(
            &json!({"localPlayerCellId": 2, "myTeam": [{"cellId":2,"championId":103,"assignedPosition":"middle"}], "theirTeam":[{"championId":0},{"championId":238}], "actions":[[{"type":"ban","completed":true,"championId":222},{"type":"ban","completed":false,"championId":23},{"type":"pick","completed":true,"championId":1}]]}),
        );
        assert!(draft.allies[0].local);
        assert_eq!(draft.enemies.len(), 2);
        assert_eq!(draft.enemies[0].champion_id, 0);
        assert_eq!(draft.bans, vec![222]);
        assert!(draft.timer.is_none());
        let hovering = draft_from(
            &json!({"localPlayerCellId": 1, "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 21500}, "myTeam": [{"cellId":1,"championId":0,"championPickIntent":103},{"cellId":2,"championId":64}], "actions":[[{"type":"pick","actorCellId":2,"completed":true,"championId":64},{"type":"pick","actorCellId":1,"completed":false,"championId":0}]]}),
        );
        assert_eq!(hovering.allies[0].champion_id, 103);
        assert!(!hovering.allies[0].locked);
        assert!(hovering.allies[1].locked);
        assert_eq!(hovering.timer.as_ref().map(|t| t.remaining_ms), Some(21500));
        assert!(draft_from(&json!({"myTeam":[{}]}))
            .allies
            .iter()
            .all(|member| !member.local));
    }
    #[test]
    fn ability_paths_stay_in_game_data() {
        assert!(
            ability_path(&json!({"abilityIconPath":"/lol-game-data/assets/ASSETS/q.png"}))
                .is_some()
        );
        for path in [
            "https://example.com/q.png",
            "/lol-game-data/assets/../secret",
            "/lol-game-data/assets/\\secret",
        ] {
            assert!(ability_path(&json!({"abilityIconPath":path})).is_none());
        }
    }
}

/// One champion in the signed-in player's pool, from League Client's champion mastery (read-only).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolChampion {
    pub champion_id: i64,
    pub points: i64,
    /// Epoch ms of the last game on this champion (0 when unknown).
    pub last_played: i64,
}

/// The most-played champions (by mastery points), at most `limit`. Empty when the client is closed.
pub fn pool_from_mastery(value: &Value, limit: usize) -> Vec<PoolChampion> {
    let mut pool: Vec<PoolChampion> = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(PoolChampion {
                champion_id: entry["championId"].as_i64().filter(|id| *id > 0)?,
                points: entry["championPoints"].as_i64().unwrap_or(0),
                last_played: entry["lastPlayTime"].as_i64().unwrap_or(0),
            })
        })
        .collect();
    pool.sort_by(|a, b| b.points.cmp(&a.points));
    pool.truncate(limit);
    pool
}

#[tauri::command]
pub async fn full_app_champion_pool() -> Result<Vec<PoolChampion>, String> {
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let mastery: Value = crate::runes::lcu_get(&client, "/lol-champion-mastery/v1/local-player/champion-mastery")
        .await
        .map_err(|e| e.to_string())?;
    Ok(pool_from_mastery(&mastery, 12))
}

#[cfg(test)]
mod pool_tests {
    #[test]
    fn pool_is_top_mastery_with_last_played() {
        let value = serde_json::json!([
            {"championId": 266, "championPoints": 900, "lastPlayTime": 5},
            {"championId": 103, "championPoints": 5000, "lastPlayTime": 9},
            {"championId": 0, "championPoints": 99999},
            {"championId": 86, "championPoints": 10}
        ]);
        let pool = super::pool_from_mastery(&value, 2);
        assert_eq!(pool.iter().map(|c| c.champion_id).collect::<Vec<_>>(), vec![103, 266]);
        assert_eq!(pool[0].last_played, 9);
        assert!(super::pool_from_mastery(&serde_json::json!({}), 5).is_empty());
    }
}
