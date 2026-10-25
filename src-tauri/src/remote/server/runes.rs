use super::super::{control, RemoteCore};
use super::actions::action_allowed;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use tauri::Emitter;

#[derive(Deserialize, Default)]
pub(super) struct PositionQuery {
    position: Option<String>,
}

pub(super) async fn runes(
    State(core): State<Arc<RemoteCore>>,
    Query(query): Query<PositionQuery>,
) -> Response {
    let auto_apply = crate::runes::auto_apply_enabled(&core.app);
    let apply_spells = crate::runes::apply_spells_enabled(&core.app);
    let tier = crate::runes::configured_tier(&core.app);
    let mut view =
        crate::runes::view(auto_apply, apply_spells, &tier, query.position.as_deref()).await;
    view.import_items = crate::runes::import_items_enabled(&core.app);
    Json(view).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProBuildsQuery {
    champion_id: i64,
    #[serde(default)]
    position: String,
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    is_otp: bool,
}

pub(super) async fn pro_builds(Query(query): Query<ProBuildsQuery>) -> Response {
    Json(
        crate::runes::pro_builds_view(
            query.champion_id,
            &query.position,
            query.page.unwrap_or(1),
            query.is_otp,
        )
        .await,
    )
    .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct KeystoneBuildQuery {
    champion_id: i64,
    #[serde(default)]
    position: String,
    #[serde(default)]
    tier: String,
    keystone: i64,
    #[serde(default)]
    most_played: Option<String>,
}

pub(super) async fn keystone_build(Query(query): Query<KeystoneBuildQuery>) -> Response {
    let most_played = match query
        .most_played
        .as_deref()
        .map(serde_json::from_str::<crate::runes::RuneSelection>)
        .transpose()
    {
        Ok(selection) => selection,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    Json(
        crate::runes::preset_build_view(
            query.champion_id,
            &query.position,
            &query.tier,
            query.keystone,
            most_played.as_ref(),
        )
        .await,
    )
    .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MatchupQuery {
    champion_id: i64,
    enemy_champion_id: i64,
    #[serde(default)]
    position: String,
    #[serde(default)]
    tier: String,
}

pub(super) async fn matchup(Query(query): Query<MatchupQuery>) -> Response {
    Json(
        crate::runes::matchup_view(
            query.champion_id,
            query.enemy_champion_id,
            &query.position,
            &query.tier,
        )
        .await,
    )
    .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CountersQuery {
    champion_id: i64,
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    tier: String,
}

pub(super) async fn champion_counters(Query(query): Query<CountersQuery>) -> Response {
    Json(
        crate::runes::champion_counters_view(
            query.champion_id,
            query.position.as_deref(),
            &query.tier,
        )
        .await,
    )
    .into_response()
}

pub(super) async fn champion_overview(Query(query): Query<CountersQuery>) -> Response {
    Json(
        crate::runes::champion_overview_view(
            query.champion_id,
            query.position.as_deref(),
            &query.tier,
        )
        .await,
    )
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct TierListQuery {
    #[serde(default)]
    position: String,
    #[serde(default)]
    tier: String,
}

pub(super) async fn tier_list(Query(query): Query<TierListQuery>) -> Response {
    Json(crate::runes::tier_list_view(&query.position, &query.tier).await).into_response()
}

pub(super) async fn duo_list(Query(query): Query<TierListQuery>) -> Response {
    Json(crate::runes::duo_list_view(&query.tier).await).into_response()
}

pub(super) async fn champion_list() -> Response {
    Json(crate::runes::champion_list().await).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ImportItemBuildRequest {
    champion_id: i64,
    champion_name: String,
    build: crate::runes::KeystoneBuildView,
}

const MAX_ITEM_IMPORT_BYTES: usize = 64 * 1024;

pub(super) async fn import_item_build(
    headers: HeaderMap,
    request: Request,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    let bytes = match axum::body::to_bytes(request.into_body(), MAX_ITEM_IMPORT_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let body: ImportItemBuildRequest = match serde_json::from_slice(&bytes) {
        Ok(body) => body,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    match crate::runes::item_sets::import_keystone_build(body.champion_id, &body.champion_name, &body.build)
        .await
    {
        Ok(_) => (StatusCode::OK, Json(control::ActionResult::success())).into_response(),
        Err(error) => (
            rune_status(&error),
            Json(control::ActionResult::error(error.message())),
        )
            .into_response(),
    }
}

fn rune_status(error: &crate::runes::RuneError) -> StatusCode {
    match error {
        crate::runes::RuneError::NotFound(_) => StatusCode::NOT_FOUND,
        crate::runes::RuneError::Conflict(_) => StatusCode::CONFLICT,
        crate::runes::RuneError::Unavailable(_) => StatusCode::BAD_GATEWAY,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ApplyRunesRequest {
    selection: crate::runes::RuneSelection,
    #[serde(default)]
    preset_index: Option<usize>,
    #[serde(default)]
    spells: Option<Vec<i64>>,
    /// The role the preset was loaded for, so its item build matches.
    #[serde(default)]
    position: Option<String>,
    /// The enemy laner of a matchup build, so its item build is imported.
    #[serde(default)]
    enemy_champion_id: Option<i64>,
    #[serde(default)]
    highest_win: bool,
}

pub(super) async fn apply_runes(
    State(core): State<Arc<RemoteCore>>,
    headers: HeaderMap,
    Json(body): Json<ApplyRunesRequest>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    use tauri::Manager;
    let (owned, apply_spells) = core
        .app
        .try_state::<crate::AppState>()
        .map(|state| (state.rune_page_id(), state.apply_spells_with_runes()))
        .unwrap_or((None, true));
    let spells = body
        .spells
        .as_deref()
        .and_then(crate::runes::spells::pair_from_ids);
    let keystone = body.selection.keystone;
    match crate::runes::apply_selection(
        body.selection,
        body.preset_index,
        owned,
        spells,
        apply_spells,
    )
    .await
    {
        Ok(applied) => {
            let wants_items =
                body.preset_index.is_some() || body.enemy_champion_id.is_some() || body.highest_win;
            if wants_items && crate::runes::import_items_enabled(&core.app) {
                crate::runes::spawn_preset_items_import(
                    body.position,
                    crate::runes::configured_tier(&core.app),
                    keystone,
                    body.enemy_champion_id,
                    body.highest_win,
                );
            }
            if let Some(id) = applied.page_id {
                persist_rune_page_id(core.app.clone(), id).await;
            }
            core.publish_runes(applied.clone());
            let _ = core.app.emit("runes_changed", applied);
            (StatusCode::OK, Json(control::ActionResult::success())).into_response()
        }
        Err(error) => (
            rune_status(&error),
            Json(control::ActionResult::error(error.message())),
        )
            .into_response(),
    }
}

/// Saves the owned rune page id without blocking the axum runtime: the vault
/// write is synchronous file I/O.
async fn persist_rune_page_id(app: tauri::AppHandle, id: i64) {
    use tauri::Manager;
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(state) = app.try_state::<crate::AppState>() {
            let _ = state.set_rune_page_id(id);
        }
    })
    .await;
}

#[derive(Deserialize)]
pub(super) struct AutoApplyRequest {
    enabled: bool,
}

pub(super) async fn set_auto_apply(
    State(core): State<Arc<RemoteCore>>,
    headers: HeaderMap,
    Json(body): Json<AutoApplyRequest>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    let app = core.app.clone();
    let enabled = body.enabled;
    // The vault save is blocking file I/O; keep it off the axum runtime.
    let result = tokio::task::spawn_blocking(move || {
        use tauri::Manager;
        let state = app
            .try_state::<crate::AppState>()
            .ok_or_else(|| "Swapper is unavailable.".to_string())?;
        state.set_auto_apply_top_preset(enabled)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()));
    match result {
        Ok(()) => {
            // Turning the setting on with the champion already locked applies
            // now, instead of waiting for the watcher's next poll.
            if enabled {
                let app = core.app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::runes::auto_apply_current(&app).await;
                });
            }
            (StatusCode::OK, Json(control::ActionResult::success())).into_response()
        }
        Err(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(control::ActionResult::error(&message)),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub(super) struct TierRequest {
    tier: String,
}

pub(super) async fn set_rune_tier(
    State(core): State<Arc<RemoteCore>>,
    headers: HeaderMap,
    Json(body): Json<TierRequest>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    let app = core.app.clone();
    let tier = body.tier.clone();
    // The vault save is blocking file I/O; keep it off the axum runtime.
    let result = tokio::task::spawn_blocking(move || {
        use tauri::Manager;
        let state = app
            .try_state::<crate::AppState>()
            .ok_or_else(|| "Swapper is unavailable.".to_string())?;
        state.set_rune_tier(&tier)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()));
    match result {
        Ok(()) => {
            // Let the desktop flyout and the phone reload with the new bracket.
            core.notify_runes_changed();
            let _ = core.app.emit("runes_changed", serde_json::Value::Null);
            (StatusCode::OK, Json(control::ActionResult::success())).into_response()
        }
        Err(message) => (
            StatusCode::BAD_REQUEST,
            Json(control::ActionResult::error(&message)),
        )
            .into_response(),
    }
}

pub(super) async fn rune_icon(Path(id): Path<i64>) -> Response {
    match crate::runes::icon(id).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "private, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn role_icon(Path(role): Path<String>) -> Response {
    match crate::runes::roles::icon(&role).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "private, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn spell_icon(Path(id): Path<i64>) -> Response {
    match crate::runes::spells::icon(id).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "private, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Item icons are addressed by a numeric id only; axum rejects anything else
/// with a 400 before this handler runs, and the module rejects non-positive ids.
pub(super) async fn item_icon(Path(id): Path<i64>) -> Response {
    match crate::runes::items::icon(id).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "private, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Rank crests are addressed by an op.gg bracket slug; the module whitelists the
/// few known slugs, so no caller-supplied path reaches the client or network.
pub(super) async fn rank_icon(Path(tier): Path<String>) -> Response {
    match crate::runes::ranks::icon(&tier).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "private, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Team logos are addressed by the team's display name; the module slugifies it
/// and only ever fetches from the one fixed probuildstats host.
pub(super) async fn team_icon(Path(team): Path<String>) -> Response {
    match crate::runes::teams::icon(&team).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "private, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SpellRequest {
    slot: String,
    spell_id: i64,
}

pub(super) async fn apply_spells(
    State(core): State<Arc<RemoteCore>>,
    headers: HeaderMap,
    Json(body): Json<SpellRequest>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    let slot = if body.slot.eq_ignore_ascii_case("d") || body.slot == "1" {
        0
    } else {
        1
    };
    match crate::runes::spells::apply_pick(slot, body.spell_id).await {
        Ok(()) => {
            core.notify_runes_changed();
            let _ = core.app.emit("runes_changed", serde_json::Value::Null);
            (StatusCode::OK, Json(control::ActionResult::success())).into_response()
        }
        Err(error) => (
            rune_status(&error),
            Json(control::ActionResult::error(error.message())),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub(super) struct SpellsSettingRequest {
    enabled: bool,
}

pub(super) async fn set_spells_with_runes(
    State(core): State<Arc<RemoteCore>>,
    headers: HeaderMap,
    Json(body): Json<SpellsSettingRequest>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    let app = core.app.clone();
    let enabled = body.enabled;
    let result = tokio::task::spawn_blocking(move || {
        use tauri::Manager;
        let state = app
            .try_state::<crate::AppState>()
            .ok_or_else(|| "Swapper is unavailable.".to_string())?;
        state.set_apply_spells_with_runes(enabled)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()));
    match result {
        Ok(()) => {
            core.notify_runes_changed();
            let _ = core.app.emit("runes_changed", serde_json::Value::Null);
            (StatusCode::OK, Json(control::ActionResult::success())).into_response()
        }
        Err(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(control::ActionResult::error(&message)),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub(super) struct ImportItemsSettingRequest {
    enabled: bool,
}

pub(super) async fn set_import_items_with_runes(
    State(core): State<Arc<RemoteCore>>,
    headers: HeaderMap,
    Json(body): Json<ImportItemsSettingRequest>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    let app = core.app.clone();
    let enabled = body.enabled;
    let result = tokio::task::spawn_blocking(move || {
        use tauri::Manager;
        let state = app
            .try_state::<crate::AppState>()
            .ok_or_else(|| "Swapper is unavailable.".to_string())?;
        state.set_import_items_with_runes(enabled)
    })
    .await
    .unwrap_or_else(|error| Err(error.to_string()));
    match result {
        Ok(()) => {
            core.notify_runes_changed();
            let _ = core.app.emit("runes_changed", serde_json::Value::Null);
            (StatusCode::OK, Json(control::ActionResult::success())).into_response()
        }
        Err(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(control::ActionResult::error(&message)),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::Request;
    use axum::handler::Handler;
    use serde_json::{json, Value};

    fn item_import() -> Value {
        json!({
            "championId": 266, "championName": "Aatrox",
            "build": {
                "starters": [{ "id": 1054, "name": "Shield", "count": 1 }, { "id": 2003, "name": "Potion", "count": 2 }],
                "items": [{ "id": 6692, "name": "Eclipse" }],
                "options": [{ "id": 3053, "name": "Sterak", "slot": 4, "games": 80, "winPct": 52.0 }],
                "games": 100, "coreGames": 100, "coreLen": 1, "coreWinPct": 51.0,
                "skillPriority": "QWE"
            }
        })
    }

    async fn import_request(body: String, allowed: bool) -> Response {
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/items/import")
            .header(header::CONTENT_TYPE, "application/json");
        if allowed {
            request = request.header("x-swapper-action", "1");
        }
        import_item_build.call(request.body(Body::from(body)).unwrap(), ()).await
    }

    #[test]
    fn item_import_deserializes_a_full_build() {
        let body: ImportItemBuildRequest = serde_json::from_value(item_import()).unwrap();
        assert_eq!(body.build.starters[1].count, 2);
        assert_eq!(body.build.items[0].id, 6692);
        assert_eq!(body.build.options[0].id, 3053);
        assert_eq!(body.build.skill_priority.as_deref(), Some("QWE"));
    }

    #[tokio::test]
    async fn item_import_requires_the_action_header() {
        let response = import_request(item_import().to_string(), false).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn item_import_rejects_an_oversized_body() {
        let mut body = item_import();
        body["championName"] = json!("a".repeat(64 * 1024));
        assert_eq!(import_request(body.to_string(), true).await.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn item_import_rejects_invalid_builds_before_contacting_league() {
        let valid = item_import();
        let mut invalid = Vec::new();
        for field in ["items", "starters"] {
            for id in [0, -1] {
                let mut body = valid.clone();
                body["build"][field][0]["id"] = json!(id);
                invalid.push(body);
            }
        }
        for count in [0, 21] {
            let mut body = valid.clone();
            body["build"]["starters"][0]["count"] = json!(count);
            invalid.push(body);
        }
        for (field, count) in [("items", 0), ("items", 8), ("starters", 8), ("options", 257)] {
            let mut body = valid.clone();
            body["build"][field] = json!(vec![valid["build"][field][0].clone(); count]);
            invalid.push(body);
        }
        let mut body = valid.clone();
        body["championId"] = json!(0);
        invalid.push(body);
        for body in invalid {
            assert_eq!(import_request(body.to_string(), true).await.status(), StatusCode::CONFLICT, "{body}");
        }
    }

    #[tokio::test]
    async fn item_import_rejects_malformed_and_legacy_payloads() {
        // An invalid champion also prevents writes when this test runs against the legacy handler.
        for body in ["{".to_string(), json!({ "championId": 0, "championName": "Invalid", "items": [6692] }).to_string()] {
            assert_eq!(import_request(body, true).await.status(), StatusCode::BAD_REQUEST);
        }
    }
}
