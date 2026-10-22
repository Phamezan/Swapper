use crate::{korean_font, remote, riot, runes, vault};
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
            Err("Another account action is in progress. Try again when it finishes.".into())
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
    /// Bundled `.fantome` the Korean font overlay is built from. Its patcher is
    /// resolved at use time from the user's LTK Manager install.
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
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
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
            return Err("That rank filter is not available.".into());
        };
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
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
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
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
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.apply_spells_with_runes == Some(enabled) {
            return Ok(());
        }
        config.apply_spells_with_runes = Some(enabled);
        vault::save(&config)
    }

    /// Whether applying a recommended preset also imports its item build.
    /// Defaults to on until the user changes it.
    pub(crate) fn import_items_with_runes(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| config.import_items_with_runes)
            .unwrap_or(true)
    }

    pub(crate) fn set_import_items_with_runes(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
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
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
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
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.ready_check_notifications == Some(enabled) {
            return Ok(());
        }
        config.ready_check_notifications = Some(enabled);
        vault::save(&config)
    }

    /// Whether the optional Korean font is on. Off until the user turns it on.
    pub(crate) fn korean_font_enabled(&self) -> bool {
        self.config
            .lock()
            .map(|config| config.korean_font)
            .unwrap_or(false)
    }

    pub(crate) fn set_korean_font_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.config.lock().map_err(|e| e.to_string())?;
        if config.korean_font == enabled {
            return Ok(());
        }
        config.korean_font = enabled;
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
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppView {
    accounts: Vec<AccountView>,
    active_id: Option<Uuid>,
    is_switching: bool,
    use_deceive: bool,
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
    /// Whether the user's LTK Manager install has the patcher host the Korean
    /// font needs. The UI shows the toggle only when it is.
    ltk_installed: bool,
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
            })
            .collect(),
        active_id: config.active_id,
        is_switching,
        use_deceive: config.use_deceive,
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
        ltk_installed: korean_font::patcher_path().is_some(),
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
            "Another account action is in progress. Try again when it finishes."
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
}
