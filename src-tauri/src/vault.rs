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

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    pub accounts: Vec<Account>,
    pub active_id: Option<Uuid>,
    pub use_deceive: bool,
    pub riot_exe: Option<String>,
    pub deceive_exe: Option<String>,
    #[serde(default)]
    pub remote_enabled: bool,
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

fn data_root() -> Result<PathBuf, String> {
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
        return Err("League profile icon data is invalid or too large".into());
    }
    let root = data_root()?.join("icons");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    fs::write(root.join(format!("{account_id}.data-url")), value.as_bytes())
        .map_err(|e| format!("Cannot cache League profile icon: {e}"))
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
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn load() -> Result<Config, String> {
    let path = data_root()?.join("state.json");
    if !path.exists() {
        return Ok(Config::default());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read Swapper settings: {e}"))
}

pub fn save(config: &Config) -> Result<(), String> {
    let root = data_root()?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let pending = root.join(format!("state-{}.tmp", Uuid::new_v4()));
    fs::write(
        &pending,
        serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&pending, root.join("state.json")) {
        let _ = fs::remove_file(&pending);
        return Err(format!("Cannot save Swapper settings: {e}"));
    }
    Ok(())
}

pub fn save_snapshot(snapshot: &Snapshot) -> Result<Uuid, String> {
    let bytes = bincode::serialize(snapshot).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("Riot session is too large to save safely".into());
    }
    let encrypted = protect(&bytes)?;
    let id = Uuid::new_v4();
    let root = data_root()?.join("vault");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    fs::write(root.join(format!("{id}.bin")), encrypted).map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn read_snapshot(id: Uuid) -> Result<Snapshot, String> {
    let path = data_root()?.join("vault").join(format!("{id}.bin"));
    let encrypted = fs::read(path).map_err(|e| format!("Cannot read saved account: {e}"))?;
    let bytes = unprotect(&encrypted)?;
    bincode::deserialize(&bytes).map_err(|e| format!("Saved account is invalid: {e}"))
}

pub fn remove_snapshot(id: Uuid) -> Result<(), String> {
    let path = data_root()?.join("vault").join(format!("{id}.bin"));
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn protect(input: &[u8]) -> Result<Vec<u8>, String> {
    crypt(input, true)
}

fn unprotect(input: &[u8]) -> Result<Vec<u8>, String> {
    crypt(input, false)
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

pub fn live_session_exists() -> Result<bool, String> {
    let root = local_riot_root()?;
    for (path, _) in &FILES[..2] {
        let source = root.join(path);
        if source.is_file() && has_persistent_auth(&fs::read(source).map_err(|e| e.to_string())?) {
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
    if !entries.iter().any(|e: &Entry| {
        (e.relative == "client/private.yaml" || e.relative == "client/legacy-private.yaml")
            && has_persistent_auth(&e.bytes)
    }) {
        return Err(
            "No signed-in Riot session found. Sign in with Stay signed in, then try again.".into(),
        );
    }
    for (path, name) in DIRS {
        let source = root.join(path);
        if source.is_dir() {
            visit_dir(&source, &source, name, &mut entries, &mut total)?;
        }
    }
    Ok(Snapshot { entries })
}

fn visit_dir(
    base: &Path,
    dir: &Path,
    target: &str,
    out: &mut Vec<Entry>,
    total: &mut usize,
) -> Result<(), String> {
    for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let ty = item.file_type().map_err(|e| e.to_string())?;
        if ty.is_symlink() {
            continue;
        }
        let path = item.path();
        if ty.is_dir() {
            visit_dir(base, &path, target, out, total)?;
        } else if ty.is_file() {
            let relative = path.strip_prefix(base).map_err(|e| e.to_string())?;
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
    let size = source.metadata().map_err(|e| e.to_string())?.len();
    if size > MAX_FILE_BYTES {
        return Err("A Riot session file is too large to save safely".into());
    }
    *total += size as usize;
    if *total > MAX_SNAPSHOT_BYTES {
        return Err("Riot session is too large to save safely".into());
    }
    out.push(Entry {
        relative,
        bytes: fs::read(source).map_err(|e| e.to_string())?,
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
    let root = local_riot_root()?;
    for (path, _) in FILES {
        let target = root.join(path);
        if target.exists() {
            fs::remove_file(target).map_err(|e| e.to_string())?;
        }
    }
    for (path, _) in DIRS {
        let target = root.join(path);
        if target.exists() {
            fs::remove_dir_all(target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

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
    clear_live()?;
    for (entry, target) in snapshot.entries.iter().zip(targets) {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(target, &entry.bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
