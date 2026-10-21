use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

const PHASE_TOPIC: &str = "OnJsonApiEvent_lol-gameflow_v1_gameflow-phase";
const DRAFT_TOPIC: &str = "OnJsonApiEvent_lol-champ-select_v1_session";

pub(super) enum Event {
    Connected,
    Disconnected,
    Phase(String),
    Draft(Option<Value>),
}

fn decode(text: &str) -> Option<Event> {
    let message: Value = serde_json::from_str(text).ok()?;
    if message[0] != 8 {
        return None;
    }
    let event = &message[2];
    let kind = event["eventType"].as_str()?;
    if !matches!(kind, "Create" | "Update" | "Delete") {
        return None;
    }
    match (message[1].as_str()?, event["uri"].as_str()?) {
        (PHASE_TOPIC, super::super::PHASE_PATH) => {
            if kind == "Delete" {
                return Some(Event::Phase("None".into()));
            }
            let phase = event["data"].as_str()?;
            (!phase.is_empty() && phase.len() <= 64).then(|| Event::Phase(phase.into()))
        }
        (DRAFT_TOPIC, super::super::SESSION_PATH) => {
            if kind == "Delete" {
                return Some(Event::Draft(None));
            }
            event["data"]
                .is_object()
                .then(|| Event::Draft(Some(event["data"].clone())))
        }
        _ => None,
    }
}

async fn receive<S>(
    mut socket: WebSocketStream<S>,
    handle: &mut impl FnMut(Event),
) -> Result<(), ()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    for topic in [PHASE_TOPIC, DRAFT_TOPIC] {
        socket
            .send(Message::Text(
                serde_json::json!([5, topic]).to_string().into(),
            ))
            .await
            .map_err(|_| ())?;
    }
    handle(Event::Connected);
    while let Some(message) = socket.next().await {
        match message.map_err(|_| ())? {
            Message::Text(text) => {
                if let Some(event) = decode(&text) {
                    handle(event);
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn spawn(mut handle: impl FnMut(Event) + Send + 'static) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Some(endpoint) = tokio::task::spawn_blocking(crate::lcu::discover)
                .await
                .ok()
                .flatten()
            {
                if let Ok(Ok(socket)) =
                    tokio::time::timeout(Duration::from_secs(3), crate::lcu::connect(&endpoint))
                        .await
                {
                    let _ = receive(socket, &mut handle).await;
                }
            }
            handle(Event::Disconnected);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};

    #[tokio::test]
    async fn phase_and_draft_arrive_without_waiting_for_the_poll_timer() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let mut topics = Vec::new();
            for _ in 0..2 {
                let message = socket.next().await.unwrap().unwrap();
                let subscription: serde_json::Value =
                    serde_json::from_str(message.to_text().unwrap()).unwrap();
                assert_eq!(subscription[0], 5);
                topics.push(subscription[1].as_str().unwrap().to_string());
            }
            assert!(topics.contains(&"OnJsonApiEvent_lol-gameflow_v1_gameflow-phase".to_string()));
            assert!(topics.contains(&"OnJsonApiEvent_lol-champ-select_v1_session".to_string()));
            for message in [
                r#"[8,"OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",{"uri":"/lol-gameflow/v1/gameflow-phase","eventType":"Update","data":"GameStart"}]"#,
                r#"[8,"OnJsonApiEvent_lol-champ-select_v1_session",{"uri":"/lol-champ-select/v1/session","eventType":"Update","data":{"localPlayerCellId":0,"myTeam":[]}}]"#,
                r#"[8,"OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",{"uri":"/lol-gameflow/v1/gameflow-phase","eventType":"Update","data":"InProgress"}]"#,
            ] {
                socket.send(Message::Text(message.into())).await.unwrap();
            }
            socket.close(None).await.unwrap();
        });
        let (socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
        let mut received = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(1),
            receive(socket, &mut |event| received.push(event)),
        )
        .await
        .expect("League events must not wait for a 3/10-second poll")
        .unwrap();
        assert!(
            matches!(received.as_slice(), [Event::Connected, Event::Phase(loading), Event::Draft(Some(_)), Event::Phase(running)]
            if loading == "GameStart" && running == "InProgress")
        );
        server.await.unwrap();
    }

    #[test]
    fn unrelated_or_malformed_events_cannot_change_the_game_state() {
        for text in [
            "",
            "{}",
            "[8]",
            r#"[5,"OnJsonApiEvent",{}]"#,
            r#"[8,"OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",{"uri":"/unrelated","data":"InProgress"}]"#,
            r#"[8,"OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",{"uri":"/lol-gameflow/v1/gameflow-phase","eventType":"Update","data":{}}]"#,
        ] {
            assert!(decode(text).is_none(), "accepted {text}");
        }
        assert!(matches!(
            decode(
                r#"[8,"OnJsonApiEvent_lol-champ-select_v1_session",{"uri":"/lol-champ-select/v1/session","eventType":"Delete","data":{}}]"#
            ),
            Some(Event::Draft(None))
        ));
    }
}
