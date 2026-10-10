//! Background champion-select watcher: emits `champ_select` events, applies
//! the recommended runes once per champion when auto-apply is on, and turns
//! gameflow phase transitions into lifecycle notifications.
//!
//! The watcher polls quickly only while something is waiting on the phase:
//! champion select, matchmaking, or a ready check (which expires in seconds).
//! Away from those — the lobby, or League not running at all — it backs off so
//! the idle app is not doing work every few seconds.

use std::sync::Mutex;
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
fn auto_apply_target(
    enabled: bool,
    ready: bool,
    key: &str,
    last: Option<&str>,
) -> bool {
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

/// Polls champion select and emits `champ_select` events, applying the top
/// preset once per champion when the auto-apply setting is on.
pub fn spawn_watch(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<ChampSelectEvent> = None;
        // The last gameflow phase, so lifecycle notifications fire once per
        // transition instead of per poll.
        let mut last_phase: Option<String> = None;
        let mut interval = IDLE_INTERVAL;
        loop {
            tokio::time::sleep(interval).await;
            let (phase, context) = match super::rune_context().await {
                Ok(super::RuneContext { phase, context }) => (phase, context),
                Err(_) => {
                    // League is not connected; keep backing off. last_phase is
                    // kept, so a reconnect cannot re-fire an old transition.
                    // The Korean font patcher is stopped so a closed client
                    // cannot leave it stranded.
                    crate::korean_font::observe(&app, "Offline");
                    interval = IDLE_INTERVAL;
                    continue;
                }
            };
            interval = if matches!(
                phase.as_str(),
                "ChampSelect" | "ReadyCheck" | "Matchmaking"
            ) {
                ACTIVE_INTERVAL
            } else {
                IDLE_INTERVAL
            };
            crate::lifecycle::observe(&app, last_phase.as_deref(), &phase);
            crate::korean_font::observe(&app, &phase);
            last_phase = Some(phase.clone());
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
            // Warm the runes and pro builds for the pick in the background, so
            // opening a tab during champion select does not wait on op.gg/u.gg.
            if let Some(context) = context.as_ref() {
                if context.champion_id > 0 {
                    super::prefetch::spawn(&app, context);
                }
            }
            let ready = context.as_ref().is_some_and(|c| c.auto_apply_ready());
            if phase == "ChampSelect" && ready {
                if let Some(context) = context.as_ref() {
                    // One apply per champion. ARAM rerolls and bench swaps
                    // change the champion, so the key changes and the page is
                    // re-applied on the next poll (the poll is the debounce).
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
            } else if phase != "ChampSelect" {
                set_applied_key(None);
            }
        }
    });
}

/// Applies the top preset for the current champion select when auto-apply is on
/// and the champion is ready, so turning the setting on while already locked
/// does not leave the user waiting for the next watcher poll. Returns the
/// applied page when an apply happened.
pub async fn auto_apply_current(app: &tauri::AppHandle) -> Option<AppliedView> {
    let super::RuneContext { phase, context } = super::rune_context().await.ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
