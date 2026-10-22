use super::super::control;
use axum::extract::Path;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

pub(super) async fn game() -> impl IntoResponse {
    Json(control::snapshot().await)
}

pub(super) async fn champions() -> Response {
    match control::catalog().await {
        Ok(champions) => Json(champions).into_response(),
        Err(error) => (
            error.status,
            Json(control::ActionResult::error(&error.message)),
        )
            .into_response(),
    }
}

pub(super) async fn champion_icon(Path(id): Path<i64>) -> Response {
    match control::icon(id).await {
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

pub(super) fn action_allowed(headers: &HeaderMap) -> bool {
    headers
        .get("x-swapper-action")
        .is_some_and(|value| value == "1")
}

pub(super) async fn accept(headers: HeaderMap) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    action_response(control::accept_ready_check().await)
}

pub(super) async fn select_champion(
    headers: HeaderMap,
    Json(body): Json<control::ChampionChoice>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    action_response(control::select_champion(body.champion_id).await)
}

pub(super) async fn prepick_champion(
    headers: HeaderMap,
    Json(body): Json<control::ChampionChoice>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    action_response(control::prepick_champion(body.champion_id).await)
}

pub(super) async fn lock_champion(headers: HeaderMap) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    action_response(control::lock_champion().await)
}

pub(super) fn action_response(result: Result<(), control::ActionError>) -> Response {
    match result {
        Ok(()) => (StatusCode::OK, Json(control::ActionResult::success())).into_response(),
        Err(error) => (
            error.status,
            Json(control::ActionResult::error(&error.message)),
        )
            .into_response(),
    }
}
