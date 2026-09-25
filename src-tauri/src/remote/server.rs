use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use futures_util::StreamExt;

use super::{control, RemoteCore};

pub fn router(core: Arc<RemoteCore>) -> Router {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/game", get(game))
        .route("/api/champions", get(champions))
        .route("/api/champion/icon/{id}", get(champion_icon))
        .route("/api/ready-check/accept", axum::routing::post(accept))
        .route("/api/champion/select", axum::routing::post(select_champion))
        .route(
            "/api/champion/prepick",
            axum::routing::post(prepick_champion),
        )
        .route("/api/champion/lock", axum::routing::post(lock_champion))
        .route("/ws", get(socket))
        .fallback(asset)
        .with_state(core)
}

async fn game() -> impl IntoResponse {
    Json(control::snapshot().await)
}

async fn champions() -> Response {
    match control::catalog().await {
        Ok(champions) => Json(champions).into_response(),
        Err(error) => (
            error.status,
            Json(control::ActionResult::error(&error.message)),
        )
            .into_response(),
    }
}

async fn champion_icon(Path(id): Path<i64>) -> Response {
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

fn action_allowed(headers: &HeaderMap) -> bool {
    headers
        .get("x-swapper-action")
        .is_some_and(|value| value == "1")
}

async fn accept(headers: HeaderMap) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    action_response(control::accept_ready_check().await)
}

async fn select_champion(
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

async fn prepick_champion(
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

async fn lock_champion(headers: HeaderMap) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    action_response(control::lock_champion().await)
}

fn action_response(result: Result<(), control::ActionError>) -> Response {
    match result {
        Ok(()) => (StatusCode::OK, Json(control::ActionResult::success())).into_response(),
        Err(error) => (
            error.status,
            Json(control::ActionResult::error(&error.message)),
        )
            .into_response(),
    }
}

async fn status(State(core): State<Arc<RemoteCore>>) -> Response {
    let body = serde_json::to_vec(&core.status()).unwrap_or_else(|_| b"{}".to_vec());
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

async fn socket(ws: WebSocketUpgrade, State(core): State<Arc<RemoteCore>>) -> impl IntoResponse {
    ws.on_upgrade(move |stream| serve_socket(stream, core))
}

async fn serve_socket(mut stream: WebSocket, core: Arc<RemoteCore>) {
    let mut updates = core.subscribe();
    let payload = serde_json::json!({ "type": "status", "status": core.status() }).to_string();
    if stream.send(Message::Text(payload.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            event = updates.recv() => match event {
                Ok(status) => {
                    let payload = serde_json::json!({ "type": "status", "status": status }).to_string();
                    if stream.send(Message::Text(payload.into())).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text.as_str()) {
                        if value.get("type").and_then(|kind| kind.as_str()) == Some("ping") {
                            let payload = serde_json::json!({ "type": "pong" }).to_string();
                            if stream.send(Message::Text(payload.into())).await.is_err() {
                                return;
                            }
                        }
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return,
            },
        }
    }
}

async fn asset(State(core): State<Arc<RemoteCore>>, req: Request) -> Response {
    let root = req.uri().path().trim_start_matches('/');
    let relative = if root.is_empty() { "remote.html" } else { root };
    let candidate = std::path::Path::new(relative);
    if candidate
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return not_found();
    }
    let resolved = candidate.to_string_lossy().replace('\\', "/");
    let resolver = core.app.asset_resolver();
    let asset = resolver.get(resolved.clone()).or_else(|| {
        if resolved.contains('.') {
            None
        } else {
            resolver.get(format!("{resolved}/remote.html"))
        }
    });
    let Some(asset) = asset else {
        return not_found();
    };
    let mut response = (
        [(header::CONTENT_TYPE, asset.mime_type.clone())],
        asset.bytes,
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    if let Some(csp) = asset.csp_header {
        if let Ok(value) = header::HeaderValue::from_str(&csp) {
            headers.insert(header::CONTENT_SECURITY_POLICY, value);
        }
    }
    response
}

fn not_found() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Swapper remote assets are missing. Run `npm run build` first, then restart Swapper.",
    )
        .into_response()
}
