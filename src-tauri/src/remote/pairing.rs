use super::{handoff, lock, RemoteCore};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

pub(super) struct LanAccess {
    pub(super) pairing_token: Option<(String, Instant)>,
    pub(super) devices: Vec<PairedLanDevice>,
    pub(super) storage_available: bool,
    pub(super) last_seen_saved_at: Instant,
    /// One-time tokens letting a paired phone copy its cookie to `.local`.
    pub(super) handoff: handoff::HandoffTokens,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct PairedLanDevice {
    id: uuid::Uuid,
    credential: String,
    name: String,
    paired_at: u64,
    last_seen: u64,
    /// In-memory only: the last authorized request or WebSocket check, used to
    /// show whether a paired device is currently connected.
    #[serde(skip)]
    last_activity: Option<Instant>,
}

/// The serialized shape sent to the frontend. It deliberately omits the
/// credential so paired device passwords never leave the backend.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDeviceView {
    pub id: String,
    pub name: String,
    pub paired_at: u64,
    pub last_seen: u64,
    pub connected: bool,
}

const MAX_LAN_DEVICE_STORE_BYTES: usize = 1024 * 1024;
const LAN_LAST_SEEN_SAVE_INTERVAL: Duration = Duration::from_secs(60);
// A phone is reported as connected while it keeps hitting Swapper: the remote
// page's WebSocket re-checks its session every second.
const LAN_DEVICE_CONNECTED_WINDOW: Duration = Duration::from_secs(45);

pub(super) fn paired_devices_path() -> Result<PathBuf, String> {
    Ok(crate::vault::data_root()?.join("lan-paired-devices.bin"))
}

pub(super) fn load_paired_devices_at(path: &Path) -> Result<Vec<PairedLanDevice>, String> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > MAX_LAN_DEVICE_STORE_BYTES as u64 => {
            return Err("Paired LAN device store is too large".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not read paired LAN devices: {error}")),
    }
    let encrypted = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not read paired LAN devices: {error}")),
    };
    if encrypted.len() > MAX_LAN_DEVICE_STORE_BYTES {
        return Err("Paired LAN device store is too large".into());
    }
    let plain = crate::vault::unprotect_local_data(&encrypted)
        .map_err(|_| "Could not unlock paired LAN devices for this Windows user".to_string())?;
    if plain.len() > MAX_LAN_DEVICE_STORE_BYTES {
        return Err("Paired LAN device store is too large".into());
    }
    let devices: Vec<PairedLanDevice> = serde_json::from_slice(&plain)
        .map_err(|_| "Paired LAN device store is invalid".to_string())?;
    if devices.iter().any(|device| {
        device.credential.len() != 32
            || !device
                .credential
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || device.name.len() > 32
    }) {
        return Err("Paired LAN device store contains invalid entries".into());
    }
    Ok(devices)
}

fn save_paired_devices_at(path: &Path, devices: &[PairedLanDevice]) -> Result<(), String> {
    let plain = serde_json::to_vec(devices).map_err(|_| "Could not encode paired LAN devices")?;
    if plain.len() > MAX_LAN_DEVICE_STORE_BYTES {
        return Err("Paired LAN device store is too large".into());
    }
    let encrypted = crate::vault::protect_local_data(&plain)
        .map_err(|_| "Could not protect paired LAN devices for this Windows user")?;
    let parent = path
        .parent()
        .ok_or("Invalid paired LAN device store path")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create Swapper data folder: {error}"))?;
    let pending = parent.join(format!("lan-paired-devices-{}.tmp", uuid::Uuid::new_v4()));
    if let Err(error) = fs::write(&pending, encrypted) {
        let _ = fs::remove_file(&pending);
        return Err(format!("Could not write paired LAN devices: {error}"));
    }
    if let Err(error) = fs::rename(&pending, path) {
        let _ = fs::remove_file(&pending);
        return Err(format!("Could not save paired LAN devices: {error}"));
    }
    Ok(())
}

fn clear_paired_devices_at(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(remove_error) => save_paired_devices_at(path, &[])
            .map_err(|_| format!("Could not clear saved paired LAN devices: {remove_error}")),
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Trims and caps a device name to the 32-byte store limit without splitting a
/// multi-byte character.
fn sanitize_device_name(raw: &str) -> String {
    let mut name = String::new();
    for character in raw.trim().chars() {
        if name.len() + character.len_utf8() > 32 {
            break;
        }
        name.push(character);
    }
    name
}

fn device_view(device: &PairedLanDevice) -> PairedDeviceView {
    PairedDeviceView {
        id: device.id.to_string(),
        name: device.name.clone(),
        paired_at: device.paired_at,
        last_seen: device.last_seen,
        connected: device
            .last_activity
            .is_some_and(|at| at.elapsed() < LAN_DEVICE_CONNECTED_WINDOW),
    }
}

fn ensure_lan_storage(access: &LanAccess) -> Result<(), String> {
    if access.storage_available {
        Ok(())
    } else {
        Err(
            "Saved LAN devices could not be opened. Use Reset LAN Access in Swapper Settings first."
                .into(),
        )
    }
}

fn apply_revoke(devices: &mut Vec<PairedLanDevice>, id: &str) -> Result<(), String> {
    let Some(index) = devices
        .iter()
        .position(|device| device.id.to_string() == id)
    else {
        return Err("That paired device is no longer paired.".into());
    };
    devices.remove(index);
    Ok(())
}

/// Renames one device and returns the name that was stored, after the same
/// 32-byte sanitizing the backend applies. A failure leaves the list unchanged.
fn apply_rename(devices: &mut [PairedLanDevice], id: &str, name: &str) -> Result<String, String> {
    let name = sanitize_device_name(name);
    if name.is_empty() {
        return Err("Enter a name for this device.".into());
    }
    let Some(device) = devices
        .iter_mut()
        .find(|device| device.id.to_string() == id)
    else {
        return Err("That paired device is no longer paired.".into());
    };
    device.name = name.clone();
    Ok(name)
}

/// The address change that makes paired phones need to scan the QR again, if
/// any. Only a private-to-different-private move counts: startup (`None` to an
/// address), LAN turning on or off (`None` on either side), and an unchanged
/// address all return `None`.
pub(super) fn lan_reconnect_change(
    previous: Option<std::net::Ipv4Addr>,
    next: Option<std::net::Ipv4Addr>,
) -> Option<(std::net::Ipv4Addr, std::net::Ipv4Addr)> {
    let (previous, next) = (previous?, next?);
    if previous == next || !previous.is_private() || !next.is_private() {
        return None;
    }
    Some((previous, next))
}

/// Drops devices last seen before `since`, returning how many were removed.
fn apply_remove_stale(devices: &mut Vec<PairedLanDevice>, since: u64) -> usize {
    let before = devices.len();
    devices.retain(|device| device.last_seen >= since);
    before - devices.len()
}

/// Constant-time comparison of two credentials so a paired device's secret
/// cannot be discovered by timing. The length check only reveals the length,
/// which the store already fixes at 32 hex characters.
fn constant_time_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

fn new_secret() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

impl RemoteCore {
    pub(crate) fn pair_lan_device(
        &self,
        token: &str,
        name: &str,
    ) -> Result<Option<String>, String> {
        let mut access = lock(&self.lan_access);
        if !access.storage_available {
            return Err(
                "Saved LAN devices could not be opened. Use Reset LAN Access in Swapper Settings before pairing again."
                    .into(),
            );
        }
        let valid = access
            .pairing_token
            .as_ref()
            .is_some_and(|(expected, expires)| Instant::now() <= *expires && expected == token);
        if !valid {
            return Ok(None);
        }
        access.pairing_token = None;
        let credential = new_secret();
        let now = unix_timestamp();
        let name = sanitize_device_name(name);
        let name = if name.is_empty() {
            "Phone".to_string()
        } else {
            name
        };
        access.devices.push(PairedLanDevice {
            id: uuid::Uuid::new_v4(),
            credential: credential.clone(),
            name,
            paired_at: now,
            last_seen: now,
            last_activity: Some(Instant::now()),
        });
        let save_result =
            paired_devices_path().and_then(|path| save_paired_devices_at(&path, &access.devices));
        if let Err(error) = save_result {
            access.devices.pop();
            return Err(error);
        }
        access.last_seen_saved_at = Instant::now();
        let view = device_view(access.devices.last().expect("just paired a device"));
        drop(access);
        // Let the desktop notice the new device without waiting for a refresh.
        let _ = self.app.emit("lan_device_paired", view);
        let _ = self.app.emit("lan_devices_changed", ());
        // A phone paired again, so the "reconnect" call to action is done.
        self.clear_lan_reconnect_after_pair();
        Ok(Some(credential))
    }

    /// Serialized paired devices for Settings. Never includes credentials.
    pub fn paired_devices(&self) -> Vec<PairedDeviceView> {
        lock(&self.lan_access)
            .devices
            .iter()
            .map(device_view)
            .collect()
    }

    /// Revokes one paired device. The reduced list is saved first, so a failed
    /// save leaves the device paired; only after a successful save is it swapped
    /// into memory, which rejects that device's requests immediately.
    pub fn revoke_paired_device(&self, id: &str) -> Result<(), String> {
        let mut access = lock(&self.lan_access);
        ensure_lan_storage(&access)?;
        let mut next = access.devices.clone();
        apply_revoke(&mut next, id)?;
        save_paired_devices_at(&paired_devices_path()?, &next)?;
        access.devices = next;
        access.last_seen_saved_at = Instant::now();
        drop(access);
        let _ = self.app.emit("lan_devices_changed", ());
        Ok(())
    }

    /// Renames a paired device and returns the stored name the frontend should
    /// display, so it never has to guess the backend's 32-byte sanitizing.
    pub fn rename_paired_device(&self, id: &str, name: &str) -> Result<String, String> {
        let mut access = lock(&self.lan_access);
        ensure_lan_storage(&access)?;
        let mut next = access.devices.clone();
        let stored = apply_rename(&mut next, id, name)?;
        save_paired_devices_at(&paired_devices_path()?, &next)?;
        access.devices = next;
        access.last_seen_saved_at = Instant::now();
        drop(access);
        let _ = self.app.emit("lan_devices_changed", ());
        Ok(stored)
    }

    /// Removes paired devices last seen before `since`, so one action can clear
    /// the phones that still hold a cookie for the old LAN address.
    pub(crate) fn remove_stale_lan_devices(&self) -> Result<usize, String> {
        let Some(since) = self.status().lan_reconnect_since else {
            return Ok(0);
        };
        let mut access = lock(&self.lan_access);
        ensure_lan_storage(&access)?;
        let mut next = access.devices.clone();
        let removed = apply_remove_stale(&mut next, since);
        if removed == 0 {
            return Ok(0);
        }
        save_paired_devices_at(&paired_devices_path()?, &next)?;
        access.devices = next;
        access.last_seen_saved_at = Instant::now();
        drop(access);
        let _ = self.app.emit("lan_devices_changed", ());
        Ok(removed)
    }

    /// Clears the reconnect flag once a phone pairs again. The address-change
    /// time is kept so the paired devices list can still offer to remove the
    /// phones that never came back.
    fn clear_lan_reconnect_after_pair(&self) {
        if !self.status().lan_reconnect_needed {
            return;
        }
        self.apply(|status| {
            status.lan_reconnect_needed = false;
            status.lan_reconnect_old_address = None;
            status.lan_reconnect_new_address = None;
        });
        let _ = self.app.emit("lan_reconnect_changed", ());
        crate::refresh_tray_tooltip(&self.app);
    }

    /// Hides the reconnect banner and forgets the change time, at the user's
    /// request. Any still-paired old devices stay paired.
    pub(crate) fn dismiss_lan_reconnect(&self) {
        self.apply(|status| {
            status.lan_reconnect_needed = false;
            status.lan_reconnect_old_address = None;
            status.lan_reconnect_new_address = None;
            status.lan_reconnect_since = None;
        });
        let _ = self.app.emit("lan_reconnect_changed", ());
        crate::refresh_tray_tooltip(&self.app);
    }

    /// Records that the LAN address changed while phones were paired, and tells
    /// the desktop (toast, tray, flyout) once per change.
    pub(super) fn record_lan_reconnect(
        &self,
        previous: std::net::Ipv4Addr,
        next: std::net::Ipv4Addr,
    ) {
        if lock(&self.lan_access).devices.is_empty() {
            return;
        }
        let next_text = next.to_string();
        let changed = {
            let inner = lock(&self.inner);
            !inner.status.lan_reconnect_needed
                || inner.status.lan_reconnect_new_address.as_deref() != Some(next_text.as_str())
        };
        if !changed {
            return;
        }
        let since = unix_timestamp();
        self.apply(|status| {
            status.lan_reconnect_needed = true;
            status.lan_reconnect_old_address = Some(previous.to_string());
            status.lan_reconnect_new_address = Some(next_text);
            status.lan_reconnect_since = Some(since);
        });
        let _ = self.app.emit("lan_reconnect_changed", ());
        crate::notify::lan_reconnect_needed(&self.app);
        crate::refresh_tray_tooltip(&self.app);
    }

    pub(crate) fn has_lan_session(&self, session: &str) -> bool {
        let mut access = lock(&self.lan_access);
        let Some(index) = access
            .devices
            .iter()
            .position(|device| constant_time_eq(&device.credential, session))
        else {
            return false;
        };
        access.devices[index].last_activity = Some(Instant::now());
        let now = unix_timestamp();
        if now.saturating_sub(access.devices[index].last_seen)
            >= LAN_LAST_SEEN_SAVE_INTERVAL.as_secs()
        {
            access.devices[index].last_seen = now;
            if access.last_seen_saved_at.elapsed() >= LAN_LAST_SEEN_SAVE_INTERVAL {
                if let Ok(path) = paired_devices_path() {
                    // Last-seen writes are best effort; they must not interrupt
                    // an already paired phone's request or WebSocket.
                    let _ = save_paired_devices_at(&path, &access.devices);
                }
                access.last_seen_saved_at = Instant::now();
            }
        }
        true
    }

    /// Mints a one-time token that lets an already paired phone move its
    /// credential cookie from the IP origin to the stable `.local` hostname.
    /// The token is bound to the calling device; a revoked device's token
    /// simply stops resolving to a credential.
    pub(crate) fn mint_lan_handoff(&self, session: &str) -> Result<String, String> {
        let mut access = lock(&self.lan_access);
        let Some(device_id) = access
            .devices
            .iter()
            .find(|device| constant_time_eq(&device.credential, session))
            .map(|device| device.id.to_string())
        else {
            return Err("This device is not paired.".into());
        };
        Ok(access
            .handoff
            .mint(&device_id, Instant::now(), handoff::HANDOFF_TTL))
    }

    /// Consumes a handoff token and returns the paired device's credential so
    /// the caller can set the same cookie on the `.local` origin. Returns
    /// `None` for unknown, expired, used, or revoked-device tokens.
    pub(crate) fn consume_lan_handoff(&self, token: &str) -> Option<String> {
        let mut access = lock(&self.lan_access);
        let device_id = access.handoff.consume(token, Instant::now())?;
        access
            .devices
            .iter()
            .find(|device| device.id.to_string() == device_id)
            .map(|device| device.credential.clone())
    }

    pub(crate) fn create_lan_pairing_url(&self) -> Result<String, String> {
        let address = self.status().lan_address.ok_or_else(|| {
            "LAN Remote Control is not available on a private network.".to_string()
        })?;
        let token = new_secret();
        lock(&self.lan_access).pairing_token =
            Some((token.clone(), Instant::now() + Duration::from_secs(300)));
        Ok(format!("{address}/pair?token={token}"))
    }

    pub(crate) fn reset_lan_access(&self) -> Result<(), String> {
        let mut access = lock(&self.lan_access);
        // Revoke sessions in memory before touching disk, so active HTTP and
        // WebSocket requests are rejected immediately.
        access.devices.clear();
        access.pairing_token = None;
        access.handoff = handoff::HandoffTokens::default();
        let result = paired_devices_path().and_then(|path| clear_paired_devices_at(&path));
        access.storage_available = result.is_ok();
        access.last_seen_saved_at = Instant::now();
        drop(access);
        let _ = self.app.emit("lan_devices_changed", ());
        // LAN access is reset from scratch, so no phone is waiting to re-pair.
        self.dismiss_lan_reconnect();
        result
    }

    pub(super) fn clear_lan_pairing_token(&self) {
        lock(&self.lan_access).pairing_token = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_lan_credentials_are_encrypted_and_reloadable() {
        let directory =
            std::env::temp_dir().join(format!("swapper-lan-test-{}", uuid::Uuid::new_v4()));
        let path = directory.join("lan-paired-devices.bin");
        let credential = new_secret();
        let device = PairedLanDevice {
            id: uuid::Uuid::new_v4(),
            credential: credential.clone(),
            name: "iPhone".into(),
            paired_at: 123,
            last_seen: 456,
            last_activity: None,
        };

        save_paired_devices_at(&path, std::slice::from_ref(&device)).unwrap();
        let stored = fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&stored).contains(&credential));

        let restored = load_paired_devices_at(&path).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].credential, credential);
        assert_eq!(restored[0].name, "iPhone");
        assert_eq!(restored[0].last_seen, 456);

        clear_paired_devices_at(&path).unwrap();
        assert!(load_paired_devices_at(&path).unwrap().is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    fn device(name: &str) -> PairedLanDevice {
        PairedLanDevice {
            id: uuid::Uuid::new_v4(),
            credential: new_secret(),
            name: name.into(),
            paired_at: 1,
            last_seen: 1,
            last_activity: None,
        }
    }

    #[test]
    fn device_views_never_expose_credentials() {
        let device = device("iPhone");
        let json = serde_json::to_string(&device_view(&device)).unwrap();
        assert!(!json.contains(&device.credential));
        assert!(!json.contains("credential"));
        assert!(json.contains("\"name\":\"iPhone\""));
    }

    #[test]
    fn connected_devices_are_reported_from_recent_activity() {
        let mut device = device("iPhone");
        assert!(!device_view(&device).connected);
        device.last_activity = Some(Instant::now());
        assert!(device_view(&device).connected);
    }

    #[test]
    fn revoke_and_rename_update_a_device_list() {
        let first = device("iPhone");
        let second = device("Android phone");
        let mut devices = vec![first.clone(), second.clone()];

        apply_rename(&mut devices, &first.id.to_string(), "  My phone  ").unwrap();
        assert_eq!(devices[0].name, "My phone");
        assert_eq!(
            apply_rename(&mut devices, &second.id.to_string(), "Android phone").unwrap(),
            "Android phone"
        );

        apply_revoke(&mut devices, &first.id.to_string()).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, second.id);

        assert!(apply_revoke(&mut devices, &first.id.to_string()).is_err());
        assert!(apply_rename(&mut devices, &second.id.to_string(), "   ").is_err());
    }

    #[test]
    fn credentials_are_compared_constant_time() {
        assert!(constant_time_eq("abc123", "abc123"));
        assert!(!constant_time_eq("abc123", "abc124"));
        assert!(!constant_time_eq("abc123", "abc12"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn device_names_are_capped_to_32_bytes_without_splitting() {
        assert_eq!(sanitize_device_name("  iPhone  "), "iPhone");
        assert_eq!(sanitize_device_name(&"x".repeat(40)).len(), 32);
        // "é" is two bytes, so only 16 fit in the 32-byte cap.
        let capped = sanitize_device_name(&"é".repeat(20));
        assert_eq!(capped.len(), 32);
        assert_eq!(capped.chars().count(), 16);
    }

    #[test]
    fn a_private_address_change_needs_a_reconnect() {
        let old = "192.168.1.20".parse().unwrap();
        let new = "192.168.1.77".parse().unwrap();
        assert_eq!(lan_reconnect_change(Some(old), Some(new)), Some((old, new)));
    }

    #[test]
    fn startup_and_lan_toggling_do_not_need_a_reconnect() {
        let address = "10.0.0.5".parse().unwrap();
        // Startup, or LAN coming on from nothing.
        assert_eq!(lan_reconnect_change(None, Some(address)), None);
        // LAN turning off.
        assert_eq!(lan_reconnect_change(Some(address), None), None);
        // Nothing moved.
        assert_eq!(lan_reconnect_change(Some(address), Some(address)), None);
        assert_eq!(lan_reconnect_change(None, None), None);
    }

    #[test]
    fn only_private_addresses_need_a_reconnect() {
        let private = "192.168.4.8".parse().unwrap();
        let public = "8.8.8.8".parse().unwrap();
        assert_eq!(lan_reconnect_change(Some(private), Some(public)), None);
        assert_eq!(lan_reconnect_change(Some(public), Some(private)), None);
    }

    #[test]
    fn removing_stale_devices_keeps_recent_ones() {
        let mut stale = device("Old phone");
        stale.last_seen = 100;
        let mut fresh = device("Current phone");
        fresh.last_seen = 900;
        let since = 500;
        let mut devices = vec![stale, fresh];
        assert_eq!(apply_remove_stale(&mut devices, since), 1);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Current phone");
        assert_eq!(apply_remove_stale(&mut devices, since), 0);
    }
}
