mod live_client;
mod model;
#[cfg(test)]
mod probe;
mod render;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{Manager, State};
use windows_sys::Win32::UI::HiDpi::{
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub enabled: bool,
    pub gold: bool,
    pub objectives: bool,
    pub skills: bool,
    pub gold_hotkey: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            gold: true,
            objectives: true,
            skills: true,
            gold_hotkey: 9,
        }
    }
}

impl Settings {
    fn active(self) -> bool {
        self.enabled && (self.gold || self.objectives || self.skills)
    }
    fn validate(self) -> Result<(), String> {
        if matches!(self.gold_hotkey,9|16..=18|112..=123) {
            Ok(())
        } else {
            Err("Choose Tab, Shift, Ctrl, Alt or F1–F12 for overlay gold.".into())
        }
    }
}

#[derive(Default)]
pub struct Controller(Mutex<Option<Worker>>);

struct Worker {
    sender: tokio::sync::mpsc::UnboundedSender<Option<Settings>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn start(settings: Settings, tier: String) -> Result<Self, String> {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let (ready, started) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("swapper-overlay".into())
            .spawn(move || {
                let setup = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string());
                match setup {
                    Ok(runtime) => {
                        let _ = ready.send(Ok(()));
                        unsafe {
                            SetThreadDpiAwarenessContext(
                                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                            );
                        }
                        runtime.block_on(run(settings, tier, receiver));
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let worker = Self {
            sender,
            thread: Some(thread),
        };
        started.recv().map_err(|e| e.to_string())??;
        Ok(worker)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.sender.send(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Controller {
    fn apply(worker: &mut Option<Worker>, settings: Settings, tier: &str) -> Result<(), String> {
        if !settings.active() {
            worker.take();
            return Ok(());
        }
        if worker
            .as_ref()
            .is_some_and(|w| w.thread.as_ref().is_some_and(JoinHandle::is_finished))
        {
            worker.take();
        }
        if let Some(worker) = worker {
            worker
                .sender
                .send(Some(settings))
                .map_err(|e| e.to_string())?;
        } else {
            *worker = Some(Worker::start(settings, tier.to_string())?);
        }
        Ok(())
    }
}

pub fn init(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let mut worker = None;
    // The only spawn is below the active gate. Disabled startup allocates just
    // this empty mutex: no runtime, client, timer, native resources or thread.
    Controller::apply(
        &mut worker,
        config.overlay,
        config.rune_tier.as_deref().unwrap_or("emerald_plus"),
    )?;
    app.manage(Controller(Mutex::new(worker)));
    Ok(())
}

#[tauri::command]
pub fn get_overlay_settings(state: State<'_, crate::AppState>) -> Result<Settings, String> {
    Ok(state.config.lock().map_err(|e| e.to_string())?.overlay)
}

#[tauri::command]
pub fn set_overlay_settings(
    state: State<'_, crate::AppState>,
    overlay: State<'_, Controller>,
    settings: Settings,
) -> Result<Settings, String> {
    settings.validate()?;
    let mut worker = overlay.0.lock().map_err(|e| e.to_string())?;
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let previous = config.overlay;
    let tier = config.rune_tier.as_deref().unwrap_or("emerald_plus");
    Controller::apply(&mut worker, settings, tier)?;
    let mut saved = config.clone();
    saved.overlay = settings;
    if let Err(error) = crate::vault::save(&saved) {
        let _ = Controller::apply(&mut worker, previous, tier);
        return Err(error);
    }
    *config = saved;
    Ok(settings)
}

fn read_priority(data: &Value, tier: &str) -> Option<String> {
    let player = model::active_player(data)?;
    let champion = player["rawChampionName"]
        .as_str()
        .and_then(|name| name.strip_prefix("game_character_displayname_"))
        .or_else(|| player["championName"].as_str())?;
    // Nonstandard rank systems need champion-specific rules. Never guess.
    if ["Aphelios", "Elise", "Jayce", "Karma", "Nidalee", "Udyr"]
        .iter()
        .any(|name| champion.eq_ignore_ascii_case(name))
    {
        return None;
    }
    let path = crate::runes::cache::cache_path(crate::runes::provider::DataKind::ItemBuild)?;
    if std::fs::metadata(&path).ok()?.len() > 2 * 1024 * 1024 {
        return None;
    }
    let cache: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    model::cached_priority(&cache, champion, player["position"].as_str()?, tier)
}

fn display(
    data: &Value,
    settings: Settings,
    priority: Option<&str>,
    held: bool,
) -> model::DisplayData {
    let ranks = ['Q', 'W', 'E', 'R']
        .map(|key| data["activePlayer"]["abilities"][key.to_string()]["abilityLevel"].as_u64());
    let skill = if settings.skills {
        ranks
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|ranks| {
                model::next_skill(
                    data["activePlayer"]["level"].as_u64()?,
                    ranks.try_into().ok()?,
                    priority?,
                )
            })
    } else {
        None
    };
    model::DisplayData {
        gold: (settings.gold && held).then(|| model::gold(data)).flatten(),
        objectives: if settings.objectives {
            model::objectives(data)
        } else {
            Vec::new()
        },
        skill,
    }
}

async fn run(
    mut settings: Settings,
    tier: String,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Option<Settings>>,
) {
    let mut next_process = Instant::now();
    let mut next_fetch = Instant::now();
    let mut pids = Vec::new();
    let mut fetch: Option<tokio::task::JoinHandle<Result<Value, String>>> = None;
    let mut scope: Option<tokio::task::JoinHandle<bool>> = None;
    let mut allowed = false;
    let mut checked_scope = false;
    let mut ended = false;
    let mut data = None;
    let mut priority = None;
    let mut read_cache = false;
    let mut prepared = model::DisplayData::default();
    let mut dirty = true;
    let mut renderer: Option<render::Renderer> = None;
    let mut retry_render = Instant::now();
    let mut client = None;
    loop {
        let delay = if pids.is_empty() || ended {
            next_process.saturating_duration_since(Instant::now())
        } else {
            Duration::from_millis(50)
        };
        tokio::select! {
            command = commands.recv() => match command {
                Some(Some(next)) => { settings = next; dirty = true; },
                _ => return,
            },
            _ = tokio::time::sleep(delay) => {},
        }
        if let Some(renderer) = renderer.as_ref() {
            renderer.pump();
        }
        let now = Instant::now();
        if now >= next_process {
            next_process = now + Duration::from_secs(3);
            let current = crate::windows::process::matching(&["League of Legends.exe"])
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>();
            if current != pids {
                pids = current;
                data = None;
                priority = None;
                read_cache = false;
                allowed = false;
                checked_scope = false;
                ended = false;
                renderer.take();
                client = None;
                if let Some(task) = fetch.take() {
                    task.abort();
                }
                if let Some(task) = scope.take() {
                    task.abort();
                }
            }
        }
        if pids.is_empty() {
            continue;
        }
        if !checked_scope && scope.is_none() && now >= next_fetch {
            scope = Some(tokio::spawn(async {
                crate::runes::overlay_game_session()
                    .await
                    .ok()
                    .is_some_and(|session| model::supported_session(&session))
            }));
        }
        if scope.as_ref().is_some_and(|task| task.is_finished()) {
            allowed = scope.take().unwrap().await.unwrap_or(false);
            checked_scope = allowed;
            // Fail closed, but retry a client that was still loading.
            next_fetch = now + Duration::from_secs(if allowed { 0 } else { 3 });
        }
        if allowed && !ended && fetch.is_none() && now >= next_fetch {
            next_fetch = now + Duration::from_secs(1);
            if client.is_none() {
                match live_client::LiveClient::new() {
                    Ok(created) => client = Some(std::sync::Arc::new(created)),
                    Err(error) => {
                        eprintln!("Overlay: {error}");
                        continue;
                    }
                }
            }
            let client = client.as_ref().unwrap().clone();
            fetch = Some(tokio::spawn(async move { client.fetch().await }));
        }
        if fetch.as_ref().is_some_and(|task| task.is_finished()) {
            dirty = true;
            data = fetch.take().unwrap().await.ok().and_then(Result::ok);
            if let Some(current) = data.as_ref() {
                ended = current["events"]["Events"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|event| event["EventName"] == "GameEnd");
                if !model::supported(current) {
                    data = None;
                }
            }
            if ended {
                renderer.take();
                client = None;
                priority = None;
                prepared = model::DisplayData::default();
            }
            if !read_cache {
                if let Some(current) = data.as_ref() {
                    let current_tier = crate::vault::load()
                        .ok()
                        .and_then(|config| config.rune_tier)
                        .unwrap_or_else(|| tier.clone());
                    priority = read_priority(current, &current_tier);
                    read_cache = true;
                }
            }
        }
        // 20 Hz state sampling while a game process exists. No global hook,
        // key capture or swallowing; no bitmap work unless visible data changes.
        let bounds = data.as_ref().and_then(|_| render::game_bounds(&pids));
        if let (Some(bounds), Some(data)) = (bounds, data.as_ref()) {
            let held =
                settings.gold && unsafe { GetAsyncKeyState(settings.gold_hotkey as i32) } < 0;
            if dirty {
                prepared = display(data, settings, priority.as_deref(), true);
                dirty = false;
            }
            let mut visible = prepared.clone();
            if !held {
                visible.gold = None;
            }
            if visible == model::DisplayData::default() {
                if let Some(renderer) = renderer.as_mut() {
                    renderer.hide();
                }
                continue;
            }
            if renderer.is_none() && now >= retry_render {
                match render::Renderer::new() {
                    Ok(created) => renderer = Some(created),
                    Err(error) => {
                        eprintln!("{error}");
                        retry_render = now + Duration::from_secs(3);
                    }
                }
            }
            if let Some(active) = renderer.as_mut() {
                if let Err(error) = active.show(bounds, &visible) {
                    eprintln!("{error}");
                    renderer.take();
                    retry_render = now + Duration::from_secs(3);
                }
            }
        } else if let Some(renderer) = renderer.as_mut() {
            renderer.hide();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_and_empty_settings_never_create_a_worker() {
        let settings = Settings::default();
        assert!(!settings.enabled);
        let mut worker = None;
        Controller::apply(&mut worker, settings, "emerald_plus").unwrap();
        assert!(worker.is_none());
        Controller::apply(
            &mut worker,
            Settings {
                enabled: true,
                gold: false,
                objectives: false,
                skills: false,
                ..settings
            },
            "emerald_plus",
        )
        .unwrap();
        assert!(worker.is_none());
        assert!(Settings {
            gold_hotkey: 0,
            ..settings
        }
        .validate()
        .is_err());
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), settings);
    }

    #[test]
    fn legacy_vault_defaults_off_and_overlay_preferences_round_trip() {
        let mut legacy = serde_json::to_value(crate::vault::Config::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("overlay");
        let mut config: crate::vault::Config = serde_json::from_value(legacy).unwrap();
        assert_eq!(config.overlay, Settings::default());
        config.overlay = Settings {
            enabled: true,
            objectives: false,
            gold_hotkey: 112,
            ..Settings::default()
        };
        let decoded: crate::vault::Config =
            serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(decoded.overlay, config.overlay);
    }

    #[test]
    fn fixture_widgets_obey_individual_toggles_and_gold_hold() {
        let data = serde_json::from_str(include_str!("fixtures/allgamedata.json")).unwrap();
        let settings = Settings::default();
        let visible = display(&data, settings, Some("QWE"), true);
        assert!(visible.gold.is_some());
        assert_eq!(visible.skill, Some('R'));
        assert_eq!(visible.objectives.len(), 2);
        assert!(display(&data, settings, Some("QWE"), false).gold.is_none());
        assert!(display(&data, settings, None, true).skill.is_none());
        assert_eq!(
            display(
                &data,
                Settings {
                    gold: false,
                    objectives: false,
                    skills: false,
                    ..settings
                },
                Some("QWE"),
                true
            ),
            model::DisplayData::default()
        );
    }

    #[test]
    fn worker_is_joined_on_disable() {
        let mut worker = None;
        Controller::apply(
            &mut worker,
            Settings {
                enabled: true,
                ..Settings::default()
            },
            "emerald_plus",
        )
        .unwrap();
        assert!(worker.is_some());
        let before = Instant::now();
        Controller::apply(&mut worker, Settings::default(), "emerald_plus").unwrap();
        assert!(worker.is_none());
        assert!(before.elapsed() < Duration::from_secs(2));
    }
}
