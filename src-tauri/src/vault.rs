use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

const MAX_SNAPSHOT_BYTES: usize = 128 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PROFILE_ICON_DATA_URL_BYTES: usize = 384 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    /// Legacy display name, kept so older builds still show something. New
    /// accounts store their Riot ID here when they are first saved.
    #[serde(default)]
    pub name: String,
    /// Canonical Riot account identity. Duplicate detection and session
    /// verification always use this — never Riot ID text, which can change.
    #[serde(default)]
    pub puuid: Option<String>,
    #[serde(default)]
    pub game_name: Option<String>,
    #[serde(default)]
    pub tag_line: Option<String>,
    /// League platform id from platform configuration, e.g. "EUW1".
    #[serde(default)]
    pub platform: Option<String>,
    /// Friendly region label derived from the platform id, e.g. "EUW".
    /// Never inferred from the Riot ID tagline.
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub profile_icon_id: Option<i64>,
    /// Optional user-defined label; independent of Riot identity.
    #[serde(default)]
    pub nickname: Option<String>,
    pub vault_id: Uuid,
}

impl Account {
    /// Display identity: "GameName#TagLine" as last reported by the client.
    pub fn riot_id(&self) -> Option<String> {
        let game = self
            .game_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let tag = self
            .tag_line
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        Some(format!("{game}#{tag}"))
    }

    /// Nickname first, then Riot ID, then the legacy name.
    pub fn display_name(&self) -> String {
        if let Some(nickname) = self
            .nickname
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return nickname.to_string();
        }
        if let Some(riot_id) = self.riot_id() {
            return riot_id;
        }
        self.name.trim().to_string()
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Fullsized App replaces the tray flyout. Off by default.
    #[serde(default)]
    pub full_app: bool,
    /// Destroy the hidden flyout's webview. `None` means the default
    /// (`full_app::routing::LOW_MEMORY_DEFAULT`) has never been changed.
    #[serde(default)]
    pub low_memory_mode: Option<bool>,
    #[serde(default)]
    pub overlay: crate::overlay::Settings,
    /// Create the Fullsized App (minimized, unfocused) on Live Game when a
    /// game starts. Off by default.
    #[serde(default)]
    pub live_game_auto_open: bool,
    pub accounts: Vec<Account>,
    pub active_id: Option<Uuid>,
    pub use_deceive: bool,
    pub riot_exe: Option<String>,
    pub deceive_exe: Option<String>,
    #[serde(default)]
    pub remote_enabled: bool,
    #[serde(default)]
    pub auto_apply_top_preset: bool,
    /// op.gg rank bracket for rune statistics. `None` means the default
    /// (Emerald+) has never been changed.
    #[serde(default)]
    pub rune_tier: Option<String>,
    /// Id of the single rune page Swapper owns. Ownership is by id, so a user
    /// page that merely shares the `Swapper:` name is never modified.
    #[serde(default)]
    pub rune_page_id: Option<i64>,
    /// Whether applying a rune page also sets its recommended summoner spells.
    /// `None` means the default (on) has never been changed.
    #[serde(default)]
    pub apply_spells_with_runes: Option<bool>,
    /// Whether applying a recommended preset also adds its item build to the
    /// League shop. `None` means the default (on) has never been changed.
    #[serde(default)]
    pub import_items_with_runes: Option<bool>,
    /// Which key recommended spells put Flash on: "d", "f", or `None` to keep
    /// whichever key Flash is already on.
    #[serde(default)]
    pub flash_key: Option<String>,
    /// Optional global shortcut that toggles the tray flyout, e.g.
    /// "ctrl+shift+s". `None` means the hotkey is disabled (the default).
    #[serde(default)]
    pub hotkey: Option<String>,
    /// Whether gameflow notifications (ready check, champion select) are on.
    /// `None` means the default (on) has never been changed.
    #[serde(default)]
    pub notifications_enabled: Option<bool>,
    /// Whether ready-check notifications are on, independently of champion
    /// select. `None` means the default (on) has never been changed.
    #[serde(default)]
    pub ready_check_notifications: Option<bool>,
    /// Whether the optional Korean font is on. Off by default; when on, Swapper
    /// patches League's in-game UI while a game runs.
    #[serde(default)]
    pub korean_font: bool,
    /// Whether custom mods are on. Off by default; when off, no custom mod
    /// is loaded (files stay on disk).
    #[serde(default)]
    pub custom_mods: bool,
    /// Whether the one-time custom-mods risk confirm was accepted.
    #[serde(default)]
    pub custom_mods_risk_ack: bool,
    /// First-run setup state. `Some(false)` only for a genuinely new install
    /// (see `load`); `None` or `Some(true)` never shows it.
    #[serde(default)]
    pub onboarded: Option<bool>,
    /// Rune pages the user saved, by champion id, in the order they were saved.
    #[serde(default)]
    pub rune_presets: std::collections::BTreeMap<i64, Vec<SavedRunes>>,
}

/// A rune page the user saved for one champion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedRunes {
    pub name: String,
    pub selection: crate::runes::RuneSelection,
}

const MAX_RUNE_PRESETS: usize = 20;
const MAX_PRESET_NAME: usize = 40;

impl Config {
    /// Saves `selection` as `name` for a champion; a preset with the same name is replaced.
    pub fn save_rune_preset(&mut self, champion_id: i64, name: &str, selection: crate::runes::RuneSelection) -> Result<(), String> {
        let name = name.trim();
        if champion_id <= 0 {
            return Err("Pick a champion first.".into());
        }
        if name.is_empty() || name.chars().count() > MAX_PRESET_NAME || name.chars().any(char::is_control) {
            return Err(format!("Name is too long. Use up to {MAX_PRESET_NAME} characters."));
        }
        let ids = selection.perk_ids();
        if selection.primary_runes.len() != 3 || selection.secondary_runes.len() != 2 || selection.shards.len() != 3
            || selection.primary_page_id <= 0 || selection.secondary_page_id <= 0 || ids.iter().any(|id| *id <= 0) {
            return Err("Finish the rune page before saving it.".into());
        }
        let presets = self.rune_presets.entry(champion_id).or_default();
        let saved = SavedRunes { name: name.to_string(), selection };
        if let Some(existing) = presets.iter_mut().find(|preset| preset.name.eq_ignore_ascii_case(name)) {
            *existing = saved;
        } else if presets.len() >= MAX_RUNE_PRESETS {
            return Err(format!("You already have {MAX_RUNE_PRESETS} presets here. Delete one first."));
        } else {
            presets.push(saved);
        }
        Ok(())
    }

    /// Deletes a champion's preset by name; deleting a missing one does nothing.
    pub fn delete_rune_preset(&mut self, champion_id: i64, name: &str) {
        if let Some(presets) = self.rune_presets.get_mut(&champion_id) {
            presets.retain(|preset| !preset.name.eq_ignore_ascii_case(name.trim()));
            if presets.is_empty() {
                self.rune_presets.remove(&champion_id);
            }
        }
    }

    /// Whether the first-run setup should open by itself. A saved account always
    /// means an existing user, whatever the flag says.
    pub fn onboarding_pending(&self) -> bool {
        self.onboarded == Some(false) && self.accounts.is_empty()
    }
}

#[derive(Serialize, Deserialize)]
pub struct Entry {
    pub relative: String,
    pub bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub entries: Vec<Entry>,
}

/// Swapper's own data folder. `SWAPPER_DATA_DIR` overrides it so an isolated test build
/// keeps its data apart while Riot paths still use the real `LOCALAPPDATA`.
pub(crate) fn data_root() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(root) = TEST_LOCAL_ROOT.with(|root| root.borrow().clone()) {
        return Ok(root.join("Swapper"));
    }
    if let Some(dir) = std::env::var_os("SWAPPER_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    Ok(PathBuf::from(local).join("Swapper"))
}

fn valid_profile_icon_data_url(value: &str) -> bool {
    value.len() <= MAX_PROFILE_ICON_DATA_URL_BYTES
        && (value.starts_with("data:image/jpeg;base64,")
            || value.starts_with("data:image/png;base64,"))
}

/// Saves presentation-only League profile art outside state.json. The account UUID
/// is used as the filename so Riot IDs and PUUIDs never become filesystem paths.
pub fn save_profile_icon(account_id: Uuid, data_url: &str) -> Result<(), String> {
    let value = data_url.trim();
    if !valid_profile_icon_data_url(value) {
        return Err("That profile picture couldn't be saved.".into());
    }
    let root = data_root()?.join("icons");
    fs::create_dir_all(&root).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
    fs::write(root.join(format!("{account_id}.data-url")), value.as_bytes())
        .map_err(|e| { eprintln!("Profile icon cache failed: {e}"); "That profile picture couldn't be saved.".to_string() })
}

/// Returns a cached profile icon for UI presentation. Corrupt or stale cache
/// files are treated as a cache miss; they must never block account switching.
pub fn profile_icon_data_url(account_id: Uuid) -> Option<String> {
    let path = data_root()
        .ok()?
        .join("icons")
        .join(format!("{account_id}.data-url"));
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    valid_profile_icon_data_url(value).then(|| value.to_string())
}

pub fn remove_profile_icon(account_id: Uuid) -> Result<(), String> {
    let path = data_root()?.join("icons").join(format!("{account_id}.data-url"));
    if path.exists() {
        fs::remove_file(path).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
    }
    Ok(())
}

pub fn load() -> Result<Config, String> {
    let path = data_root()?.join("state.json");
    let bytes = match read_with_retry(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config { onboarded: Some(false), ..Config::default() });
        }
        // A lock (antivirus, backup) is transient; the file itself is fine.
        Err(error) => { eprintln!("Settings read failed: {error}"); return Err("Couldn't read your settings. Restart Swapper and try again.".into()) },
    };
    let mut config: Config = match serde_json::from_slice(&bytes) {
        Ok(config) => config,
        Err(error) => return recover_settings(&path, &error.to_string()),
    };
    // Settings saved before the setup existed belong to an existing user.
    config.onboarded.get_or_insert(true);
    Ok(config)
}

fn read_with_retry(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut attempt = 0;
    loop {
        match fs::read(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound && attempt < 4 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            result => return result,
        }
    }
}

fn recover_settings(path: &Path, reason: &str) -> Result<Config, String> {
    use std::os::windows::ffi::OsStrExt;
    use std::time::{SystemTime, UNIX_EPOCH};
    eprintln!("Settings read failed: {reason}"); let error = "Couldn't read your settings. Restart Swapper and try again.".to_string();
    // A directory or symlink at this path must not be moved out of the way.
    if !fs::symlink_metadata(path).map_err(|_| error.clone())?.is_file() {
        return Err(error);
    }
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?.as_millis();
    let backup = path.with_file_name(format!("state.json.broken-{millis}"));
    let source: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let target: Vec<u16> = backup.as_os_str().encode_wide().chain([0]).collect();
    // MoveFileW refuses to replace an existing backup, including a timestamp collision.
    if unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileW(source.as_ptr(), target.as_ptr()) } == 0 {
        eprintln!("Settings backup failed: {}", std::io::Error::last_os_error()); return Err(error);
    }
    #[cfg(not(test))]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
        // Loading precedes Tauri's notification plugin and every app window.
        let message: Vec<u16> = format!(
            "Swapper could not read its settings and will start with defaults.\n\nYour account list and preferences have been reset. Saved session files are untouched.\n\nYour original settings were preserved at:\n{}",
            backup.display()
        ).encode_utf16().chain([0]).collect();
        let title: Vec<u16> = "Swapper settings recovered".encode_utf16().chain([0]).collect();
        unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONWARNING); }
    }
    Ok(Config::default())
}

pub fn save(config: &Config) -> Result<(), String> {
    let root = data_root()?;
    fs::create_dir_all(&root).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
    let pending = root.join(format!("state-{}.tmp", Uuid::new_v4()));
    fs::write(
        &pending,
        serde_json::to_vec_pretty(config).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?,
    )
    .map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
    if let Err(e) = fs::rename(&pending, root.join("state.json")) {
        let _ = fs::remove_file(&pending);
        eprintln!("Settings save failed: {e}"); return Err("Couldn't save. Try again.".into());
    }
    Ok(())
}

pub fn save_snapshot(snapshot: &Snapshot) -> Result<Uuid, String> {
    let bytes = bincode::serialize(snapshot).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("That sign-in is too large to save. Sign in again and retry.".into());
    }
    let encrypted = protect(&bytes)?;
    let id = Uuid::new_v4();
    let root = data_root()?.join("vault");
    fs::create_dir_all(&root).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
    fs::write(root.join(format!("{id}.bin")), encrypted).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
    Ok(id)
}

pub fn read_snapshot(id: Uuid) -> Result<Snapshot, String> {
    let path = data_root()?.join("vault").join(format!("{id}.bin"));
    let encrypted = fs::read(path).map_err(|e| { eprintln!("Saved account read failed: {e}"); "Couldn't open the saved sign-in. Try Repair account.".to_string() })?;
    let bytes = unprotect(&encrypted)?;
    let snapshot: Snapshot = bincode::deserialize(&bytes).map_err(|e| { eprintln!("Saved account invalid: {e}"); "Couldn't open the saved sign-in. Try Repair account.".to_string() })?;
    // A snapshot with no sign-in would wipe the live session and land on the login screen.
    if !snapshot_has_sign_in(&snapshot) {
        eprintln!("Saved account {id} holds no sign-in");
        return Err("Couldn't restore that sign-in. Try Repair account.".into());
    }
    Ok(snapshot)
}

/// Replaces one account's saved snapshot in place: writes `snapshot` as a new
/// encrypted blob, points the account at it, and deletes the previous blob
/// only after the state file saved. On a save failure the account keeps its
/// previous record, so the old snapshot stays the one it points at.
pub fn replace_account_snapshot(
    config: &mut Config,
    account_id: Uuid,
    snapshot: &Snapshot,
) -> Result<(), String> {
    let index = config
        .accounts
        .iter()
        .position(|a| a.id == account_id)
        .ok_or("Account no longer exists")?;
    let new_vault = save_snapshot(snapshot)?;
    let previous = config.accounts[index].clone();
    let old_vault = previous.vault_id;
    config.accounts[index].vault_id = new_vault;
    if let Err(e) = save(config) {
        config.accounts[index] = previous;
        let _ = remove_snapshot(new_vault);
        return Err(e);
    }
    let _ = remove_snapshot(old_vault);
    Ok(())
}

pub fn remove_snapshot(id: Uuid) -> Result<(), String> {
    let path = data_root()?.join("vault").join(format!("{id}.bin"));
    if path.exists() {
        fs::remove_file(path).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
    }
    Ok(())
}

fn protect(input: &[u8]) -> Result<Vec<u8>, String> {
    crypt(input, true)
}

fn unprotect(input: &[u8]) -> Result<Vec<u8>, String> {
    crypt(input, false)
}

/// Protects non-account secrets with the same current-Windows-user DPAPI
/// boundary used for saved Riot sessions.
pub(crate) fn protect_local_data(input: &[u8]) -> Result<Vec<u8>, String> {
    protect(input)
}

/// Opens data previously protected for the current Windows user.
pub(crate) fn unprotect_local_data(input: &[u8]) -> Result<Vec<u8>, String> {
    unprotect(input)
}

fn crypt(input: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    let source = CRYPT_INTEGER_BLOB {
        cbData: input.len().try_into().map_err(|_| "Session is too large")?,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // DPAPI binds the blob to the current Windows user and checks integrity on decrypt.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &source,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &source,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(format!(
            "Windows could not {} this session: {}",
            if encrypt { "protect" } else { "unlock" },
            std::io::Error::last_os_error()
        ));
    }
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(result)
}

fn local_riot_root() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(root) = TEST_LOCAL_ROOT.with(|root| root.borrow().clone()) {
        return Ok(root.join("Riot Games"));
    }
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    Ok(PathBuf::from(local).join("Riot Games"))
}

// Only these Riot authentication paths are touched. Game settings and logs stay untouched.
const FILES: &[(&str, &str)] = &[
    (
        "Riot Client/Data/RiotGamesPrivateSettings.yaml",
        "client/private.yaml",
    ),
    (
        "Riot Client/Data/RiotClientPrivateSettings.yaml",
        "client/legacy-private.yaml",
    ),
    (
        "Riot Client/Config/RiotClientSettings.yaml",
        "client/settings.yaml",
    ),
    (
        "League of Legends/Data/RiotGamesPrivateSettings.yaml",
        "league/private.yaml",
    ),
];
const DIRS: &[(&str, &str)] = &[
    ("Riot Client/Data/Sessions", "client/sessions"),
    ("Riot Client/Data/Cookies", "client/cookies"),
];

/// The persisted Riot Client ID token's subject identifies its session owner.
/// This is local ownership evidence, not signature validation or proof that the
/// token is still accepted by Riot. Never log the token, claims, or subject.
fn session_puuid(bytes: &[u8]) -> Option<String> {
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_slice(bytes).ok()?;
    let token = doc["psl"]["authorization"]["riot-client"]["id_token"].as_str()?;
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    if header.is_empty() || payload.is_empty() || signature.is_empty() {
        return None;
    }
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&claims).ok()?;
    Uuid::parse_str(claims.get("sub")?.as_str()?)
        .ok()
        .map(|id| id.to_string())
}

#[cfg(test)]
pub(crate) fn test_session_yaml(puuid: &str) -> Vec<u8> {
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::json!({"sub": puuid}).to_string());
    format!("psl:\n  authorization:\n    riot-client:\n      id_token: 'e30.{claims}.fake-signature'\n      refresh_token: fake-refresh\n").into_bytes()
}

impl Snapshot {
    pub(crate) fn puuid(&self) -> Option<String> {
        // The modern file is authoritative, even when signed out or malformed.
        let entry = self
            .entries
            .iter()
            .find(|e| e.relative == "client/private.yaml")
            .or_else(|| {
                self.entries
                    .iter()
                    .find(|e| e.relative == "client/legacy-private.yaml")
            })?;
        session_puuid(&entry.bytes)
    }
}

#[cfg(test)]
thread_local! {
    static TEST_LOCAL_ROOT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Isolates filesystem tests without changing the process-wide environment.
#[cfg(test)]
pub(crate) fn with_test_data<T>(test: impl FnOnce(&Path) -> T) -> T {
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            TEST_LOCAL_ROOT.with(|root| *root.borrow_mut() = None);
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let root = std::env::temp_dir().join(format!("swapper-session-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    TEST_LOCAL_ROOT.with(|slot| *slot.borrow_mut() = Some(root.clone()));
    test(&root)
}

pub fn live_session_exists() -> Result<bool, String> {
    let root = local_riot_root()?;
    for (path, _) in &FILES[..2] {
        let source = root.join(path);
        if source.is_file() && has_persistent_auth(&fs::read(source).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn has_persistent_auth(bytes: &[u8]) -> bool {
    let body = String::from_utf8_lossy(bytes);
    let mut cookie_name_ssid = false;
    let mut cookie_has_value = false;
    let mut sessions_indent = None;
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        if let Some(parent_indent) = sessions_indent.take() {
            if indent > parent_indent && !line.starts_with('#') {
                return true;
            }
        }
        let field = if let Some(next_cookie) = line.strip_prefix('-') {
            if cookie_name_ssid && cookie_has_value {
                return true;
            }
            cookie_name_ssid = false;
            cookie_has_value = false;
            next_cookie.trim()
        } else {
            line
        };
        if let Some(value) = field.strip_prefix("name:") {
            cookie_name_ssid = value.trim().trim_matches(['\'', '"']) == "ssid";
        }
        if let Some(value) = field.strip_prefix("value:") {
            cookie_has_value = nonempty_yaml_value(value);
        }
        if let Some(value) = line.strip_prefix("sessions:") {
            if nonempty_yaml_value(value) {
                return true;
            }
            if value.trim().is_empty() {
                sessions_indent = Some(indent);
            }
        }
        for key in ["private:", "refresh_token:", "access_token:", "id_token:"] {
            if let Some(value) = line.strip_prefix(key) {
                if nonempty_yaml_value(value) {
                    return true;
                }
            }
        }
    }
    cookie_name_ssid && cookie_has_value
}

fn nonempty_yaml_value(value: &str) -> bool {
    let value = value.trim().split(" #").next().unwrap_or("").trim();
    !matches!(value, "" | "{}" | "[]" | "''" | "\"\"" | "null" | "~")
}

pub fn capture() -> Result<Snapshot, String> {
    let root = local_riot_root()?;
    let mut entries = Vec::new();
    let mut total = 0usize;
    for (path, name) in FILES {
        let source = root.join(path);
        if source.is_file() {
            push_file(&source, name.to_string(), &mut entries, &mut total)?;
        }
    }
    let mut snapshot = Snapshot { entries };
    if !snapshot_has_sign_in(&snapshot) {
        return Err(
            "No signed-in Riot session found. Sign in with Stay signed in, then try again.".into(),
        );
    }
    for (path, name) in DIRS {
        let source = root.join(path);
        if source.is_dir() {
            visit_dir(&source, &source, name, &mut snapshot.entries, &mut total)?;
        }
    }
    Ok(snapshot)
}

fn visit_dir(
    base: &Path,
    dir: &Path,
    target: &str,
    out: &mut Vec<Entry>,
    total: &mut usize,
) -> Result<(), String> {
    for item in fs::read_dir(dir).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })? {
        let item = item.map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
        let ty = item.file_type().map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
        if ty.is_symlink() {
            continue;
        }
        let path = item.path();
        if ty.is_dir() {
            visit_dir(base, &path, target, out, total)?;
        } else if ty.is_file() {
            let relative = path.strip_prefix(base).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?;
            let relative = format!("{target}/{}", relative.to_string_lossy().replace('\\', "/"));
            push_file(&path, relative, out, total)?;
        }
    }
    Ok(())
}

fn push_file(
    source: &Path,
    relative: String,
    out: &mut Vec<Entry>,
    total: &mut usize,
) -> Result<(), String> {
    let size = source.metadata().map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?.len();
    if size > MAX_FILE_BYTES {
        return Err("That sign-in is too large to save. Sign in again and retry.".into());
    }
    *total += size as usize;
    if *total > MAX_SNAPSHOT_BYTES {
        return Err("That sign-in is too large to save. Sign in again and retry.".into());
    }
    out.push(Entry {
        relative,
        bytes: fs::read(source).map_err(|e| { eprintln!("Swapper data failed: {e}"); "Couldn't read or save your data. Restart Swapper and retry.".to_string() })?,
    });
    Ok(())
}

fn target_path(relative: &str) -> Option<PathBuf> {
    let root = local_riot_root().ok()?;
    for (source, target) in FILES {
        if relative == *target {
            return Some(root.join(source));
        }
    }
    for (source, target) in DIRS {
        let Some(rest) = relative.strip_prefix(&format!("{target}/")) else {
            continue;
        };
        let path = Path::new(rest);
        if path.components().all(|c| matches!(c, Component::Normal(_))) && !rest.is_empty() {
            return Some(root.join(source).join(path));
        }
    }
    None
}

pub fn clear_live() -> Result<(), String> {
    restore(&Snapshot { entries: Vec::new() })
}

const DATA_ERROR: &str = "Couldn't read or save your data. Restart Swapper and retry.";
pub const ROLLBACK_FAILED: &str =
    "Couldn't put your previous sign-in back. Open Riot Client and sign in again.";

fn io_error(e: std::io::Error) -> String {
    eprintln!("Swapper data failed: {e}");
    DATA_ERROR.to_string()
}

/// Whether a snapshot carries a persistent Riot sign-in (not just settings files).
pub fn snapshot_has_sign_in(snapshot: &Snapshot) -> bool {
    snapshot.entries.iter().any(|e| {
        (e.relative == "client/private.yaml" || e.relative == "client/legacy-private.yaml")
            && has_persistent_auth(&e.bytes)
    })
}

/// Moves the live auth files and folders into `backup`, returning (original, moved) pairs.
fn move_live_aside(root: &Path, backup: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let mut moved = Vec::new();
    for (index, (path, _)) in FILES.iter().chain(DIRS).enumerate() {
        let target = root.join(path);
        if !target.exists() {
            continue;
        }
        fs::create_dir_all(backup).map_err(io_error)?;
        let aside = backup.join(index.to_string());
        if let Err(e) = fs::rename(&target, &aside) {
            // Only move back what was moved: the paths not reached yet still hold live files.
            for (original, aside) in moved.iter().rev() {
                if let Err(e) = fs::rename(aside, original) {
                    eprintln!("Swapper rollback failed: {e}");
                    return Err(ROLLBACK_FAILED.to_string());
                }
            }
            let _ = fs::remove_dir(backup);
            return Err(io_error(e));
        }
        moved.push((target, aside));
    }
    Ok(moved)
}

/// Removes whatever now sits at the auth paths and moves the backed-up originals back.
fn put_back(root: &Path, moved: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let mut failure = None;
    for (path, _) in FILES.iter().chain(DIRS) {
        let target = root.join(path);
        let result = if target.is_dir() { fs::remove_dir_all(&target) } else if target.exists() { fs::remove_file(&target) } else { Ok(()) };
        if let Err(e) = result {
            failure = Some(e);
        }
    }
    for (target, aside) in moved {
        if let Err(e) = fs::rename(aside, target) {
            failure = Some(e);
        }
    }
    match failure {
        None => Ok(()),
        Some(e) => {
            eprintln!("Swapper rollback failed: {e}");
            Err(ROLLBACK_FAILED.to_string())
        }
    }
}

/// Replaces the live auth files with the snapshot. The old files are moved aside first and
/// put back if anything fails, so the live session is either fully new or fully unchanged.
pub fn restore(snapshot: &Snapshot) -> Result<(), String> {
    // Validate the entire encrypted snapshot before modifying the live Riot files.
    let targets: Vec<_> = snapshot
        .entries
        .iter()
        .map(|e| {
            target_path(&e.relative)
                .ok_or_else(|| "Saved session contains an invalid path".to_string())
        })
        .collect::<Result<_, _>>()?;
    let root = local_riot_root()?;
    let backup = root.join(format!(".swapper-restore-{}", Uuid::new_v4()));
    let moved = move_live_aside(&root, &backup)?;
    let written = snapshot.entries.iter().zip(targets).try_for_each(|(entry, target)| {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::write(target, &entry.bytes).map_err(io_error)
    });
    if let Err(error) = written {
        put_back(&root, &moved)?;
        let _ = fs::remove_dir_all(&backup);
        return Err(error);
    }
    if let Err(e) = fs::remove_dir_all(&backup) {
        if backup.exists() {
            eprintln!("Swapper couldn't remove its restore backup: {e}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_replaces_and_deletes_rune_presets_per_champion() {
        let page = || crate::runes::RuneSelection { primary_page_id: 8000, secondary_page_id: 8300, keystone: 8010,
            primary_runes: vec![9111, 9104, 8299], secondary_runes: vec![8304, 8347], shards: vec![5005, 5008, 5001] };
        let mut config = Config::default();
        config.save_rune_preset(266, " Lane bully ", page()).unwrap();
        config.save_rune_preset(266, "Safe", page()).unwrap();
        let mut changed = page();
        changed.keystone = 8005;
        config.save_rune_preset(266, "lane BULLY", changed).unwrap();
        let names: Vec<_> = config.rune_presets[&266].iter().map(|p| (p.name.as_str(), p.selection.keystone)).collect();
        assert_eq!(names, [("lane BULLY", 8005), ("Safe", 8010)], "same name replaces in place");
        assert!(!config.rune_presets.contains_key(&103), "presets are per champion");

        let mut unfinished = page();
        unfinished.shards.pop();
        assert!(config.save_rune_preset(266, "Half", unfinished).is_err());
        assert!(config.save_rune_preset(266, "  ", page()).is_err());
        assert!(config.save_rune_preset(266, &"x".repeat(41), page()).is_err());
        assert!(config.save_rune_preset(0, "No champion", page()).is_err());
        for i in 2..MAX_RUNE_PRESETS {
            config.save_rune_preset(266, &format!("P{i}"), page()).unwrap();
        }
        assert!(config.save_rune_preset(266, "One too many", page()).is_err());

        config.delete_rune_preset(266, "safe");
        assert!(config.rune_presets[&266].iter().all(|p| p.name != "Safe"));
        let mut one = Config::default();
        one.save_rune_preset(1, "Only", page()).unwrap();
        one.delete_rune_preset(1, "Only");
        assert!(one.rune_presets.is_empty(), "the last delete drops the champion");
        // Older settings files have no presets.
        let old: Config = serde_json::from_str(r#"{"accounts":[],"active_id":null,"use_deceive":false,"riot_exe":null,"deceive_exe":null}"#).unwrap();
        assert!(old.rune_presets.is_empty());
    }

    #[test]
    fn reads_the_observed_riot_client_id_token_subject() {
        use base64::Engine;
        let puuid = "00000000-0000-0000-0000-000000000001";
        let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(format!(r#"{{"sub":"{puuid}"}}"#));
        let yaml = format!("psl:\n  authorization:\n    riot-client:\n      id_token: 'e30.{claims}.fake-signature'\n");
        assert_eq!(session_puuid(yaml.as_bytes()).as_deref(), Some(puuid));
    }

    #[test]
    fn offline_puuid_requires_a_valid_id_token_at_the_observed_path() {
        let puuid = "00000000-0000-0000-0000-000000000001";
        assert_eq!(
            session_puuid(&test_session_yaml(puuid)).as_deref(),
            Some(puuid)
        );
        for input in [
            b"riot-login: {persist: {session: null}}".as_slice(),
            b"psl: [broken",
            b"cookies: [{name: sub, value: '00000000-0000-0000-0000-000000000001'}]",
            b"psl: {authorization: {riot-client: {id_token: 'not-a-jwt'}}}",
            b"psl: {authorization: {riot-client: {id_token: 'e30.@@.signature'}}}",
            b"psl: {authorization: {riot-client: {id_token: 'e30.e30.signature'}}}",
            b"psl: {authorization: {riot-client: {id_token: 'e30.bm90LWpzb24.signature'}}}",
            b"psl: {authorization: {riot-client: {id_token: 'e30.e30.signature.extra'}}}",
        ] {
            assert_eq!(session_puuid(input), None);
        }
        assert_eq!(session_puuid(&test_session_yaml("not-a-uuid")), None);
        let missing_subject = test_session_yaml(puuid);
        let wrong_path = String::from_utf8(missing_subject)
            .unwrap()
            .replace("riot-client:", "other-client:");
        assert_eq!(session_puuid(wrong_path.as_bytes()), None);
    }

    #[test]
    fn modern_session_is_authoritative_over_a_stale_legacy_subject() {
        let legacy = Entry {
            relative: "client/legacy-private.yaml".into(),
            bytes: test_session_yaml("00000000-0000-0000-0000-000000000001"),
        };
        let mut snapshot = Snapshot {
            entries: vec![legacy],
        };
        assert!(snapshot.puuid().is_some());
        snapshot.entries.push(Entry {
            relative: "client/private.yaml".into(),
            bytes: b"riot-login: {persist: {session: null}}".to_vec(),
        });
        assert!(snapshot.puuid().is_none());
    }
    #[test]
    fn accepts_only_bounded_image_data_urls_for_profile_icons() {
        assert!(valid_profile_icon_data_url("data:image/png;base64,AA=="));
        assert!(valid_profile_icon_data_url("data:image/jpeg;base64,AA=="));
        assert!(!valid_profile_icon_data_url("data:text/html;base64,AA=="));
        assert!(!valid_profile_icon_data_url("https://example.com/icon.png"));
    }

    #[test]
    fn rejects_snapshot_traversal() {
        assert!(target_path("client/sessions/../outside").is_none());
        assert!(target_path("client/cookies/C:/outside").is_none());
    }

    #[test]
    fn dpapi_roundtrip_keeps_session_material_encrypted() {
        let secret = b"example Riot session token";
        let encrypted = protect(secret).unwrap();
        assert_ne!(encrypted, secret);
        assert_eq!(unprotect(&encrypted).unwrap(), secret);
    }

    #[test]
    fn recognises_signed_in_yaml_but_rejects_empty_login() {
        assert!(has_persistent_auth(b"private: 'token-data'\n"));
        assert!(has_persistent_auth(
            b"cookies:\n  - name: ssid\n    value: 'session-token'\n"
        ));
        assert!(has_persistent_auth(
            b"cookies:\n  - value: 'session-token'\n    name: ssid\n"
        ));
        assert!(has_persistent_auth(b"sessions:\n  account: active\n"));
        assert!(!has_persistent_auth(
            b"private: ''\ncookies:\n  - name: tdid\n    value: 'tracking'\n"
        ));
    }

    #[test]
    fn loads_accounts_saved_before_identity_fields_existed() {
        let id = Uuid::new_v4();
        let vault_id = Uuid::new_v4();
        let legacy = format!(r#"{{"id":"{id}","name":"Main","vault_id":"{vault_id}"}}"#);
        let account: Account = serde_json::from_str(&legacy).unwrap();
        assert_eq!(account.display_name(), "Main");
        assert!(account.puuid.is_none());
        assert!(account.riot_id().is_none());
        assert!(account.nickname.is_none());
        assert!(account.region.is_none());
    }

    #[test]
    fn broken_settings_are_preserved_and_recover_with_defaults() {
        for bytes in [b"{broken settings".as_slice(), b"{}", b"\xff\xfe"] {
            with_test_data(|root| {
                let data = root.join("Swapper");
                fs::create_dir_all(data.join("vault")).unwrap();
                let blob = data.join("vault").join("saved-session.bin");
                fs::write(&blob, b"encrypted session bytes").unwrap();
                let path = data.join("state.json");
                fs::write(&path, bytes).unwrap();

                let config = load().expect("broken settings should not prevent startup");
                assert!(config.accounts.is_empty());
                assert!(config.active_id.is_none());
                assert!(!config.onboarding_pending());
                assert!(!path.exists());
                let backups: Vec<_> = fs::read_dir(&data).unwrap().flatten()
                    .filter(|file| file.file_name().to_string_lossy().starts_with("state.json.broken-"))
                    .collect();
                assert_eq!(backups.len(), 1);
                let name = backups[0].file_name().to_string_lossy().into_owned();
                assert!(name.trim_start_matches("state.json.broken-").parse::<u128>().is_ok());
                assert_eq!(fs::read(backups[0].path()).unwrap(), bytes);
                assert_eq!(fs::read(&blob).unwrap(), b"encrypted session bytes");
                save(&config).unwrap();
                assert!(load().unwrap().accounts.is_empty());
                assert_eq!(fs::read(backups[0].path()).unwrap(), bytes);
                assert_eq!(fs::read(&blob).unwrap(), b"encrypted session bytes");
            });
        }
    }

    #[test]
    fn unreadable_settings_are_not_renamed_or_reset() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE;
        with_test_data(|root| {
            let data = root.join("Swapper");
            fs::create_dir_all(&data).unwrap();
            let path = data.join("state.json");
            fs::write(&path, b"{}").unwrap();
            let lock = fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_DELETE).open(&path).unwrap();
            assert!(fs::read(&path).is_err());
            assert!(load().is_err());
            drop(lock);
            assert_eq!(fs::read(&path).unwrap(), b"{}");
            assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
        });
    }

    #[test]
    fn settings_read_error_does_not_move_a_directory_or_overwrite_a_locked_file() {
        use std::os::windows::fs::OpenOptionsExt;
        with_test_data(|root| {
            let path = root.join("Swapper").join("state.json");
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("keep"), b"untouched").unwrap();
            assert!(load().is_err());
            assert_eq!(fs::read(path.join("keep")).unwrap(), b"untouched");
        });
        with_test_data(|root| {
            let data = root.join("Swapper");
            fs::create_dir_all(&data).unwrap();
            let path = data.join("state.json");
            fs::write(&path, b"locked settings").unwrap();
            let lock = fs::OpenOptions::new().read(true).share_mode(0).open(&path).unwrap();
            assert!(load().is_err());
            drop(lock);
            assert_eq!(fs::read(&path).unwrap(), b"locked settings");
            assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
        });
    }

    #[test]
    fn missing_settings_are_fresh_and_valid_settings_keep_their_values() {
        with_test_data(|_| {
            assert!(load().unwrap().onboarding_pending());
            let config = Config { full_app: true, riot_exe: Some("example.exe".into()), ..Config::default() };
            save(&config).unwrap();
            let loaded = load().unwrap();
            assert!(loaded.full_app);
            assert_eq!(loaded.riot_exe.as_deref(), Some("example.exe"));
            assert_eq!(loaded.onboarded, Some(true));
        });
    }

    #[test]
    fn display_name_prefers_nickname_then_riot_id_then_legacy_name() {
        let id = Uuid::new_v4();
        let vault_id = Uuid::new_v4();
        let base = format!(
            r#"{{"id":"{id}","name":"OldLabel","puuid":"abc","game_name":"Example","tag_line":"ABC","vault_id":"{vault_id}"}}"#
        );
        let mut account: Account = serde_json::from_str(&base).unwrap();
        assert_eq!(account.riot_id().as_deref(), Some("Example#ABC"));
        assert_eq!(account.display_name(), "Example#ABC");
        account.nickname = Some("Main".into());
        assert_eq!(account.display_name(), "Main");
        account.nickname = Some("   ".into());
        assert_eq!(account.display_name(), "Example#ABC");
    }

    #[test]
    fn riot_id_is_optional_until_identity_is_discovered() {
        let id = Uuid::new_v4();
        let vault_id = Uuid::new_v4();
        let partial = format!(
            r#"{{"id":"{id}","name":"Main","game_name":"Example","vault_id":"{vault_id}"}}"#
        );
        let account: Account = serde_json::from_str(&partial).unwrap();
        assert_eq!(account.riot_id(), None);
        assert_eq!(account.display_name(), "Main");
    }

    #[test]
    fn config_defaults_to_no_rune_tier_and_round_trips_one() {
        // Settings saved before the rank filter existed have no `rune_tier`.
        let legacy: Config = serde_json::from_str(r#"{"accounts":[],"use_deceive":false}"#).unwrap();
        assert_eq!(legacy.rune_tier, None);
        assert!(!legacy.auto_apply_top_preset);

        let config = Config {
            rune_tier: Some("diamond_plus".into()),
            ..Config::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.rune_tier.as_deref(), Some("diamond_plus"));
    }

    #[test]
    fn config_defaults_to_hotkey_disabled_and_round_trips_one() {
        // Settings saved before the hotkey existed have no `hotkey` entry.
        let legacy: Config = serde_json::from_str(r#"{"accounts":[],"use_deceive":false}"#).unwrap();
        assert_eq!(legacy.hotkey, None);

        let config = Config {
            hotkey: Some("ctrl+shift+s".into()),
            ..Config::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.hotkey.as_deref(), Some("ctrl+shift+s"));
    }

    #[test]
    fn notification_settings_default_on_and_round_trip() {
        // Settings saved before notifications existed have neither toggle.
        let legacy: Config = serde_json::from_str(r#"{"accounts":[],"use_deceive":false}"#).unwrap();
        assert_eq!(legacy.notifications_enabled, None);
        assert_eq!(legacy.ready_check_notifications, None);

        let config = Config {
            notifications_enabled: Some(true),
            ready_check_notifications: Some(false),
            ..Config::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.notifications_enabled, Some(true));
        assert_eq!(restored.ready_check_notifications, Some(false));
    }

    #[test]
    fn onboarding_is_pending_only_for_a_new_install_without_accounts() {
        let legacy: Config = serde_json::from_str(r#"{"accounts":[],"use_deceive":false}"#).unwrap();
        assert!(!legacy.onboarding_pending());
        let fresh = Config { onboarded: Some(false), ..Config::default() };
        assert!(fresh.onboarding_pending());
        let done = Config { onboarded: Some(true), ..Config::default() };
        assert!(!done.onboarding_pending());
    }

    // ---- swap e2e: capture / restore failure paths (edge cases 20, 21, 23) ----

    fn signed_in_entry(puuid: &str) -> Entry {
        Entry { relative: "client/private.yaml".into(), bytes: test_session_yaml(puuid) }
    }

    fn private_path(root: &Path) -> PathBuf {
        root.join("Riot Games/Riot Client/Data/RiotGamesPrivateSettings.yaml")
    }

    #[test]
    fn restore_then_capture_round_trips_every_saved_entry() {
        with_test_data(|root| {
            let snapshot = Snapshot {
                entries: vec![
                    signed_in_entry("00000000-0000-0000-0000-000000000001"),
                    Entry { relative: "client/settings.yaml".into(), bytes: b"settings: 1\n".to_vec() },
                    Entry { relative: "client/sessions/a/b.json".into(), bytes: b"{}".to_vec() },
                    Entry { relative: "client/cookies/c".into(), bytes: b"cookie".to_vec() },
                ],
            };
            restore(&snapshot).unwrap();
            assert!(live_session_exists().unwrap());
            let captured = capture().unwrap();
            let mut left: Vec<_> = snapshot.entries.iter().map(|e| (e.relative.clone(), e.bytes.clone())).collect();
            let mut right: Vec<_> = captured.entries.iter().map(|e| (e.relative.clone(), e.bytes.clone())).collect();
            left.sort();
            right.sort();
            assert_eq!(left, right);
            assert!(private_path(root).is_file());
        });
    }

    #[test]
    fn restore_rejects_an_invalid_path_before_touching_the_live_session() {
        with_test_data(|root| {
            restore(&Snapshot { entries: vec![signed_in_entry("00000000-0000-0000-0000-000000000001")] }).unwrap();
            let before = fs::read(private_path(root)).unwrap();
            let bad = Snapshot {
                entries: vec![signed_in_entry("00000000-0000-0000-0000-000000000002"), Entry { relative: "client/sessions/../../evil".into(), bytes: vec![1] }],
            };
            assert!(restore(&bad).is_err());
            assert_eq!(fs::read(private_path(root)).unwrap(), before);
        });
    }

    // edge case 21 / finding SW-03: a failure part-way through must leave the old session whole.
    #[test]
    fn a_failed_restore_leaves_the_live_session_intact() {
        with_test_data(|root| {
            let first = Snapshot {
                entries: vec![
                    signed_in_entry("00000000-0000-0000-0000-000000000001"),
                    Entry { relative: "client/sessions/keep.json".into(), bytes: b"keep".to_vec() },
                ],
            };
            restore(&first).unwrap();
            let before = fs::read(private_path(root)).unwrap();
            // A file and a folder claiming the same path: the second write fails after the first landed.
            let next = Snapshot {
                entries: vec![
                    signed_in_entry("00000000-0000-0000-0000-000000000002"),
                    Entry { relative: "client/sessions/a".into(), bytes: vec![1] },
                    Entry { relative: "client/sessions/a/b".into(), bytes: vec![2] },
                ],
            };
            assert!(restore(&next).is_err());
            assert_eq!(fs::read(private_path(root)).ok(), Some(before), "the previous live session must survive a failed restore");
            assert_eq!(fs::read(root.join("Riot Games/Riot Client/Data/Sessions/keep.json")).unwrap(), b"keep");
            assert!(!root.join("Riot Games/Riot Client/Data/Sessions/a").exists());
            let strays: Vec<_> = fs::read_dir(root.join("Riot Games")).unwrap().flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with(".swapper-restore")).collect();
            assert!(strays.is_empty(), "the backup folder must be cleaned up");
        });
    }

    #[test]
    fn clear_live_removes_every_auth_file_and_a_second_clear_is_fine() {
        with_test_data(|root| {
            restore(&Snapshot { entries: vec![signed_in_entry("00000000-0000-0000-0000-000000000001"),
                Entry { relative: "client/cookies/c".into(), bytes: b"x".to_vec() }] }).unwrap();
            clear_live().unwrap();
            clear_live().unwrap();
            assert!(!private_path(root).exists());
            assert!(!root.join("Riot Games/Riot Client/Data/Cookies").exists());
        });
    }

    #[test]
    fn a_failed_move_aside_keeps_every_live_file() {
        with_test_data(|root| {
            restore(&Snapshot { entries: vec![signed_in_entry("00000000-0000-0000-0000-000000000001"),
                Entry { relative: "client/sessions/keep.json".into(), bytes: b"keep".to_vec() }] }).unwrap();
            let before = fs::read(private_path(root)).unwrap();
            let riot = root.join("Riot Games");
            let backup = riot.join(".swapper-restore-test");
            // Index 4 is the Sessions folder: a non-empty folder in its way makes that move fail
            // after private.yaml was already moved.
            fs::create_dir_all(backup.join("4/blocker")).unwrap();
            fs::write(backup.join("4/blocker/x"), b"x").unwrap();
            assert!(move_live_aside(&riot, &backup).is_err());
            assert_eq!(fs::read(private_path(root)).unwrap(), before, "moved files come back");
            assert_eq!(fs::read(riot.join("Riot Client/Data/Sessions/keep.json")).unwrap(), b"keep", "unmoved files stay");
        });
    }

    #[test]
    fn a_snapshot_without_a_sign_in_is_not_restorable() {
        assert!(snapshot_has_sign_in(&Snapshot { entries: vec![signed_in_entry("00000000-0000-0000-0000-000000000001")] }));
        let hollow = Snapshot { entries: vec![Entry { relative: "client/private.yaml".into(), bytes: b"psl:
  authorization: {}
".to_vec() }] };
        assert!(!snapshot_has_sign_in(&hollow));
        assert!(!snapshot_has_sign_in(&Snapshot { entries: vec![] }));
    }

    // edge case 20
    #[test]
    fn capture_of_an_oversized_session_file_fails_without_touching_it() {
        with_test_data(|root| {
            let path = private_path(root);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, test_session_yaml("00000000-0000-0000-0000-000000000001")).unwrap();
            let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
            file.set_len(MAX_FILE_BYTES + 1).unwrap();
            drop(file);
            let before = fs::metadata(&path).unwrap().len();
            assert!(capture().err().unwrap().contains("too large"));
            assert_eq!(fs::metadata(&path).unwrap().len(), before);
        });
    }

    #[test]
    fn capture_without_a_persistent_sign_in_is_refused() {
        with_test_data(|root| {
            let path = private_path(root);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "psl:\n  authorization: {}\n").unwrap();
            assert!(!live_session_exists().unwrap());
            assert!(capture().err().unwrap().contains("No signed-in Riot session"));
        });
    }
}
