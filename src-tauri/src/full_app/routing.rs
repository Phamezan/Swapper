//! Pure window-routing decisions, kept apart from Tauri so they can be tested.
use serde::Serialize;

/// Low memory mode when the user has never changed it. Off: in a release
/// build reopening a destroyed flyout took ~417 ms (median, tray idle), above
/// the ~400 ms budget.
pub const LOW_MEMORY_DEFAULT: bool = false;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSettings {
    pub full_app: bool,
    pub low_memory: bool,
}

impl WindowSettings {
    pub fn from_config(config: &crate::vault::Config) -> Self {
        Self {
            full_app: config.full_app,
            low_memory: config.low_memory_mode.unwrap_or(LOW_MEMORY_DEFAULT),
        }
    }

    /// The flyout webview exists from startup only in flyout mode without low
    /// memory mode; otherwise it is created on first show (or never).
    pub fn create_flyout_at_startup(self) -> bool {
        !self.full_app && !self.low_memory
    }
}

/// Only Swapper's own two windows may call app commands. Both get the full
/// command set; any other webview label is rejected.
pub fn window_may_invoke(label: &str) -> bool {
    label == super::FLYOUT || label == super::LABEL
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(json: serde_json::Value) -> crate::vault::Config {
        let mut base = serde_json::json!({
            "accounts": [], "active_id": null, "use_deceive": false,
            "riot_exe": null, "deceive_exe": null
        });
        base.as_object_mut()
            .unwrap()
            .extend(json.as_object().unwrap().clone());
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn old_settings_default_to_flyout_and_the_low_memory_default() {
        let settings = WindowSettings::from_config(&config(serde_json::json!({})));
        assert_eq!(
            settings,
            WindowSettings { full_app: false, low_memory: LOW_MEMORY_DEFAULT }
        );
    }

    #[test]
    fn settings_round_trip_through_the_vault_format() {
        let mut saved = config(serde_json::json!({}));
        saved.full_app = true;
        saved.low_memory_mode = Some(!LOW_MEMORY_DEFAULT);
        let loaded: crate::vault::Config =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(
            WindowSettings::from_config(&loaded),
            WindowSettings { full_app: true, low_memory: !LOW_MEMORY_DEFAULT }
        );
    }

    #[test]
    fn flyout_is_created_at_startup_only_in_plain_flyout_mode() {
        let at = |full_app, low_memory| {
            WindowSettings { full_app, low_memory }.create_flyout_at_startup()
        };
        assert!(at(false, false));
        assert!(!at(false, true));
        assert!(!at(true, false));
        assert!(!at(true, true));
    }

    #[test]
    fn only_swapper_windows_may_invoke_commands() {
        assert!(window_may_invoke("main"));
        assert!(window_may_invoke("full_app"));
        for label in ["dashboard", "remote", "", "Main", "full_app2"] {
            assert!(!window_may_invoke(label));
        }
    }
}
