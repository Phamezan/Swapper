use std::collections::{HashMap, HashSet};
use std::time::Duration;

use axum::http::StatusCode;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};

use crate::lcu::{self, LcuEndpoint};

const PHASE_PATH: &str = "/lol-gameflow/v1/gameflow-phase";
const SEARCH_PATH: &str = "/lol-matchmaking/v1/search";
const READY_CHECK_PATH: &str = "/lol-matchmaking/v1/ready-check";
const SESSION_PATH: &str = "/lol-champ-select/v1/session";
const PICKABLE_PATH: &str = "/lol-champ-select/v1/pickable-champion-ids";
const BANNABLE_PATH: &str = "/lol-champ-select/v1/bannable-champion-ids";
const CHAMPIONS_PATH: &str = "/lol-game-data/assets/v1/champion-summary.json";
const POSITIONS_PATH: &str = "/lol-perks/v1/recommended-champion-positions";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    phase: String,
    queue_time_seconds: Option<u64>,
    ready_check: bool,
    champion_select: Option<ChampionSelectView>,
    message: Option<String>,
}

impl GameSnapshot {
    fn unavailable(message: &str) -> Self {
        Self {
            phase: "Unavailable".into(),
            queue_time_seconds: None,
            ready_check: false,
            champion_select: None,
            message: Some(message.into()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampionSelectView {
    game_id: i64,
    action_kind: Option<String>,
    selected_champion_id: Option<i64>,
    prepick_champion_id: Option<i64>,
    can_complete: bool,
    can_prepick: bool,
    available_champion_ids: Vec<i64>,
}

#[derive(Deserialize)]
struct ChampionSummary {
    id: i64,
    name: String,
}

#[derive(Serialize)]
pub(super) struct Champion {
    id: i64,
    name: String,
    positions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecommendedPositions {
    #[serde(default)]
    recommended_positions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadyCheck {
    state: String,
    #[serde(default)]
    player_response: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatchmakingSearch {
    search_state: String,
    time_in_queue: Option<f64>,
}

impl MatchmakingSearch {
    fn elapsed_seconds(&self) -> Option<u64> {
        self.time_in_queue
            .filter(|seconds| {
                self.search_state == "Searching" && seconds.is_finite() && *seconds >= 0.0
            })
            .map(|seconds| seconds.floor() as u64)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChampionSession {
    game_id: i64,
    local_player_cell_id: i64,
    actions: Vec<Vec<ChampionAction>>,
    timer: ChampionTimer,
}

#[derive(Deserialize)]
struct ChampionTimer {
    phase: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChampionAction {
    id: i64,
    actor_cell_id: i64,
    champion_id: i64,
    completed: bool,
    #[serde(default)]
    is_in_progress: bool,
    #[serde(rename = "type")]
    kind: String,
}

impl ChampionSession {
    fn current_action(&self) -> Option<&ChampionAction> {
        if self.timer.phase != "BAN_PICK" {
            return None;
        }
        let local_action = |action: &&ChampionAction| {
            action.actor_cell_id == self.local_player_cell_id
                && !action.completed
                && matches!(action.kind.as_str(), "pick" | "ban")
        };
        if let Some(action) = self
            .actions
            .iter()
            .flatten()
            .filter(local_action)
            .find(|action| action.is_in_progress)
        {
            return Some(action);
        }
        self.actions
            .iter()
            .find(|turn| turn.iter().any(|action| !action.completed))?
            .iter()
            .find(local_action)
    }

    fn prepick_action(&self) -> Option<&ChampionAction> {
        self.actions.iter().flatten().find(|action| {
            action.actor_cell_id == self.local_player_cell_id
                && action.kind == "pick"
                && !action.completed
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionChoice {
    pub champion_id: i64,
}

#[derive(Serialize)]
pub struct ActionResult {
    ok: bool,
    message: Option<String>,
}

impl ActionResult {
    pub fn success() -> Self {
        Self {
            ok: true,
            message: None,
        }
    }

    pub fn error(message: &str) -> Self {
        Self {
            ok: false,
            message: Some(message.into()),
        }
    }
}

pub struct ActionError {
    pub status: StatusCode,
    pub message: String,
}

impl ActionError {
    fn conflict(message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn unavailable() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: "League Client did not respond. Try again after it reconnects.".into(),
        }
    }
}

async fn connection() -> Result<(Client, LcuEndpoint), ActionError> {
    let endpoint = tokio::task::spawn_blocking(lcu::discover)
        .await
        .ok()
        .flatten()
        .ok_or_else(ActionError::unavailable)?;
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| ActionError::unavailable())?;
    Ok((client, endpoint))
}

async fn get<T: for<'de> Deserialize<'de>>(
    client: &Client,
    endpoint: &LcuEndpoint,
    path: &str,
) -> Result<T, ActionError> {
    client
        .get(lcu::api_url(endpoint, path))
        .header(reqwest::header::AUTHORIZATION, lcu::authorization(endpoint))
        .send()
        .await
        .map_err(|_| ActionError::unavailable())?
        .error_for_status()
        .map_err(|_| ActionError::unavailable())?
        .json()
        .await
        .map_err(|_| ActionError::unavailable())
}

fn fallback_ban_candidates(summaries: Vec<ChampionSummary>) -> Vec<i64> {
    summaries
        .into_iter()
        .filter(|champion| (1..10_000).contains(&champion.id))
        .map(|champion| champion.id)
        .collect()
}

async fn ban_candidates(client: &Client, endpoint: &LcuEndpoint) -> Result<Vec<i64>, ActionError> {
    if let Ok(ids) = get::<Vec<i64>>(client, endpoint, BANNABLE_PATH).await {
        let ids: Vec<_> = ids
            .into_iter()
            .filter(|id| (1..10_000).contains(id))
            .collect();
        if !ids.is_empty() {
            return Ok(ids);
        }
    }
    get::<Vec<ChampionSummary>>(client, endpoint, CHAMPIONS_PATH)
        .await
        .map(fallback_ban_candidates)
}

async fn write(
    client: &Client,
    endpoint: &LcuEndpoint,
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<(), ActionError> {
    let mut request = client
        .request(method, lcu::api_url(endpoint, path))
        .header(reqwest::header::AUTHORIZATION, lcu::authorization(endpoint));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|_| ActionError::unavailable())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(ActionError::conflict(
            "League rejected this action. Refresh the page and try again.",
        ))
    }
}

pub async fn snapshot() -> GameSnapshot {
    let Ok((client, endpoint)) = connection().await else {
        return GameSnapshot::unavailable("League Client is not connected.");
    };
    let Ok(phase) = get::<String>(&client, &endpoint, PHASE_PATH).await else {
        return GameSnapshot::unavailable("League Client state is unavailable.");
    };
    let mut snapshot = GameSnapshot {
        phase: phase.clone(),
        queue_time_seconds: None,
        ready_check: false,
        champion_select: None,
        message: None,
    };
    if phase == "Matchmaking" {
        snapshot.queue_time_seconds = get::<MatchmakingSearch>(&client, &endpoint, SEARCH_PATH)
            .await
            .ok()
            .and_then(|search| search.elapsed_seconds());
    } else if phase == "ReadyCheck" {
        match get::<ReadyCheck>(&client, &endpoint, READY_CHECK_PATH).await {
            Ok(check) => snapshot.ready_check = ready_check_available(&check),
            Err(_) => snapshot.message = Some("Ready check details are unavailable.".into()),
        }
    } else if phase == "ChampSelect" {
        match get::<ChampionSession>(&client, &endpoint, SESSION_PATH).await {
            Ok(session) => {
                let action = session.current_action();
                let prepick = session.prepick_action();
                let available = if action.is_some_and(|action| action.kind == "ban") {
                    ban_candidates(&client, &endpoint).await
                } else {
                    get::<Vec<i64>>(&client, &endpoint, PICKABLE_PATH).await
                };
                let Ok(available) = available else {
                    snapshot.message = Some("Available champion details are unavailable.".into());
                    return snapshot;
                };
                let banned: HashSet<i64> = session
                    .actions
                    .iter()
                    .flatten()
                    .filter(|action| action.kind == "ban" && action.completed)
                    .map(|action| action.champion_id)
                    .collect();
                snapshot.champion_select = Some(ChampionSelectView {
                    game_id: session.game_id,
                    action_kind: action.map(|action| action.kind.clone()),
                    selected_champion_id: action
                        .and_then(|action| (action.champion_id > 0).then_some(action.champion_id)),
                    prepick_champion_id: prepick
                        .and_then(|action| (action.champion_id > 0).then_some(action.champion_id)),
                    can_complete: action.is_some_and(|action| action.champion_id > 0),
                    can_prepick: prepick.is_some(),
                    available_champion_ids: available
                        .into_iter()
                        .filter(|id| !banned.contains(id))
                        .collect(),
                });
            }
            Err(_) => snapshot.message = Some("Champion select details are unavailable.".into()),
        }
    }
    snapshot
}

pub async fn catalog() -> Result<Vec<Champion>, ActionError> {
    let (client, endpoint) = connection().await?;
    let summaries = get::<Vec<ChampionSummary>>(&client, &endpoint, CHAMPIONS_PATH).await?;
    let positions =
        get::<HashMap<String, RecommendedPositions>>(&client, &endpoint, POSITIONS_PATH)
            .await
            .unwrap_or_default();
    Ok(catalog_entries(summaries, &positions))
}

fn catalog_entries(
    summaries: Vec<ChampionSummary>,
    positions: &HashMap<String, RecommendedPositions>,
) -> Vec<Champion> {
    let mut champions: Vec<_> = summaries
        .into_iter()
        .filter(|entry| (1..10_000).contains(&entry.id))
        .map(|entry| Champion {
            id: entry.id,
            name: entry.name,
            positions: positions
                .get(&entry.id.to_string())
                .map(|entry| entry.recommended_positions.clone())
                .unwrap_or_default(),
        })
        .collect();
    champions.sort_by(|a, b| a.name.cmp(&b.name));
    champions
}

pub async fn icon(champion_id: i64) -> Result<Vec<u8>, ActionError> {
    if !(1..10_000).contains(&champion_id) {
        return Err(ActionError::conflict("Unknown champion."));
    }
    let (client, endpoint) = connection().await?;
    let path = format!("/lol-game-data/assets/v1/champion-icons/{champion_id}.png");
    let response = client
        .get(lcu::api_url(&endpoint, &path))
        .header(
            reqwest::header::AUTHORIZATION,
            lcu::authorization(&endpoint),
        )
        .send()
        .await
        .map_err(|_| ActionError::unavailable())?
        .error_for_status()
        .map_err(|_| ActionError::unavailable())?;
    let bytes = response
        .bytes()
        .await
        .map_err(|_| ActionError::unavailable())?;
    if bytes.is_empty() || bytes.len() > 512 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ActionError::unavailable());
    }
    Ok(bytes.to_vec())
}

fn ready_check_available(check: &ReadyCheck) -> bool {
    check.state == "InProgress"
        && (check.player_response.is_empty() || check.player_response == "None")
}

pub async fn accept_ready_check() -> Result<(), ActionError> {
    let (client, endpoint) = connection().await?;
    let phase = get::<String>(&client, &endpoint, PHASE_PATH).await?;
    if phase != "ReadyCheck" {
        return Err(ActionError::conflict("There is no active ready check."));
    }
    let check = get::<ReadyCheck>(&client, &endpoint, READY_CHECK_PATH).await?;
    if !ready_check_available(&check) {
        return Err(ActionError::conflict(
            "This ready check is no longer awaiting your response.",
        ));
    }
    write(
        &client,
        &endpoint,
        Method::POST,
        &format!("{READY_CHECK_PATH}/accept"),
        None,
    )
    .await
}

async fn champion_select() -> Result<(Client, LcuEndpoint, ChampionSession), ActionError> {
    let (client, endpoint) = connection().await?;
    let phase = get::<String>(&client, &endpoint, PHASE_PATH).await?;
    if phase != "ChampSelect" {
        return Err(ActionError::conflict("Champion select is not active."));
    }
    let session = get::<ChampionSession>(&client, &endpoint, SESSION_PATH).await?;
    Ok((client, endpoint, session))
}

pub async fn select_champion(champion_id: i64) -> Result<(), ActionError> {
    if champion_id <= 0 {
        return Err(ActionError::conflict("Choose a champion first."));
    }
    let (client, endpoint, session) = champion_select().await?;
    let action = session
        .current_action()
        .or_else(|| session.prepick_action())
        .ok_or_else(|| ActionError::conflict("No pick or ban action is available."))?;
    let available = if action.kind == "ban" {
        ban_candidates(&client, &endpoint).await?
    } else {
        get::<Vec<i64>>(&client, &endpoint, PICKABLE_PATH).await?
    };
    let already_banned =
        session.actions.iter().flatten().any(|entry| {
            entry.kind == "ban" && entry.completed && entry.champion_id == champion_id
        });
    if !available.contains(&champion_id) || already_banned {
        return Err(ActionError::conflict(
            "That champion is not currently available.",
        ));
    }
    write(
        &client,
        &endpoint,
        Method::PATCH,
        &format!("{SESSION_PATH}/actions/{}", action.id),
        Some(serde_json::json!({ "championId": champion_id })),
    )
    .await
}

pub async fn prepick_champion(champion_id: i64) -> Result<(), ActionError> {
    if champion_id <= 0 {
        return Err(ActionError::conflict("Choose a champion first."));
    }
    let (client, endpoint, session) = champion_select().await?;
    let action = session
        .prepick_action()
        .ok_or_else(|| ActionError::conflict("There is no pending pick action."))?;
    let pickable = get::<Vec<i64>>(&client, &endpoint, PICKABLE_PATH).await?;
    let already_banned =
        session.actions.iter().flatten().any(|entry| {
            entry.kind == "ban" && entry.completed && entry.champion_id == champion_id
        });
    if !pickable.contains(&champion_id) || already_banned {
        return Err(ActionError::conflict(
            "That champion is not currently pickable.",
        ));
    }
    write(
        &client,
        &endpoint,
        Method::PATCH,
        &format!("{SESSION_PATH}/actions/{}", action.id),
        Some(serde_json::json!({ "championId": champion_id })),
    )
    .await
}

pub async fn lock_champion() -> Result<(), ActionError> {
    let (client, endpoint, session) = champion_select().await?;
    let action = session
        .current_action()
        .ok_or_else(|| ActionError::conflict("It is not your pick or ban turn."))?;
    if action.champion_id <= 0 {
        return Err(ActionError::conflict(
            "Select a champion before confirming.",
        ));
    }
    write(
        &client,
        &endpoint,
        Method::PATCH,
        &format!("{SESSION_PATH}/actions/{}", action.id),
        Some(serde_json::json!({
            "championId": action.champion_id,
            "completed": true
        })),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_time_only_uses_a_live_search_elapsed_seconds() {
        let searching: MatchmakingSearch = serde_json::from_value(serde_json::json!({
            "searchState": "Searching", "timeInQueue": 125.8
        }))
        .unwrap();
        let found: MatchmakingSearch = serde_json::from_value(serde_json::json!({
            "searchState": "Found", "timeInQueue": 126.0
        }))
        .unwrap();
        let missing: MatchmakingSearch = serde_json::from_value(serde_json::json!({
            "searchState": "Searching"
        }))
        .unwrap();
        assert_eq!(searching.elapsed_seconds(), Some(125));
        assert_eq!(found.elapsed_seconds(), None);
        assert_eq!(missing.elapsed_seconds(), None);
    }

    #[test]
    fn catalog_omits_duplicate_variants_without_portraits() {
        let summaries = vec![
            ChampionSummary {
                id: 103,
                name: "Ahri".into(),
            },
            ChampionSummary {
                id: 60103,
                name: "Ahri".into(),
            },
            ChampionSummary {
                id: 266,
                name: "Aatrox".into(),
            },
        ];
        let champions = catalog_entries(summaries, &HashMap::new());
        assert_eq!(
            champions.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![266, 103]
        );
    }

    #[test]
    fn ban_candidates_fall_back_to_primary_champions_when_lcu_list_is_empty() {
        let summaries = vec![
            ChampionSummary {
                id: 103,
                name: "Ahri".into(),
            },
            ChampionSummary {
                id: 60103,
                name: "Ahri".into(),
            },
            ChampionSummary {
                id: 266,
                name: "Aatrox".into(),
            },
        ];
        assert_eq!(fallback_ban_candidates(summaries), vec![103, 266]);
    }

    #[test]
    fn current_turn_allows_the_local_ban_and_prepares_a_later_pick() {
        let session: ChampionSession = serde_json::from_value(serde_json::json!({
            "gameId": 42,
            "localPlayerCellId": 2,
            "timer": {"phase": "BAN_PICK"},
            "actions": [
                [{"id": 1, "actorCellId": 3, "championId": 5, "completed": true, "type": "ban"}],
                [{"id": 2, "actorCellId": 2, "championId": 0, "completed": false, "type": "ban"}],
                [{"id": 3, "actorCellId": 2, "championId": 157, "completed": false, "type": "pick"}]
            ]
        }))
        .unwrap();
        assert_eq!(session.current_action().map(|action| action.id), Some(2));
        assert_eq!(session.prepick_action().map(|action| action.id), Some(3));
    }

    #[test]
    fn in_progress_ban_wins_over_an_earlier_unfinished_turn() {
        let session: ChampionSession = serde_json::from_value(serde_json::json!({
            "gameId": 42,
            "localPlayerCellId": 2,
            "timer": {"phase": "BAN_PICK"},
            "actions": [
                [{"id": 1, "actorCellId": 3, "championId": 0, "completed": false, "type": "ban", "isInProgress": false}],
                [{"id": 2, "actorCellId": 2, "championId": 0, "completed": false, "type": "ban", "isInProgress": true}]
            ]
        })).unwrap();
        assert_eq!(session.current_action().map(|action| action.id), Some(2));
    }

    #[test]
    fn planning_phase_has_no_current_action_but_can_pre_pick() {
        let session: ChampionSession = serde_json::from_value(serde_json::json!({
            "gameId": 42,
            "localPlayerCellId": 2,
            "timer": {"phase": "PLANNING"},
            "actions": [[{"id": 3, "actorCellId": 2, "championId": 0, "completed": false, "type": "pick"}]]
        })).unwrap();
        assert!(session.current_action().is_none());
        assert_eq!(session.prepick_action().map(|action| action.id), Some(3));
    }

    #[test]
    fn ready_check_cannot_be_accepted_twice() {
        let pending = ReadyCheck {
            state: "InProgress".into(),
            player_response: "None".into(),
        };
        let accepted = ReadyCheck {
            state: "InProgress".into(),
            player_response: "Accepted".into(),
        };
        assert!(ready_check_available(&pending));
        assert!(!ready_check_available(&accepted));
    }
}
