//! Background champion-select watcher: emits `champ_select` events and applies
//! the top preset once per champion when auto-apply is on.
//!
//! The watcher polls quickly only inside champion select. Outside it — the
//! lobby, matchmaking, or League not running at all — it backs off so the idle
//! app is not doing work every few seconds.

use std::time::Duration;

use serde::Serialize;

use super::apply_top;

/// Champion select is short and reactive, so poll often there.
const ACTIVE_INTERVAL: Duration = Duration::from_secs(3);
/// Away from champion select there is nothing to auto-apply, so back off.
const IDLE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampSelectEvent {
    pub phase: String,
    pub champion_id: i64,
    pub champion_name: String,
    pub position: String,
    pub locked: bool,
}

/// Polls champion select and emits `champ_select` events, applying the top
/// preset once per champion when the auto-apply setting is on.
pub fn spawn_watch(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        let mut last: Option<ChampSelectEvent> = None;
        let mut applied_key: Option<String> = None;
        let mut interval = IDLE_INTERVAL;
        loop {
            tokio::time::sleep(interval).await;
            let (phase, context) = match super::rune_context().await {
                Ok(super::RuneContext { phase, context }) => (phase, context),
                Err(_) => {
                    // League is not connected; keep backing off.
                    interval = IDLE_INTERVAL;
                    continue;
                }
            };
            interval = if phase == "ChampSelect" {
                ACTIVE_INTERVAL
            } else {
                IDLE_INTERVAL
            };
            let mut event = ChampSelectEvent {
                phase: phase.clone(),
                champion_id: context.as_ref().map(|c| c.champion_id).unwrap_or(0),
                champion_name: context
                    .as_ref()
                    .map(|c| c.champion_name.clone())
                    .unwrap_or_default(),
                position: String::new(),
                locked: context.as_ref().map(|c| c.locked).unwrap_or(false),
            };
            if let Some(context) = context.as_ref() {
                event.position = context.position().unwrap_or("").to_string();
            }
            if last.as_ref() != Some(&event) {
                last = Some(event.clone());
                let _ = app.emit("champ_select", event.clone());
            }
            let ready = context.as_ref().is_some_and(|c| c.auto_apply_ready());
            if phase == "ChampSelect" && ready {
                if let Some(context) = context.as_ref() {
                    if auto_apply_enabled(&app) {
                        // One apply per champion. ARAM rerolls and bench swaps
                        // change the champion, so the key changes and the page
                        // is re-applied on the next poll (the poll is the
                        // debounce).
                        let key = format!("{}|{}", context.champion_id, context.map_id);
                        if applied_key.as_deref() != Some(key.as_str()) {
                            let owned = owned_page_id(&app);
                            if let Ok(Some(applied)) = apply_top(context, owned).await {
                                if let Some(id) = applied.page_id {
                                    persist_page_id(&app, id).await;
                                }
                                applied_key = Some(key);
                                let _ = app.emit("runes_changed", applied.clone());
                                use tauri::Manager;
                                if let Some(state) = app.try_state::<crate::AppState>() {
                                    state.publish_runes(applied);
                                }
                            }
                        }
                    }
                }
            } else if phase != "ChampSelect" {
                applied_key = None;
            }
        }
    });
}

pub fn auto_apply_enabled(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<crate::AppState>()
        .is_some_and(|state| state.auto_apply_top_preset())
}

fn owned_page_id(app: &tauri::AppHandle) -> Option<i64> {
    use tauri::Manager;
    app.try_state::<crate::AppState>()
        .and_then(|state| state.rune_page_id())
}

/// Saves the page id off the async runtime, because the vault write is
/// blocking.
async fn persist_page_id(app: &tauri::AppHandle, id: i64) {
    let app = app.clone();
    let _ = tokio::task::spawn_blocking(move || {
        use tauri::Manager;
        if let Some(state) = app.try_state::<crate::AppState>() {
            let _ = state.set_rune_page_id(id);
        }
    })
    .await;
}

/// The champion-select state the flyout uses to decide whether to show runes.
pub async fn current_status() -> ChampSelectEvent {
    match super::rune_context().await {
        Ok(super::RuneContext { phase, context }) => {
            let mut event = ChampSelectEvent {
                phase,
                champion_id: 0,
                champion_name: String::new(),
                position: String::new(),
                locked: false,
            };
            if let Some(context) = context {
                event.champion_id = context.champion_id;
                event.position = context.position().unwrap_or("").to_string();
                event.locked = context.locked;
                event.champion_name = context.champion_name;
            }
            if event.champion_id > 0 && event.champion_name.trim().is_empty() {
                if let Ok(names) = super::data::champion_names().await {
                    if let Some(name) = names.get(&event.champion_id) {
                        event.champion_name = name.clone();
                    }
                }
            }
            event
        }
        Err(_) => ChampSelectEvent {
            phase: "Unavailable".into(),
            champion_id: 0,
            champion_name: String::new(),
            position: String::new(),
            locked: false,
        },
    }
}
