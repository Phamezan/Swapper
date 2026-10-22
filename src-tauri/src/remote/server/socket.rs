use super::super::RemoteCore;
use super::lan::LanSession;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use futures_util::StreamExt;
use std::sync::Arc;

pub(super) async fn socket(
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
    let mut items = crate::runes::subscribe_items_imported();
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
            imported = items.recv() => match imported {
                Ok(imported) => {
                    let payload = serde_json::json!({ "type": "itemsImported", "result": imported }).to_string();
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
