use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request, State};
use axum::Extension;
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use futures_util::StreamExt;
use serde::Deserialize;
use tauri::Emitter;

use super::{control, RemoteCore};

#[derive(Clone)]
struct LanSession(String);

pub fn router(core: Arc<RemoteCore>) -> Router {
    routes().with_state(core)
}

fn routes() -> Router<Arc<RemoteCore>> {
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
        .route("/api/runes", get(runes))
        .route("/api/runes/pro-builds", get(pro_builds))
        .route("/api/runes/keystone-build", get(keystone_build))
        .route("/api/items/import", axum::routing::post(import_item_build))
        .route("/api/runes/apply", axum::routing::post(apply_runes))
        .route(
            "/api/runes/auto-apply",
            axum::routing::post(set_auto_apply),
        )
        .route("/api/runes/tier", axum::routing::post(set_rune_tier))
        .route(
            "/api/runes/spells-setting",
            axum::routing::post(set_spells_with_runes),
        )
        .route("/api/runes/spells", axum::routing::post(apply_spells))
        .route("/api/rune/icon/{id}", get(rune_icon))
        .route("/api/role/icon/{role}", get(role_icon))
        .route("/api/spell/icon/{id}", get(spell_icon))
        .route("/api/item/icon/{id}", get(item_icon))
        .route("/api/rank/icon/{tier}", get(rank_icon))
        .route("/ws", get(socket))
        .fallback(asset)
}

pub fn lan_router(core: Arc<RemoteCore>) -> Router {
    routes()
        .route("/pair", get(pair_lan_device))
        .layer(middleware::from_fn_with_state(core.clone(), authorize_lan))
        .with_state(core)
}

async fn authorize_lan(
    State(core): State<Arc<RemoteCore>>,
    mut request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip());
    let allowed_peer = peer.is_some_and(|ip| {
        ip.is_loopback() || match ip {
            std::net::IpAddr::V4(address) => address.is_private(),
            std::net::IpAddr::V6(_) => false,
        }
    });
    if !allowed_peer {
        return StatusCode::NOT_FOUND.into_response();
    }
    if request.uri().path() == "/pair" {
        return next.run(request).await;
    }
    let session = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == "swapper_lan_session").then_some(value)
            })
        })
        .map(str::to_string);
    let Some(session) = session.filter(|session| core.has_lan_session(session)) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    request.extensions_mut().insert(LanSession(session));
    next.run(request).await
}

#[derive(Deserialize, Default)]
struct PairingQuery {
    token: Option<String>,
}

async fn pair_lan_device(
    State(core): State<Arc<RemoteCore>>,
    Query(query): Query<PairingQuery>,
) -> Response {
    let Some(token) = query.token else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(session) = core.pair_lan_device(&token) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = axum::response::Redirect::to("/").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!("swapper_lan_session={session}; HttpOnly; SameSite=Strict; Path=/")
            .parse()
            .expect("valid pairing cookie"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        "no-referrer".parse().expect("valid referrer policy"),
    );
    response
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

#[derive(Deserialize, Default)]
struct PositionQuery {
    position: Option<String>,
}

async fn runes(
    State(core): State<Arc<RemoteCore>>,
    Query(query): Query<PositionQuery>,
) -> Response {
    let auto_apply = crate::runes::auto_apply_enabled(&core.app);
    let apply_spells = crate::runes::apply_spells_enabled(&core.app);
    let tier = crate::runes::configured_tier(&core.app);
    Json(crate::runes::view(auto_apply, apply_spells, &tier, query.position.as_deref()).await).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProBuildsQuery {
    champion_id: i64,
    #[serde(default)]
    position: String,
    #[serde(default)]
    page: Option<u32>,
}

async fn pro_builds(Query(query): Query<ProBuildsQuery>) -> Response {
    Json(crate::runes::pro_builds_view(
        query.champion_id,
        &query.position,
        query.page.unwrap_or(1),
    )
    .await)
        .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeystoneBuildQuery {
    champion_id: i64,
    #[serde(default)]
    position: String,
    #[serde(default)]
    tier: String,
    keystone: i64,
}

async fn keystone_build(Query(query): Query<KeystoneBuildQuery>) -> Response {
    Json(
        crate::runes::preset_build_view(query.champion_id, &query.position, &query.tier, query.keystone)
            .await,
    )
    .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportItemBuildRequest {
    champion_id: i64,
    champion_name: String,
    source: String,
    items: Vec<i64>,
}

async fn import_item_build(headers: HeaderMap, Json(body): Json<ImportItemBuildRequest>) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    match crate::runes::item_sets::import_build(
        body.champion_id,
        &body.champion_name,
        &body.source,
        &body.items,
    )
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
struct ApplyRunesRequest {
    selection: crate::runes::RuneSelection,
    #[serde(default)]
    preset_index: Option<usize>,
    #[serde(default)]
    spells: Option<Vec<i64>>,
}

async fn apply_runes(
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
    let spells = body.spells.as_deref().and_then(crate::runes::spells::pair_from_ids);
    match crate::runes::apply_selection(body.selection, body.preset_index, owned, spells, apply_spells)
        .await
    {
        Ok(applied) => {
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
struct AutoApplyRequest {
    enabled: bool,
}

async fn set_auto_apply(
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
struct TierRequest {
    tier: String,
}

async fn set_rune_tier(
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

async fn rune_icon(Path(id): Path<i64>) -> Response {
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

async fn role_icon(Path(role): Path<String>) -> Response {
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

async fn spell_icon(Path(id): Path<i64>) -> Response {
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
async fn item_icon(Path(id): Path<i64>) -> Response {
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
async fn rank_icon(Path(tier): Path<String>) -> Response {
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpellRequest {
    slot: String,
    spell_id: i64,
}

async fn apply_spells(
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
struct SpellsSettingRequest {
    enabled: bool,
}

async fn set_spells_with_runes(
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

async fn socket(
    ws: WebSocketUpgrade,
    State(core): State<Arc<RemoteCore>>,
    lan_session: Option<Extension<LanSession>>,
) -> impl IntoResponse {
    let session = lan_session.map(|Extension(LanSession(session))| session);
    ws.on_upgrade(move |stream| serve_socket(stream, core, session))
}

async fn serve_socket(mut stream: WebSocket, core: Arc<RemoteCore>, lan_session: Option<String>) {
    let mut updates = core.subscribe();
    let mut runes = core.subscribe_runes();
    let mut runes_changed = core.subscribe_runes_changed();
    let payload = serde_json::json!({ "type": "status", "status": core.status() }).to_string();
    if stream.send(Message::Text(payload.into())).await.is_err() {
        return;
    }
    let mut session_check = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = session_check.tick() => {
                if lan_session.as_deref().is_some_and(|session| !core.has_lan_session(session)) {
                    let _ = stream.send(Message::Close(None)).await;
                    return;
                }
            },
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
            applied = runes.recv() => match applied {
                Ok(applied) => {
                    let payload = serde_json::json!({ "type": "runes", "applied": applied }).to_string();
                    if stream.send(Message::Text(payload.into())).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            },
            _ = runes_changed.recv() => {
                let payload = serde_json::json!({ "type": "runes" }).to_string();
                if stream.send(Message::Text(payload.into())).await.is_err() {
                    return;
                }
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
    // In development the frontend is served live by Vite: `cargo` does not
    // rebuild when only `dist/` changes, so the embedded assets go stale during
    // `tauri dev`. Release builds keep serving the embedded assets below.
    #[cfg(debug_assertions)]
    {
        let target = match req.uri().query() {
            Some(query) => format!("/{resolved}?{query}"),
            None => format!("/{resolved}"),
        };
        if let Some(response) = dev_asset(&core, &target, req.headers()).await {
            return response;
        }
    }
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

/// Proxies an asset request to the Vite dev server named by the Tauri
/// `devUrl`, so the phone always gets the live frontend during `tauri dev`.
/// Returns `None` when Vite is unreachable, is not a dev build, or has no
/// matching asset, so the caller can fall back to the embedded assets.
#[cfg(debug_assertions)]
async fn dev_asset(core: &RemoteCore, target: &str, request: &HeaderMap) -> Option<Response> {
    let dev_url = core.app.config().build.dev_url.clone()?;
    let base = dev_url.as_str().trim_end_matches('/');
    fetch_dev_asset(base, target, request).await
}

// Vite picks the response format from these request headers: a `.css` import
// fetched as a module script must come back as JavaScript, while a stylesheet
// link gets plain CSS. Forward the browser's own values instead of guessing.
#[cfg(debug_assertions)]
const DEV_FORWARDED_HEADERS: [&str; 3] = ["accept", "sec-fetch-dest", "sec-fetch-mode"];

/// Fetches one asset from the Vite dev server and wraps it as a no-store
/// response. Split from [`dev_asset`] so it can be exercised against a running
/// `npm run dev` without a Tauri app.
#[cfg(debug_assertions)]
async fn fetch_dev_asset(base: &str, target: &str, request: &HeaderMap) -> Option<Response> {
    const DEV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
    let client = reqwest::Client::builder()
        .connect_timeout(DEV_TIMEOUT)
        .timeout(DEV_TIMEOUT)
        .build()
        .ok()?;
    let mut upstream = client.get(format!("{base}{target}"));
    for name in DEV_FORWARDED_HEADERS {
        if let Some(value) = request.get(name).and_then(|value| value.to_str().ok()) {
            upstream = upstream.header(name, value);
        }
    }
    let upstream = upstream.send().await.ok()?;
    if !upstream.status().is_success() {
        return None;
    }
    let status = StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::OK);
    let content_type = upstream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = upstream.bytes().await.ok()?.to_vec();
    let mut response = (status, [(header::CONTENT_TYPE, content_type)], bytes).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    Some(response)
}

fn not_found() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Swapper remote assets are missing. Run `npm run build` first, then restart Swapper.",
    )
        .into_response()
}

#[cfg(all(test, debug_assertions))]
mod dev_proxy_tests {
    use super::fetch_dev_asset;
    use axum::http::{HeaderMap, HeaderValue};

    fn module_request() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("*/*"));
        headers.insert("sec-fetch-dest", HeaderValue::from_static("script"));
        headers
    }

    /// Exercises the dev proxy against a live `npm run dev`. Ignored by default
    /// because it needs the Vite server; run it with
    /// `cargo test --manifest-path src-tauri/Cargo.toml dev_proxy -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs `npm run dev` on http://localhost:1420"]
    async fn remote_html_and_its_module_graph_come_from_vite() {
        let base =
            std::env::var("SWAPPER_DEV_URL").unwrap_or_else(|_| "http://localhost:1420".into());
        let html = fetch_dev_asset(&base, "/remote.html", &HeaderMap::new())
            .await
            .expect("Vite should serve remote.html");
        let body = axum::body::to_bytes(html.into_body(), 1 << 20).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("<div id=\"root\">"), "unexpected HTML: {text}");
        assert!(text.contains("/src/remote/main.tsx"), "no module entry: {text}");

        let module = fetch_dev_asset(&base, "/src/remote/main.tsx", &module_request())
            .await
            .expect("Vite should serve the module graph");
        let module_body = axum::body::to_bytes(module.into_body(), 1 << 20)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&module_body).contains("createRoot"));
    }

    /// A CSS file imported from a module must come back as JavaScript, or the
    /// browser rejects the module and the phone page never mounts.
    #[tokio::test]
    #[ignore = "needs `npm run dev` on http://localhost:1420"]
    async fn css_imported_by_a_module_is_served_as_javascript() {
        let base =
            std::env::var("SWAPPER_DEV_URL").unwrap_or_else(|_| "http://localhost:1420".into());
        let css = fetch_dev_asset(&base, "/src/remote/remote.css", &module_request())
            .await
            .expect("Vite should serve the stylesheet module");
        let content_type = css
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.contains("javascript"), "got {content_type}");
    }
}
