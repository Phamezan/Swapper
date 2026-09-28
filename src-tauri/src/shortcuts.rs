//! Account launch shortcuts: `swapper://switch/<internal-account-id>` deep
//! links and the desktop `.url` files that create them.
//!
//! The link carries only Swapper's internal account id — never a Riot id,
//! PUUID or session data. Links go through exactly the switch path the UI
//! uses (see `start_switch` in lib.rs), so the game-running and busy checks
//! apply unchanged.
//!
//! Windows registration behaviour: the deep-link plugin registers the scheme
//! under `HKCU\Software\Classes\swapper` pointing at the running executable.
//! `init` calls `register_all` on every startup, so a dev run points the
//! scheme at `target\debug\swapper.exe` and an installed build at the install
//! path — the last Swapper that ran owns the scheme. Switching through a
//! shortcut therefore needs at least one prior launch of the build under
//! test; a plain `cargo run` in another worktree would silently steal the
//! registration.

use std::fmt;
use std::path::Path;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_notification::NotificationExt;
use url::Url;
use uuid::Uuid;

use crate::AppState;

pub const SCHEME: &str = "swapper";
const SWITCH_HOST: &str = "switch";

/// Why a switch link was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum SwitchLinkError {
    /// Not a URL or not the `swapper://` scheme.
    Scheme,
    /// Host is not `switch`.
    Host,
    /// Missing or extra path segments.
    Path,
    /// The path segment is not an account id.
    Id,
    /// Port, credentials, query or fragment present.
    Extra,
}

impl fmt::Display for SwitchLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            SwitchLinkError::Scheme => "link must use the swapper:// scheme",
            SwitchLinkError::Host => "link host must be switch",
            SwitchLinkError::Path => "link must have exactly one path segment",
            SwitchLinkError::Id => "link path must be an account id",
            SwitchLinkError::Extra => "link must not contain a port, query or fragment",
        };
        f.write_str(text)
    }
}

/// The `swapper://switch/<id>` link for one account, in canonical form.
pub fn switch_link(id: Uuid) -> String {
    format!("{SCHEME}://{SWITCH_HOST}/{}", id.hyphenated())
}

/// Strictly parses `swapper://switch/<internal-account-id>` and rejects
/// everything else: other schemes or hosts, extra segments, ports,
/// credentials, queries, fragments, and non-canonical or non-UUID ids.
pub fn parse_switch_link(raw: &str) -> Result<Uuid, SwitchLinkError> {
    let url = Url::parse(raw).map_err(|_| SwitchLinkError::Scheme)?;
    if url.scheme() != SCHEME {
        return Err(SwitchLinkError::Scheme);
    }
    if !url
        .host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case(SWITCH_HOST))
    {
        return Err(SwitchLinkError::Host);
    }
    if url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(SwitchLinkError::Extra);
    }
    let segment = url.path().strip_prefix('/').ok_or(SwitchLinkError::Path)?;
    let mut segments = segment.split('/');
    let id_segment = segments.next().ok_or(SwitchLinkError::Path)?;
    // A trailing slash or a second segment is never a bare account id.
    if segments.next().is_some() || id_segment.is_empty() {
        return Err(SwitchLinkError::Path);
    }
    let Ok(id) = Uuid::parse_str(id_segment) else {
        return Err(SwitchLinkError::Id);
    };
    // Accept the canonical hyphenated form only, in any letter case; this
    // keeps out brace, urn and compact UUID spellings in the path.
    if !id_segment.eq_ignore_ascii_case(&id.hyphenated().to_string()) {
        return Err(SwitchLinkError::Id);
    }
    Ok(id)
}

/// The Windows internet-shortcut file contents for one account link.
fn shortcut_contents(link: &str, icon: Option<&Path>) -> String {
    let mut contents = format!("[InternetShortcut]\nURL={link}\n");
    if let Some(icon) = icon {
        contents.push_str(&format!("IconFile={}\nIconIndex=0\n", icon.display()));
    }
    contents
}

/// Turns an account display name into a Windows-safe file name stem,
/// replacing characters Explorer cannot store.
fn shortcut_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    // Windows forbids trailing dots and spaces in file names.
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        "account".into()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// Writes `Swapper <name>.url` onto the user's Desktop; returns its path.
/// A `.url` file is plain text and launches the link through the OS protocol
/// handler, so it survives Swapper updates and path changes. Overwrites a
/// previous shortcut for the same name.
pub fn create_desktop_shortcut(app: &AppHandle, id: Uuid, name: &str) -> Result<String, String> {
    let desktop = app
        .path()
        .desktop_dir()
        .map_err(|e| format!("Could not find the Desktop folder: {e}"))?;
    let file = desktop.join(format!("Swapper {}.url", shortcut_file_stem(name)));
    let icon = std::env::current_exe().ok();
    let contents = shortcut_contents(&switch_link(id), icon.as_deref());
    std::fs::write(&file, contents)
        .map_err(|e| format!("Could not write the desktop shortcut: {e}"))?;
    Ok(file.display().to_string())
}

#[tauri::command]
pub fn create_account_shortcut(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<String, String> {
    let name = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| "Account no longer exists.".to_string())?
            .display_name()
    };
    create_desktop_shortcut(&app, id, &name)
}

fn notify(app: &AppHandle, title: &str, body: String) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Handles one link: strict validation, then the same switch path as the UI.
/// Invalid links and unknown ids only notify — nothing else happens.
pub fn handle_switch_link(app: &AppHandle, raw: &str) {
    let id = match parse_switch_link(raw) {
        Ok(id) => id,
        Err(_) => {
            return notify(
                app,
                "Swapper link not recognized",
                "Only swapper://switch links to a saved account are supported. No action was taken."
                    .into(),
            );
        }
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let saved = state
        .config
        .lock()
        .ok()
        .is_some_and(|config| config.accounts.iter().any(|a| a.id == id));
    if !saved {
        return notify(
            app,
            "Shortcut account missing",
            "The account this shortcut points to is no longer saved in Swapper.".into(),
        );
    }
    if let Err(reason) = crate::start_switch(app, &state, id) {
        notify(app, "Cannot switch now", reason);
    }
}

/// Registers the `swapper://` scheme for this executable and hands links to
/// the switch path: launch links once from `get_current`, then every link of
/// a second instance through `on_open_url` (the single-instance plugin
/// forwards those arguments to the deep-link plugin before its callback).
pub fn init(app: tauri::AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt;

    let _ = app.deep_link().register_all();
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            handle_switch_link(&app, url.as_str());
        }
    }
    let handler_app = app.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            handle_switch_link(&handler_app, url.as_str());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: Uuid = Uuid::from_u128(0x1b671a64_40d5_491e_99b0_da01ff1f2354);

    #[test]
    fn canonical_link_parses_back_to_the_id() {
        assert_eq!(switch_link(ID), format!("swapper://switch/{ID}"));
        assert_eq!(parse_switch_link(&switch_link(ID)), Ok(ID));
    }

    #[test]
    fn uppercase_uuid_is_accepted() {
        let raw = format!(
            "swapper://switch/{}",
            ID.hyphenated().to_string().to_uppercase()
        );
        assert_eq!(parse_switch_link(&raw), Ok(ID));
    }

    #[test]
    fn other_schemes_and_hosts_are_rejected() {
        let id = ID.hyphenated();
        assert_eq!(
            parse_switch_link(&format!("https://switch/{id}")),
            Err(SwitchLinkError::Scheme)
        );
        assert_eq!(
            parse_switch_link("not a link"),
            Err(SwitchLinkError::Scheme)
        );
        assert_eq!(
            parse_switch_link(&format!("swapper://other/{id}")),
            Err(SwitchLinkError::Host)
        );
        assert_eq!(
            parse_switch_link(&format!("swapper:///{id}")),
            Err(SwitchLinkError::Host)
        );
        assert_eq!(parse_switch_link(&format!("swapper://SWITCH/{id}")), Ok(ID));
    }

    #[test]
    fn path_must_be_exactly_one_uuid_segment() {
        let id = ID.hyphenated();
        assert_eq!(
            parse_switch_link(&format!("swapper://switch/{id}/")),
            Err(SwitchLinkError::Path)
        );
        assert_eq!(
            parse_switch_link(&format!("swapper://switch/{id}/extra")),
            Err(SwitchLinkError::Path)
        );
        assert_eq!(
            parse_switch_link("swapper://switch"),
            Err(SwitchLinkError::Path)
        );
        assert_eq!(
            parse_switch_link("swapper://switch/"),
            Err(SwitchLinkError::Path)
        );
        assert_eq!(
            parse_switch_link("swapper://switch/550e8400-e29b-41d4-a716-4466554400000"),
            Err(SwitchLinkError::Id)
        );
        assert_eq!(
            parse_switch_link("swapper://switch/hello"),
            Err(SwitchLinkError::Id)
        );
    }

    #[test]
    fn compact_urn_and_braced_uuid_spellings_are_rejected() {
        let compact = ID.simple().to_string();
        assert_eq!(
            parse_switch_link(&format!("swapper://switch/{compact}")),
            Err(SwitchLinkError::Id)
        );
        assert_eq!(
            parse_switch_link(&format!("swapper://switch/{{{}}}", ID.hyphenated())),
            Err(SwitchLinkError::Id)
        );
        assert_eq!(
            parse_switch_link(&format!("swapper://switch/urn:uuid:{}", ID.hyphenated())),
            Err(SwitchLinkError::Id)
        );
    }

    #[test]
    fn ports_credentials_queries_and_fragments_are_rejected() {
        let id = ID.hyphenated();
        assert_eq!(
            parse_switch_link(&format!("swapper://switch:443/{id}")),
            Err(SwitchLinkError::Extra)
        );
        assert_eq!(
            parse_switch_link(&format!("swapper://user@switch/{id}")),
            Err(SwitchLinkError::Extra)
        );
        assert_eq!(
            parse_switch_link(&format!("swapper://switch/{id}?x=1")),
            Err(SwitchLinkError::Extra)
        );
        assert_eq!(
            parse_switch_link(&format!("swapper://switch/{id}#frag")),
            Err(SwitchLinkError::Extra)
        );
    }

    #[test]
    fn file_stem_replaces_windows_forbidden_characters() {
        assert_eq!(shortcut_file_stem("Main"), "Main");
        assert_eq!(shortcut_file_stem("Example#1234"), "Example#1234");
        assert_eq!(
            shortcut_file_stem(r#"a/b\c:d*e?f"g<h>i|j"#),
            "a-b-c-d-e-f-g-h-i-j"
        );
        assert_eq!(shortcut_file_stem("trailing dots..."), "trailing dots");
        assert_eq!(shortcut_file_stem("  spaced  "), "spaced");
        assert_eq!(shortcut_file_stem("///"), "---");
        assert_eq!(shortcut_file_stem("x\u{7}y"), "x-y");
    }

    #[test]
    fn shortcut_file_is_an_internet_shortcut_pointing_at_the_link() {
        let contents = shortcut_contents(&switch_link(ID), None);
        assert_eq!(
            contents,
            format!("[InternetShortcut]\nURL=swapper://switch/{ID}\n")
        );
    }

    #[test]
    fn shortcut_file_can_reference_an_icon() {
        let contents =
            shortcut_contents(&switch_link(ID), Some(Path::new("C:\\apps\\Swapper.exe")));
        assert_eq!(
            contents,
            format!(
                "[InternetShortcut]\nURL=swapper://switch/{ID}\nIconFile=C:\\apps\\Swapper.exe\nIconIndex=0\n"
            )
        );
    }
}
