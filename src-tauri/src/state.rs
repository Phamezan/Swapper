use crate::{mods, remote, riot, runes, vault};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct SwitchGuard {
    in_progress: Arc<AtomicBool>,
    /// Bumped on every acquired lease. Background watches compare the epoch
    /// they started with and stop when it has moved on, so a stale watch can
    /// never fire for a session another action already replaced.
    epoch: Arc<AtomicU64>,
}

#[derive(Debug)]
pub struct SwitchLease {
    in_progress: Arc<AtomicBool>,
}

impl SwitchGuard {
    pub fn new() -> Self {
        Self {
            in_progress: Arc::new(AtomicBool::new(false)),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn is_switching(&self) -> bool {
        self.in_progress.load(Ordering::SeqCst)
    }

    /// The epoch value handed to a background watch so it can detect that
    /// another account action started after its switch.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub(crate) fn epoch_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.epoch)
    }

    pub fn acquire(&self) -> Result<SwitchLease, String> {
        if self
            .in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.epoch.fetch_add(1, Ordering::SeqCst);
            Ok(SwitchLease {
                in_progress: Arc::clone(&self.in_progress),
            })
        } else {
            Err("Something else is running. Wait for it to finish.".into())
        }
    }
}

impl Drop for SwitchLease {
    fn drop(&mut self) {
        self.in_progress.store(false, Ordering::SeqCst);
    }
}

pub(crate) struct AppState {
    pub(crate) config: Mutex<vault::Config>,
    pub(crate) bundled_deceive: PathBuf,
    /// Bundled Korean font `.fantome`, handed to LTK Manager when turned on.
    pub(crate) bundled_korean_font: PathBuf,
    pub(crate) switch_guard: SwitchGuard,
    pub(crate) remote: Arc<remote::RemoteCore>,
    /// Whether the saved global hotkey is actually registered with the OS.
    /// A saved hotkey can be inactive when another application owns it.
    pub(crate) hotkey_active: AtomicBool,
}

impl AppState {
    /// Whether an account action is running. The updater stays out of the way
    /// while it is.
    pub(crate) fn is_switching(&self) -> bool {
        self.switch_guard.is_switching()
    }

    pub(crate) fn auto_apply_top_preset(&self) -> bool {
        self.config
            .lock()
            .map(|config| config.auto_apply_top_preset)
            .unwrap_or(false)
    }

    pub(crate) fn set_auto_apply_top_preset(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        config.auto_apply_top_preset = enabled;
        vault::save(&config)
    }

    /// The op.gg rank bracket, defaulting to Emerald+ when unset or unknown.
    pub(crate) fn rune_tier(&self) -> String {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.rune_tier.clone())
            .map(|tier| runes::normalize_tier(&tier).to_string())
            .unwrap_or_else(|| runes::DEFAULT_TIER.to_string())
    }

    pub(crate) fn set_rune_tier(&self, tier: &str) -> Result<(), String> {
        let Some(slug) = runes::tier_slug(tier) else {
            return Err("That rank isn't available. Pick another rank.".into());
        };
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        if config.rune_tier.as_deref() == Some(slug) {
            return Ok(());
        }
        config.rune_tier = Some(slug.to_string());
        vault::save(&config)
    }

    /// The League page id Swapper owns, if it has created one yet.
    pub(crate) fn rune_page_id(&self) -> Option<i64> {
        self.config
            .lock()
            .map(|config| config.rune_page_id)
            .unwrap_or(None)
    }

    pub(crate) fn set_rune_page_id(&self, id: i64) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        if config.rune_page_id == Some(id) {
            return Ok(());
        }
        config.rune_page_id = Some(id);
        vault::save(&config)
    }

    pub(crate) fn publish_runes(&self, applied: runes::AppliedView) {
        self.remote.publish_runes(applied);
    }

    /// Whether applying a page should also set its summoner spells. Defaults to
    /// on until the user changes it.
    pub(crate) fn apply_spells_with_runes(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.apply_spells_with_runes)
            .unwrap_or(true)
    }

    pub(crate) fn set_apply_spells_with_runes(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        if config.apply_spells_with_runes == Some(enabled) {
            return Ok(());
        }
        config.apply_spells_with_runes = Some(enabled);
        vault::save(&config)
    }

    /// Whether applying a recommended preset also imports its item build.
    /// Defaults to on until the user changes it.
    /// "d", "f", or "keep" (the default): where recommended spells put Flash.
    pub(crate) fn rune_presets(&self, champion_id: i64) -> Vec<vault::SavedRunes> {
        self.config.lock().ok().and_then(|config| config.rune_presets.get(&champion_id).cloned()).unwrap_or_default()
    }

    /// Applies `change` to the config and saves it, keeping the old config if saving fails.
    pub(crate) fn change_rune_presets(&self, champion_id: i64, change: impl FnOnce(&mut vault::Config) -> Result<(), String>) -> Result<Vec<vault::SavedRunes>, String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        let mut next = config.clone();
        change(&mut next)?;
        vault::save(&next)?;
        *config = next;
        Ok(config.rune_presets.get(&champion_id).cloned().unwrap_or_default())
    }

    pub(crate) fn flash_key(&self) -> String {
        self.config.lock().ok().and_then(|config| config.flash_key.clone()).unwrap_or_else(|| "keep".into())
    }

    pub(crate) fn set_flash_key(&self, key: &str) -> Result<(), String> {
        let value = match key {
            "d" | "f" => Some(key.to_string()),
            "keep" => None,
            _ => return Err("Pick D, F, or Keep for Flash.".into()),
        };
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        config.flash_key = value;
        vault::save(&config)?;
        crate::runes::spells::set_flash_key(key);
        Ok(())
    }

    pub(crate) fn import_items_with_runes(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.import_items_with_runes)
            .unwrap_or(true)
    }

    pub(crate) fn set_import_items_with_runes(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        if config.import_items_with_runes == Some(enabled) {
            return Ok(());
        }
        config.import_items_with_runes = Some(enabled);
        vault::save(&config)
    }

    /// Whether gameflow notifications (ready check, champion select) are on.
    /// Defaults to on until the user changes it.
    pub(crate) fn notifications_enabled(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.notifications_enabled)
            .unwrap_or(true)
    }

    pub(crate) fn set_notifications_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        if config.notifications_enabled == Some(enabled) {
            return Ok(());
        }
        config.notifications_enabled = Some(enabled);
        vault::save(&config)
    }

    /// Whether ready-check notifications are on, independently of champion
    /// select. Defaults to on until the user changes it.
    pub(crate) fn ready_check_notifications(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.ready_check_notifications)
            .unwrap_or(true)
    }

    pub(crate) fn set_ready_check_notifications(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        if config.ready_check_notifications == Some(enabled) {
            return Ok(());
        }
        config.ready_check_notifications = Some(enabled);
        vault::save(&config)
    }

    pub(crate) fn set_korean_font_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        if config.korean_font == enabled {
            return Ok(());
        }
        config.korean_font = enabled;
        vault::save(&config)
    }

    /// Whether custom mods are on. Off until the user turns them on.
    pub(crate) fn custom_mods_enabled(&self) -> bool {
        self.config.lock().map(|config| config.custom_mods).unwrap_or(false)
    }

    pub(crate) fn set_custom_mods_enabled(&self, enabled: bool, risk_ack: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| { eprintln!("Settings lock poisoned: {e}"); "Couldn't read your settings. Restart Swapper.".to_string() })?;
        config.custom_mods = enabled;
        if risk_ack {
            config.custom_mods_risk_ack = true;
        }
        vault::save(&config)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountView {
    id: Uuid,
    name: String,
    riot_id: Option<String>,
    nickname: Option<String>,
    platform: Option<String>,
    region: Option<String>,
    profile_icon_id: Option<i64>,
    icon_data_url: Option<String>,
    launch_target: Option<vault::LaunchTarget>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppView {
    accounts: Vec<AccountView>,
    active_id: Option<Uuid>,
    is_switching: bool,
    use_deceive: bool,
    launch_target: vault::LaunchTarget,
    riot_exe: Option<String>,
    riot_detected: bool,
    deceive_detected: bool,
    auto_apply_top_preset: bool,
    rune_tier: String,
    apply_spells_with_runes: bool,
    import_items_with_runes: bool,
    hotkey: Option<String>,
    hotkey_active: bool,
    notifications_enabled: bool,
    ready_check_notifications: bool,
    korean_font: bool,
    custom_mods: bool,
    custom_mods_risk_ack: bool,
    mods_region_blocked: bool,
    /// Whether LTK Manager is installed. The UI shows the toggle only when it is.
    ltk_installed: bool,
    /// Installed LTK Manager version, when it can be read.
    ltk_version: Option<String>,
    /// Whether the installed LTK Manager is below the minimum supported version.
    ltk_outdated: bool,
    /// Latest LTK Manager version already fetched from GitHub, if any.
    ltk_latest_version: Option<String>,
    /// Whether a current LTK Manager is not set up to apply mods every game.
    ltk_needs_setup: bool,
    /// What a queued LTK change waits for, if anything.
    ltk_notice: Option<&'static str>,
    remote: remote::RemoteStatus,
}

pub(crate) fn view(
    config: &vault::Config,
    bundled_deceive: &std::path::Path,
    remote: &remote::RemoteStatus,
    is_switching: bool,
    hotkey_active: bool,
) -> AppView {
    AppView {
        accounts: config
            .accounts
            .iter()
            .map(|a| AccountView {
                id: a.id,
                name: a.display_name(),
                riot_id: a.riot_id(),
                nickname: a.nickname.clone(),
                platform: a.platform.clone(),
                region: a.region.clone(),
                profile_icon_id: a.profile_icon_id,
                icon_data_url: vault::profile_icon_data_url(a.id),
                launch_target: a.launch_target,
            })
            .collect(),
        active_id: config.active_id,
        is_switching,
        use_deceive: config.use_deceive,
        launch_target: config.launch_target,
        riot_exe: config.riot_exe.clone(),
        riot_detected: riot::riot_path(config).is_some(),
        deceive_detected: riot::deceive_path(config, bundled_deceive).is_some(),
        auto_apply_top_preset: config.auto_apply_top_preset,
        rune_tier: config
            .rune_tier
            .clone()
            .map(|tier| runes::normalize_tier(&tier).to_string())
            .unwrap_or_else(|| runes::DEFAULT_TIER.to_string()),
        apply_spells_with_runes: config.apply_spells_with_runes.unwrap_or(true),
        import_items_with_runes: config.import_items_with_runes.unwrap_or(true),
        hotkey: config.hotkey.clone(),
        hotkey_active,
        notifications_enabled: config.notifications_enabled.unwrap_or(true),
        ready_check_notifications: config.ready_check_notifications.unwrap_or(true),
        korean_font: config.korean_font,
        custom_mods: config.custom_mods,
        custom_mods_risk_ack: config.custom_mods_risk_ack,
        mods_region_blocked: config.accounts.iter().find(|a| Some(a.id) == config.active_id).is_some_and(|a| mods::library::region_blocked(a.platform.as_deref(), a.region.as_deref())),
        ltk_installed: mods::patcher::ltk_installed(),
        ltk_version: mods::patcher::installed_version(),
        ltk_outdated: mods::patcher::is_outdated(mods::patcher::installed_version().as_deref()),
        ltk_latest_version: mods::patcher::cached_latest_version(),
        ltk_needs_setup: mods::patcher::ltk_installed()
            && !mods::patcher::is_outdated(mods::patcher::installed_version().as_deref())
            && !mods::ltk::is_set_up(),
        ltk_notice: mods::ltk::notice(),
        remote: remote.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_initially_not_switching() {
        let guard = SwitchGuard::new();
        assert!(!guard.is_switching());
    }

    #[test]
    fn guard_acquire_sets_switching_and_blocks_second_acquire() {
        let guard = SwitchGuard::new();
        let lease = guard.acquire().expect("First acquire should succeed");
        assert!(guard.is_switching());

        let second = guard.acquire();
        assert!(second.is_err(), "Second acquire while leased should fail");
        assert_eq!(
            second.unwrap_err(),
            "Something else is running. Wait for it to finish."
        );

        drop(lease);
        assert!(!guard.is_switching());
    }

    #[test]
    fn guard_can_reacquire_after_lease_drop() {
        let guard = SwitchGuard::new();
        {
            let _lease = guard.acquire().unwrap();
            assert!(guard.is_switching());
        }
        assert!(!guard.is_switching());

        let lease2 = guard
            .acquire()
            .expect("Should be able to acquire after drop");
        assert!(guard.is_switching());
        drop(lease2);
        assert!(!guard.is_switching());
    }

    #[test]
    fn guard_resets_on_panic() {
        let guard = SwitchGuard::new();
        let guard_clone = guard.clone();

        let _ = std::panic::catch_unwind(move || {
            let _lease = guard_clone.acquire().unwrap();
            assert!(guard_clone.is_switching());
            panic!("Simulated panic inside switch task");
        });

        assert!(
            !guard.is_switching(),
            "Guard lease must be released even if task panics"
        );
        let next_lease = guard.acquire();
        assert!(
            next_lease.is_ok(),
            "Guard can be acquired after panic unwinds"
        );
    }


    // ---- swap e2e: lease / epoch (edge cases 1, 31, 34) ----

    #[test]
    fn epoch_moves_on_every_acquired_lease_but_not_on_a_refused_one() {
        let guard = SwitchGuard::new();
        let start = guard.epoch();
        let lease = guard.acquire().unwrap();
        assert_eq!(guard.epoch(), start + 1);
        assert!(guard.acquire().is_err());
        assert_eq!(guard.epoch(), start + 1, "a refused action must not cancel the running watch");
        drop(lease);
        let _next = guard.acquire().unwrap();
        assert_eq!(guard.epoch(), start + 2);
    }

    #[test]
    fn a_watch_started_by_a_switch_is_cancelled_by_the_next_action() {
        let guard = SwitchGuard::new();
        let lease = guard.acquire().unwrap();
        let watch_epoch = guard.epoch();
        let handle = guard.epoch_handle();
        let cancelled = || handle.load(Ordering::SeqCst) != watch_epoch;
        drop(lease);
        assert!(!cancelled(), "releasing the lease alone must not cancel the post-switch watches");
        let _other_action = guard.acquire().unwrap();
        assert!(cancelled());
    }

    #[test]
    fn only_one_of_many_simultaneous_switches_wins() {
        let guard = SwitchGuard::new();
        let barrier = Arc::new(std::sync::Barrier::new(16));
        let wins: Vec<_> = (0..16)
            .map(|_| {
                let guard = guard.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    guard.acquire().map(|lease| std::mem::forget(lease)).is_ok()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(wins.iter().filter(|won| **won).count(), 1);
        assert!(guard.is_switching());
    }
}
