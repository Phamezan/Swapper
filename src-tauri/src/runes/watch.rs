//! Background champion-select watcher: emits `champ_select` events, applies
//! the recommended runes once per champion when auto-apply is on, and turns
//! gameflow phase transitions into lifecycle notifications.
//!
//! League pushes phase and draft changes over its local WebSocket. REST polling
//! recovers missed events and clients that do not support subscriptions.

mod events;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::Emitter;

use super::apply_top;
use super::session::ChampSelectContext;
use super::AppliedView;

/// Champion select is short and reactive, so poll often there.
const ACTIVE_INTERVAL: Duration = Duration::from_secs(3);
/// Away from champion select there is nothing to auto-apply, so back off.
const IDLE_INTERVAL: Duration = Duration::from_secs(10);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(30);

/// `champion|map` of the last auto-applied page, shared between the watcher and
/// the enable path so turning the setting on while already locked applies once
/// and the next poll does not apply again.
static APPLIED_KEY: Mutex<Option<String>> = Mutex::new(None);

fn applied_key() -> Option<String> {
    APPLIED_KEY
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn set_applied_key(key: Option<String>) {
    *APPLIED_KEY
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = key;
}

fn apply_key(context: &ChampSelectContext) -> String {
    format!("{}|{}", context.champion_id, context.map_id)
}

/// Whether an auto-apply should run for this state. Split out so the "enable
/// while already locked applies once" rule is testable without League.
fn auto_apply_target(enabled: bool, ready: bool, key: &str, last: Option<&str>) -> bool {
    enabled && ready && last != Some(key)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampSelectEvent {
    pub phase: String,
    pub champion_id: i64,
    pub champion_name: String,
    pub position: String,
    pub locked: bool,
}

fn replace_draft(
    last: &mut Option<crate::full_app::ui_data::Draft>,
    next: Option<crate::full_app::ui_data::Draft>,
) -> bool {
    if *last == next {
        return false;
    }
    *last = next;
    true
}

#[derive(Default)]
struct WatchState {
    phase: Option<String>,
    revision: u64,
    connected: bool,
    last: Option<ChampSelectEvent>,
    draft: Option<crate::full_app::ui_data::Draft>,
}

impl WatchState {
    fn set_phase(&mut self, phase: &str) -> bool {
        if self.phase.as_deref() == Some(phase) {
            return false;
        }
        self.phase = Some(phase.into());
        self.revision += 1;
        true
    }

    fn emit(&mut self, app: &tauri::AppHandle, event: ChampSelectEvent) -> bool {
        if self.last.as_ref() == Some(&event) {
            return false;
        }
        self.last = Some(event.clone());
        let _ = app.emit("champ_select", event);
        true
    }

    fn emit_draft(
        &mut self,
        app: &tauri::AppHandle,
        draft: Option<crate::full_app::ui_data::Draft>,
    ) {
        if replace_draft(&mut self.draft, draft) {
            let _ = app.emit("draft_changed", &self.draft);
        }
    }

    fn observe(&mut self, app: &tauri::AppHandle, phase: &str) {
        let previous = self.phase.clone();
        crate::korean_font::observe(app, phase);
        if !self.set_phase(phase) {
            return;
        }
        crate::lifecycle::observe(app, previous.as_deref(), phase);
        crate::live_game::observe(app, phase);
        crate::overlay::observe(app, phase);
        if phase != "ChampSelect" {
            self.emit_draft(app, None);
            set_applied_key(None);
        }
        self.emit(app, champ_event(phase, None));
    }
}

fn champ_event(phase: &str, context: Option<&ChampSelectContext>) -> ChampSelectEvent {
    ChampSelectEvent {
        phase: phase.into(),
        champion_id: context.map(|c| c.champion_id).unwrap_or(0),
        champion_name: context.map(|c| c.champion_name.clone()).unwrap_or_default(),
        position: context
            .and_then(ChampSelectContext::position)
            .unwrap_or("")
            .into(),
        locked: context.is_some_and(|c| c.locked),
    }
}

/// Watches champion select and emits `champ_select` events, applying the top
/// preset once per champion when the auto-apply setting is on.
pub fn spawn_watch(app: tauri::AppHandle) {
    let state = Arc::new(Mutex::new(WatchState::default()));
    let refresh = Arc::new(tokio::sync::Notify::new());
    let event_app = app.clone();
    let event_state = state.clone();
    let event_refresh = refresh.clone();
    events::spawn(move |event| {
        let mut state = event_state.lock().unwrap_or_else(|e| e.into_inner());
        match event {
            events::Event::Connected | events::Event::Disconnected => {
                let connected = matches!(event, events::Event::Connected);
                if connected {
                    crate::client_health::note_api(true);
                }
                if state.connected == connected {
                    return;
                }
                state.connected = connected;
                state.revision += 1;
            }
            events::Event::Phase(phase) => state.observe(&event_app, &phase),
            events::Event::Draft(draft) => {
                if state.phase.as_deref() != Some("ChampSelect") {
                    return;
                }
                state.revision += 1;
                let context = draft.as_ref().and_then(|value| {
                    super::session::parse_champ_select(&value.to_string())
                        .ok()
                        .flatten()
                });
                state.emit_draft(
                    &event_app,
                    draft.as_ref().map(crate::full_app::ui_data::draft_from),
                );
                if !state.emit(&event_app, champ_event("ChampSelect", context.as_ref())) {
                    return;
                }
            }
        }
        event_refresh.notify_one();
    });
    tauri::async_runtime::spawn(async move {
        loop {
            let revision = state.lock().unwrap_or_else(|e| e.into_inner()).revision;
            let result = super::rune_context().await;
            let context = {
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                // A push received during the REST read is newer than its result.
                if state.revision != revision {
                    continue;
                }
                // A live push connection means the API answers even if this read failed.
                crate::client_health::note_api(result.is_ok() || state.connected);
                match result {
                    Ok(super::RuneContext {
                        phase,
                        context,
                        draft,
                    }) => {
                        state.observe(&app, &phase);
                        state.emit_draft(&app, draft);
                        state.emit(&app, champ_event(&phase, context.as_ref()));
                        context
                    }
                    Err(_) => {
                        if !state.connected {
                            state.observe(&app, "Offline");
                        }
                        None
                    }
                }
            };
            // Warm the runes and pro builds for the pick in the background, so
            // opening a tab during champion select does not wait on op.gg/u.gg.
            if let Some(context) = context.as_ref() {
                if context.champion_id > 0 {
                    super::prefetch::spawn(&app, context);
                }
            }
            let ready = context.as_ref().is_some_and(|c| c.auto_apply_ready());
            if ready {
                if let Some(context) = context.as_ref() {
                    // One apply per champion, including ARAM rerolls and bench swaps.
                    let key = apply_key(context);
                    if auto_apply_target(
                        auto_apply_enabled(&app),
                        true,
                        &key,
                        applied_key().as_deref(),
                    ) {
                        let _ = apply_and_track(&app, context, key).await;
                    }
                }
            }
            let retry_apply = context.as_ref().is_some_and(|context| {
                auto_apply_target(
                    auto_apply_enabled(&app),
                    context.auto_apply_ready(),
                    &apply_key(context),
                    applied_key().as_deref(),
                )
            });
            let interval = {
                let state = state.lock().unwrap_or_else(|e| e.into_inner());
                if retry_apply {
                    ACTIVE_INTERVAL
                } else if state.connected {
                    RECONCILE_INTERVAL
                } else if matches!(
                    state.phase.as_deref(),
                    Some("ChampSelect" | "ReadyCheck" | "Matchmaking")
                ) {
                    ACTIVE_INTERVAL
                } else {
                    IDLE_INTERVAL
                }
            };
            tokio::select! {
                _ = refresh.notified() => {},
                _ = tokio::time::sleep(interval) => {},
            }
        }
    });
}

/// Applies the top preset for the current champion select when auto-apply is on
/// and the champion is ready, so turning the setting on while already locked
/// does not leave the user waiting for the next watcher poll. Returns the
/// applied page when an apply happened.
pub async fn auto_apply_current(app: &tauri::AppHandle) -> Option<AppliedView> {
    let super::RuneContext { phase, context, .. } = super::rune_context().await.ok()?;
    if phase != "ChampSelect" {
        return None;
    }
    let context = context?;
    let key = apply_key(&context);
    if !auto_apply_target(
        auto_apply_enabled(app),
        context.auto_apply_ready(),
        &key,
        applied_key().as_deref(),
    ) {
        return None;
    }
    apply_and_track(app, &context, key).await
}

/// Applies and records the result on every surface: the shared applied page
/// (flagged as an auto-apply), the persisted page id, the `runes_changed` event
/// and the phone websocket.
async fn apply_and_track(
    app: &tauri::AppHandle,
    context: &ChampSelectContext,
    key: String,
) -> Option<AppliedView> {
    let owned = owned_page_id(app);
    let applied = apply_top(
        context,
        owned,
        &configured_tier(app),
        apply_spells_enabled(app),
        import_items_enabled(app),
    )
    .await
    .ok()
    .flatten()?;
    if let Some(id) = applied.page_id {
        persist_page_id(app, id).await;
    }
    let mut applied = applied;
    applied.auto_applied = true;
    super::shared().applied = Some(applied.clone());
    set_applied_key(Some(key));
    let _ = app.emit("runes_changed", applied.clone());
    use tauri::Manager;
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.publish_runes(applied.clone());
    }
    Some(applied)
}

pub fn auto_apply_enabled(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<crate::AppState>()
        .is_some_and(|state| state.auto_apply_top_preset())
}

/// Whether applying a page should also set its summoner spells. Defaults to on
/// when the setting has never been changed.
pub fn apply_spells_enabled(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<crate::AppState>()
        .map(|state| state.apply_spells_with_runes())
        .unwrap_or(true)
}

pub fn import_items_enabled(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<crate::AppState>()
        .map(|state| state.import_items_with_runes())
        .unwrap_or(true)
}

/// The persisted op.gg rank bracket, used when the view or auto-apply loads
/// presets.
pub fn configured_tier(app: &tauri::AppHandle) -> String {
    use tauri::Manager;
    app.try_state::<crate::AppState>()
        .map(|state| state.rune_tier())
        .unwrap_or_else(|| super::opgg::DEFAULT_TIER.to_string())
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
        Ok(super::RuneContext { phase, context, .. }) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushed_phase_invalidates_a_slow_poll_without_repeating_transitions() {
        let mut state = WatchState::default();
        assert!(state.set_phase("ChampSelect"));
        let pending_poll = state.revision;
        assert!(state.set_phase("GameStart"));
        assert_ne!(
            pending_poll, state.revision,
            "old champion-select reads must be discarded"
        );
        let current = state.revision;
        assert!(!state.set_phase("GameStart"));
        assert_eq!(state.revision, current);
        assert!(state.set_phase("InProgress"));
        assert!(state.set_phase("EndOfGame"));
    }

    #[test]
    fn draft_events_follow_other_players_bans_and_timer_without_duplicates() {
        let base = serde_json::json!({"localPlayerCellId": 0,
            "myTeam": [{"cellId": 0, "championId": 103}, {"cellId": 1, "championId": 0}],
            "theirTeam": [{"cellId": 5, "championId": 0}], "actions": [],
            "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 20000}});
        let draft = |v: &serde_json::Value| Some(crate::full_app::ui_data::draft_from(v));
        let mut last = None;
        assert!(replace_draft(&mut last, draft(&base)));
        assert!(!replace_draft(&mut last, draft(&base)));
        let mut next = base.clone();
        next["theirTeam"][0]["championId"] = serde_json::json!(238);
        assert!(replace_draft(&mut last, draft(&next)));
        next["myTeam"][1]["championId"] = serde_json::json!(266);
        assert!(replace_draft(&mut last, draft(&next)));
        next["actions"] =
            serde_json::json!([[{"type": "ban", "championId": 11, "completed": true}]]);
        assert!(replace_draft(&mut last, draft(&next)));
        next["timer"]["phase"] = serde_json::json!("FINALIZATION");
        assert!(replace_draft(&mut last, draft(&next)));
        next["timer"]["adjustedTimeLeftInPhase"] = serde_json::json!(18000);
        assert!(replace_draft(&mut last, draft(&next)));
        assert!(!replace_draft(&mut last, draft(&next)));
        assert!(replace_draft(&mut last, None));
        assert!(!replace_draft(&mut last, None));
    }

    #[test]
    fn enabling_while_already_locked_applies_once() {
        let key = "103|11";
        // Turning the setting on with the champion already locked applies now.
        assert!(auto_apply_target(true, true, key, None));
        // The applied key is remembered, so the watcher's next poll is a no-op.
        assert!(!auto_apply_target(true, true, key, Some(key)));
        // A different champion or map is a fresh apply (the poll is the debounce).
        assert!(auto_apply_target(true, true, "266|11", Some(key)));
    }

    #[test]
    fn auto_apply_needs_the_setting_and_a_ready_champion() {
        // Off never applies, even when locked.
        assert!(!auto_apply_target(false, true, "103|11", None));
        // A hovered champion that is not locked never applies.
        assert!(!auto_apply_target(true, false, "103|11", None));
    }
}
