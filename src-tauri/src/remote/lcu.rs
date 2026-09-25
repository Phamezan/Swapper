use std::sync::Arc;
use std::time::Duration;
use futures_util::{FutureExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::lcu::{connect, discover, LcuEndpoint};

use super::RemoteCore;

pub async fn watch(core: Arc<RemoteCore>) {
    let mut socket: Option<WebSocketStream<MaybeTlsStream<TcpStream>>> = None;
    let mut connected_to: Option<LcuEndpoint> = None;
    let mut expected: Option<LcuEndpoint> = None;
    let mut league_running = false;
    let mut lcu_connected = false;
    loop {
        let endpoint = tokio::task::spawn_blocking(discover).await.unwrap_or(None);
        let running = endpoint.is_some();
        if running != league_running || endpoint != expected {
            league_running = running;
            expected = endpoint.clone();
            if endpoint.is_none() {
                drop(socket.take());
                connected_to = None;
                lcu_connected = false;
            }
            core.patch_league(running, lcu_connected);
        }
        if let Some(endpoint) = endpoint {
            let stale = connected_to.as_ref().is_some_and(|peer| *peer != endpoint);
            if socket.is_none() || stale {
                drop(socket.take());
                connected_to = None;
                lcu_connected = false;
                match connect(&endpoint).await {
                    Ok(active) => {
                        socket = Some(active);
                        connected_to = Some(endpoint.clone());
                        lcu_connected = true;
                        core.patch_league(true, true);
                    }
                    Err(_) => core.patch_league(true, false),
                }
            }
        }
        if let Some(active) = socket.as_mut() {
            let mut closed = false;
            while let Some(item) = active.next().now_or_never() {
                match item {
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => {
                        closed = true;
                        break;
                    }
                }
            }
            if closed {
                drop(socket.take());
                connected_to = None;
                if lcu_connected {
                    lcu_connected = false;
                    core.patch_league(true, false);
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
