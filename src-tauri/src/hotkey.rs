//! Global shortcut that opens/toggles the tray flyout from anywhere.
//!
//! The combination is recorded in Settings as a string such as
//! "ctrl+shift+KeyS", validated here, and registered through the official
//! global-shortcut plugin. When the user changes it, the new combination is
//! registered before the old one is released, so a failed registration
//! (another application owns the combination) leaves the previous shortcut
//! working.

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};

const INVALID_MESSAGE: &str =
    "That combination is not usable as a global hotkey. Record it again.";
/// A global key fires no matter which app has focus, so plain Shift — which
/// users press constantly while typing — must never be enough on its own.
const NEEDS_MODIFIER_MESSAGE: &str =
    "Add Ctrl, Alt or the Windows key. Shift alone would fire while you type.";

/// Parses and validates a recorded combination such as "ctrl+shift+KeyS".
pub fn parse(raw: &str) -> Result<Shortcut, String> {
    let shortcut: Shortcut = raw.trim().parse().map_err(|_| INVALID_MESSAGE)?;
    let real_modifiers = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER;
    if shortcut.mods.intersects(real_modifiers) {
        Ok(shortcut)
    } else {
        Err(NEEDS_MODIFIER_MESSAGE.into())
    }
}

/// Canonical stored form: modifiers in recorder order, then the key code
/// (e.g. "ctrl+alt+KeyP"), so equal combinations compare equal as strings.
pub fn canonical(shortcut: Shortcut) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(5);
    if shortcut.mods.contains(Modifiers::CONTROL) {
        parts.push("ctrl".into());
    }
    if shortcut.mods.contains(Modifiers::ALT) {
        parts.push("alt".into());
    }
    if shortcut.mods.contains(Modifiers::SHIFT) {
        parts.push("shift".into());
    }
    if shortcut.mods.contains(Modifiers::SUPER) {
        parts.push("super".into());
    }
    parts.push(shortcut.key.to_string());
    parts.join("+")
}

/// Registers a combination. Fails when another application already owns it.
pub fn register(app: &AppHandle, shortcut: Shortcut) -> Result<(), String> {
    app.global_shortcut()
        .register(shortcut)
        .map_err(|_| "That combination is already used by another application. Pick a different one.".to_string())
}

pub fn unregister(app: &AppHandle, shortcut: Shortcut) {
    let _ = app.global_shortcut().unregister(shortcut);
}

/// Makes the OS registration match `wanted`: registers the new combination
/// before releasing the one currently registered, so a conflict aborts here
/// with the previous combination still working. Both arguments are the parsed
/// forms of the stored strings.
pub fn swap(app: &AppHandle, registered: Option<Shortcut>, wanted: Option<Shortcut>) -> Result<(), String> {
    if let Some(wanted) = wanted {
        register(app, wanted)?;
    }
    if let Some(registered) = registered {
        if Some(registered) != wanted {
            unregister(app, registered);
        }
    }
    Ok(())
}

/// Registers the saved combination at startup and reports whether it is live.
/// A conflict (another app took the combination before Swapper started) is
/// tolerated: the setting stays saved, and Settings shows it as inactive so
/// recording a different combination there reports the conflict to the user.
pub fn register_saved(app: &AppHandle, saved: Option<&str>) -> bool {
    saved
        .and_then(|raw| parse(raw).ok())
        .map(|shortcut| app.global_shortcut().register(shortcut).is_ok())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::Code;

    #[test]
    fn accepts_combinations_with_a_real_modifier() {
        assert!(parse("ctrl+shift+KeyS").is_ok());
        assert!(parse("alt+Space").is_ok());
        assert!(parse("super+F5").is_ok());
        assert!(parse("ctrl+alt+Digit1").is_ok());
    }

    #[test]
    fn rejects_combinations_without_a_real_modifier() {
        assert_eq!(parse("KeyS").unwrap_err(), NEEDS_MODIFIER_MESSAGE);
        assert_eq!(parse("shift+KeyS").unwrap_err(), NEEDS_MODIFIER_MESSAGE);
        assert_eq!(parse("F5").unwrap_err(), NEEDS_MODIFIER_MESSAGE);
    }

    #[test]
    fn rejects_unparseable_combinations() {
        assert_eq!(parse("").unwrap_err(), INVALID_MESSAGE);
        assert_eq!(parse("ctrl+").unwrap_err(), INVALID_MESSAGE);
        assert_eq!(parse("ctrl+shift").unwrap_err(), INVALID_MESSAGE);
        assert_eq!(parse("ctrl+nope").unwrap_err(), INVALID_MESSAGE);
        assert_eq!(parse("nope+KeyS").unwrap_err(), INVALID_MESSAGE);
    }

    #[test]
    fn canonical_form_is_stable_and_reparses() {
        let shortcut = parse("shift+ctrl+KeyS").unwrap();
        let text = canonical(shortcut);
        assert_eq!(text, "ctrl+shift+KeyS");
        assert_eq!(parse(&text).unwrap(), shortcut);
    }

    #[test]
    fn canonical_form_ignores_input_order_and_case() {
        let a = parse("ctrl+shift+KeyS").unwrap();
        let b = parse("shift+CTRL+s").unwrap();
        assert_eq!(canonical(a), canonical(b));
    }

    #[test]
    fn canonical_form_keeps_the_key_code() {
        assert_eq!(canonical(parse("alt+Digit0").unwrap()), "alt+Digit0");
        assert_eq!(canonical(parse("ctrl+Space").unwrap()), "ctrl+Space");
    }

    // Code is what parse() builds combinations from; keep the names in sync
    // with the recorder whitelist in src/settings/HotkeySetting.tsx.
    #[test]
    fn recorder_key_codes_parse() {
        let codes = [
            Code::Backquote,
            Code::Backslash,
            Code::BracketLeft,
            Code::BracketRight,
            Code::Comma,
            Code::Digit0,
            Code::Digit9,
            Code::Equal,
            Code::KeyA,
            Code::KeyZ,
            Code::Minus,
            Code::Period,
            Code::Quote,
            Code::Semicolon,
            Code::Slash,
            Code::Space,
            Code::Insert,
            Code::Delete,
            Code::Home,
            Code::End,
            Code::PageUp,
            Code::PageDown,
            Code::ArrowUp,
            Code::ArrowDown,
            Code::ArrowLeft,
            Code::ArrowRight,
            Code::PrintScreen,
            Code::F1,
            Code::F12,
            Code::F24,
        ];
        for code in codes {
            let shortcut = parse(&format!("ctrl+{}", code)).expect("code should parse");
            assert_eq!(shortcut.key, code);
        }
    }
}
