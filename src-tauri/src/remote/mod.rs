mod control;
mod lcu;
mod network;
mod server;
mod tailscale;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::AppHandle;
use tokio::sync::{broadcast, oneshot};

pub(crate) fn network_settings_page() -> &'static str {
    network::settings_page()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

// How long shutdown waits for the axum service thread to stop before letting
// the process exit without it.
const SERVICE_STOP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteState {
    Disabled,
    Starting,
    NotInstalled,
    Disconnected,
    Available,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub enabled: bool,
    pub state: RemoteState,
    pub address: Option<String>,
    pub tailscale_address: Option<String>,
    pub lan_address: Option<String>,
    pub lan_message: Option<String>,
    pub message: Option<String>,
    pub tailscale_installed: bool,
    pub tailscale_running: bool,
    pub dns_name: Option<String>,
    pub league_running: bool,
    pub lcu_connected: bool,
}

impl RemoteStatus {
    fn disabled() -> Self {
        Self {
            enabled: false,
            state: RemoteState::Disabled,
            address: None,
            tailscale_address: None,
            lan_address: None,
            lan_message: None,
            message: None,
            tailscale_installed: false,
            tailscale_running: false,
            dns_name: None,
            league_running: false,
            lcu_connected: false,
        }
    }
}

struct Inner {
    status: RemoteStatus,
    epoch: u64,
}

struct Service {
    port: u16,
    lan: Option<LanEndpoint>,
    lan_message: Option<String>,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<thread::JoinHandle<()>>,
}

#[derive(Clone)]
struct LanEndpoint {
    address: std::net::Ipv4Addr,
    port: u16,
}

struct ServiceInfo {
    port: u16,
    lan: Option<LanEndpoint>,
    lan_message: Option<String>,
}

struct LanAccess {
    pairing_token: Option<(String, Instant)>,
    devices: Vec<PairedLanDevice>,
    storage_available: bool,
    last_seen_saved_at: Instant,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct PairedLanDevice {
    id: uuid::Uuid,
    credential: String,
    name: String,
    paired_at: u64,
    last_seen: u64,
}

const MAX_LAN_DEVICE_STORE_BYTES: usize = 1024 * 1024;
const LAN_LAST_SEEN_SAVE_INTERVAL: Duration = Duration::from_secs(60);
// Keep a paired phone's saved address usable after Swapper restarts.
const LAN_REMOTE_PORT: u16 = 38127;

fn paired_devices_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    Ok(PathBuf::from(root)
        .join("Swapper")
        .join("lan-paired-devices.bin"))
}

fn load_paired_devices_at(path: &Path) -> Result<Vec<PairedLanDevice>, String> {
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

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct OwnedRoute {
    dns_name: String,
    port: u16,
}

fn route_claim_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    Ok(PathBuf::from(root).join("Swapper").join("remote-route.json"))
}

fn load_route_claim_at(path: &Path) -> Result<Option<OwnedRoute>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("Could not read Swapper's Tailscale route record: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Could not read Swapper's Tailscale route record: {e}")),
    }
}

fn save_route_claim_at(path: &Path, claim: &OwnedRoute) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid Tailscale route record path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let pending = parent.join(format!("remote-route-{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&pending, serde_json::to_vec(claim).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&pending, path) {
        let _ = fs::remove_file(&pending);
        return Err(format!("Could not save Swapper's Tailscale route record: {e}"));
    }
    Ok(())
}

fn route_is_available(route: &tailscale::RootRoute, dns_name: &str, owned: Option<&OwnedRoute>) -> bool {
    match route {
        tailscale::RootRoute::Vacant => true,
        tailscale::RootRoute::Proxy(target) => owned.is_some_and(|owned| {
            owned.dns_name.eq_ignore_ascii_case(dns_name)
                && target == &tailscale::proxy_target(owned.port)
        }),
        tailscale::RootRoute::Other => false,
    }
}

pub struct RemoteCore {
    pub(crate) app: AppHandle,
    ops: Mutex<()>,
    inner: Mutex<Inner>,
    service: Mutex<Option<Service>>,
    events: broadcast::Sender<RemoteStatus>,
    rune_events: broadcast::Sender<crate::runes::AppliedView>,
    /// Fired when a rune setting changes so the phone reloads without polling.
    runes_changed: broadcast::Sender<()>,
    owned_route: Mutex<Option<OwnedRoute>>,
    lan_access: Mutex<LanAccess>,
}

impl RemoteCore {
    pub fn new(app: AppHandle, enabled: bool) -> Arc<Self> {
        let (events, _) = broadcast::channel(32);
        let (rune_events, _) = broadcast::channel(32);
        let (runes_changed, _) = broadcast::channel(32);
        let status = if enabled {
            RemoteStatus {
                enabled: true,
                state: RemoteState::Starting,
                ..RemoteStatus::disabled()
            }
        } else {
            RemoteStatus::disabled()
        };
        let (devices, storage_available) =
            match paired_devices_path().and_then(|path| load_paired_devices_at(&path)) {
                Ok(devices) => (devices, true),
                Err(_) => (Vec::new(), false),
            };
        let core = Arc::new(Self {
            app,
            ops: Mutex::new(()),
            inner: Mutex::new(Inner { status, epoch: 0 }),
            service: Mutex::new(None),
            events,
            rune_events,
            runes_changed,
            owned_route: Mutex::new(None),
            lan_access: Mutex::new(LanAccess {
                pairing_token: None,
                devices,
                storage_available,
                last_seen_saved_at: Instant::now(),
            }),
        });
        let weak = Arc::downgrade(&core);
        thread::Builder::new()
            .name("swapper-remote-maintainer".into())
            .spawn(move || maintain(weak))
            .expect("Could not start the remote maintainer");
        core
    }

    pub fn status(&self) -> RemoteStatus {
        lock(&self.inner).status.clone()
    }

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
        access.devices.push(PairedLanDevice {
            id: uuid::Uuid::new_v4(),
            credential: credential.clone(),
            name: name.chars().take(32).collect(),
            paired_at: now,
            last_seen: now,
        });
        let save_result =
            paired_devices_path().and_then(|path| save_paired_devices_at(&path, &access.devices));
        if let Err(error) = save_result {
            access.devices.pop();
            return Err(error);
        }
        access.last_seen_saved_at = Instant::now();
        Ok(Some(credential))
    }

    pub(crate) fn has_lan_session(&self, session: &str) -> bool {
        let mut access = lock(&self.lan_access);
        let Some(index) = access
            .devices
            .iter()
            .position(|device| device.credential == session)
        else {
            return false;
        };
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
        let result = paired_devices_path().and_then(|path| clear_paired_devices_at(&path));
        access.storage_available = result.is_ok();
        access.last_seen_saved_at = Instant::now();
        result
    }

    fn clear_lan_pairing_token(&self) {
        lock(&self.lan_access).pairing_token = None;
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RemoteStatus> {
        self.events.subscribe()
    }

    /// Broadcasts an applied rune page so both surfaces show the same active
    /// page without polling.
    pub fn publish_runes(&self, applied: crate::runes::AppliedView) {
        let _ = self.rune_events.send(applied);
    }

    pub fn subscribe_runes(&self) -> broadcast::Receiver<crate::runes::AppliedView> {
        self.rune_events.subscribe()
    }

    /// Tells the phone a rune setting changed so it reloads the current view.
    pub fn notify_runes_changed(&self) {
        let _ = self.runes_changed.send(());
    }

    pub fn subscribe_runes_changed(&self) -> broadcast::Receiver<()> {
        self.runes_changed.subscribe()
    }

    pub fn set_enabled(self: Arc<Self>, enabled: bool) {
        let _ops = lock(&self.ops);
        if !enabled {
            self.clear_lan_pairing_token();
            self.apply_disabled();
            self.remove_owned_route();
            self.stop_service();
            self.probe_tailscale_without_remote();
            return;
        }
        self.apply(|status| {
            status.enabled = true;
            status.state = RemoteState::Starting;
            status.message = None;
        });
        self.clear_lan_pairing_token();
        if let Err(err) = Self::ensure_service(&self) {
            self.apply(|status| {
                status.state = RemoteState::Failed;
                status.message = Some(err);
            });
            return;
        }
        self.reconcile();
    }

    /// Tear down the remote service and its Tailscale Serve route when the app
    /// is exiting. Unlike [`Self::set_enabled`] this skips the Tailscale status
    /// probe that would only matter if Swapper kept running. Every step is
    /// bounded (Tailscale calls by their process timeout, the service by a join
    /// timeout), so a stuck shutdown cannot hang the caller.
    pub fn shutdown(&self) {
        let _ops = lock(&self.ops);
        self.clear_lan_pairing_token();
        self.apply_disabled();
        self.remove_owned_route();
        self.stop_service();
    }

    // Turns off the root Serve route only when Swapper is the one serving it,
    // so a route another tool owns is never removed.
    fn remove_owned_route(&self) {
        let recorded = route_claim_path()
            .and_then(|path| load_route_claim_at(&path))
            .ok()
            .flatten();
        let Some(owned) = lock(&self.owned_route).clone().or(recorded) else {
            return;
        };
        let Some(cli) = tailscale::find_cli() else {
            return;
        };
        if matches!(tailscale::root_route(&cli, &owned.dns_name), Ok(tailscale::RootRoute::Proxy(ref target)) if target == &tailscale::proxy_target(owned.port))
            && tailscale::serve_off(&cli).is_ok()
        {
            *lock(&self.owned_route) = None;
            if let Ok(path) = route_claim_path() {
                let _ = fs::remove_file(path);
            }
        }
    }

    pub fn probe(self: Arc<Self>) -> RemoteStatus {
        let _ops = lock(&self.ops);
        if !self.status().enabled {
            self.probe_tailscale_without_remote();
            return self.status();
        }
        if let Err(err) = Self::ensure_service(&self) {
            self.apply(|status| {
                status.state = RemoteState::Failed;
                status.message = Some(err);
            });
            return self.status();
        }
        self.reconcile();
        self.status()
    }

    fn probe_tailscale_without_remote(&self) {
        let Some(cli) = tailscale::find_cli() else {
            self.apply(|status| {
                status.tailscale_installed = false;
                status.tailscale_running = false;
                status.tailscale_address = None;
                status.dns_name = None;
            });
            return;
        };
        let detected = tailscale::status(&cli).ok();
        self.apply(|status| {
            status.tailscale_installed = true;
            status.tailscale_running = detected
                .as_ref()
                .is_some_and(|ts| ts.backend_state.eq_ignore_ascii_case("Running"));
            status.dns_name = detected.and_then(|ts| ts.dns_name);
            status.tailscale_address = None;
        });
    }

    pub(crate) fn patch_league(&self, league_running: bool, lcu_connected: bool) {
        self.apply(|status| {
            if !status.enabled {
                return;
            }
            status.league_running = league_running;
            status.lcu_connected = lcu_connected;
        });
    }

    fn apply(&self, update: impl FnOnce(&mut RemoteStatus)) {
        let epoch = lock(&self.inner).epoch;
        self.apply_if_current(epoch, update);
    }

    fn apply_if_current(&self, epoch: u64, update: impl FnOnce(&mut RemoteStatus)) {
        let mut inner = lock(&self.inner);
        if inner.epoch != epoch {
            return;
        }
        let before = inner.status.clone();
        update(&mut inner.status);
        if before != inner.status {
            let event = inner.status.clone();
            drop(inner);
            let _ = self.events.send(event);
        }
    }

    fn apply_disabled(&self) {
        let mut inner = lock(&self.inner);
        inner.epoch += 1;
        let before = inner.status.clone();
        inner.status = RemoteStatus::disabled();
        if before != inner.status {
            let event = inner.status.clone();
            drop(inner);
            let _ = self.events.send(event);
        }
    }

    fn stop_service(&self) {
        let taken = lock(&self.service).take();
        let Some(mut service) = taken else {
            return;
        };
        if let Some(shutdown) = service.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(join) = service.join.take() {
            let deadline = Instant::now() + SERVICE_STOP_TIMEOUT;
            while !join.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if join.is_finished() {
                let _ = join.join();
            }
        }
    }

    fn ensure_service(core: &Arc<Self>) -> Result<(), String> {
        {
            let mut guard = lock(&core.service);
            let alive = guard
                .as_ref()
                .is_some_and(|service| service.join.as_ref().is_some_and(|join| !join.is_finished()));
            if alive {
                return Ok(());
            }
            *guard = None;
        }
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let thread_core = core.clone();
        let join = thread::Builder::new()
            .name("swapper-remote".into())
            .spawn(move || service_thread(thread_core, ready_tx, shutdown_rx))
            .map_err(|e| format!("Could not start the remote service: {e}"))?;
        let info = match ready_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(info)) => info,
            Ok(Err(err)) => {
                let _ = shutdown_tx.send(());
                let _ = join.join();
                return Err(err);
            }
            Err(_) => {
                let _ = shutdown_tx.send(());
                let _ = join.join();
                return Err("The remote service did not start in time.".into());
            }
        };
        *lock(&core.service) = Some(Service {
            port: info.port,
            lan: info.lan,
            lan_message: info.lan_message,
            shutdown: Some(shutdown_tx),
            join: Some(join),
        });
        Ok(())
    }

    fn reconcile(&self) {
        let epoch = lock(&self.inner).epoch;
        let (port, lan_address, lan_message) = {
            let service = lock(&self.service);
            let Some(service) = service.as_ref() else {
                self.apply_if_current(epoch, |status| {
                    status.state = RemoteState::Failed;
                    status.address = None;
                    status.lan_address = None;
                    status.message = Some("The remote service is not running.".into());
                });
                return;
            };
            (
                service.port,
                service.lan.as_ref().map(|lan| format!("http://{}:{}", lan.address, lan.port)),
                service.lan_message.clone(),
            )
        };
        let Some(cli) = tailscale::find_cli() else {
            self.apply_if_current(epoch, |status| {
                status.tailscale_installed = false;
                status.tailscale_running = false;
                status.tailscale_address = None;
                status.dns_name = None;
                status.lan_address = lan_address.clone();
                status.lan_message = lan_message.clone();
                status.address = lan_address.clone();
                if lan_address.is_some() {
                    status.state = RemoteState::Available;
                    status.message = None;
                } else {
                    status.state = RemoteState::NotInstalled;
                    status.message = lan_message.clone().or_else(|| Some("Tailscale is not installed and LAN access is unavailable.".into()));
                }
            });
            return;
        };
        match tailscale::status(&cli) {
            Err(err) => self.apply_if_current(epoch, |status| {
                status.tailscale_installed = true;
                status.tailscale_running = false;
                status.tailscale_address = None;
                status.dns_name = None;
                status.lan_address = lan_address.clone();
                status.lan_message = lan_message.clone();
                status.address = lan_address.clone();
                if lan_address.is_some() {
                    status.state = RemoteState::Available;
                    status.message = None;
                } else {
                    status.state = RemoteState::Disconnected;
                    status.message = lan_message.clone().or(Some(err));
                }
            }),
            Ok(ts) => {
                let running = ts.backend_state.eq_ignore_ascii_case("Running");
                if !running {
                    self.apply_if_current(epoch, |status| {
                        status.tailscale_installed = true;
                        status.tailscale_running = false;
                        status.tailscale_address = None;
                        status.dns_name = ts.dns_name.clone();
                        status.lan_address = lan_address.clone();
                        status.lan_message = lan_message.clone();
                        status.address = lan_address.clone();
                        if lan_address.is_some() {
                            status.state = RemoteState::Available;
                            status.message = None;
                        } else {
                            status.state = RemoteState::Disconnected;
                            status.message = lan_message.clone().or(Some("Tailscale is not connected.".into()));
                        }
                    });
                    return;
                }
                let address = ts
                    .dns_name
                    .as_deref()
                    .and_then(tailscale::address_for);
                let Some(address) = address else {
                    self.apply_if_current(epoch, |status| {
                        status.tailscale_installed = true;
                        status.tailscale_running = true;
                        status.tailscale_address = None;
                        status.dns_name = ts.dns_name.clone();
                        status.lan_address = lan_address.clone();
                        status.lan_message = lan_message.clone();
                        status.address = lan_address.clone();
                        if lan_address.is_some() {
                            status.state = RemoteState::Available;
                            status.message = None;
                        } else {
                            status.state = RemoteState::Failed;
                            status.message = Some("Tailscale is connected but did not report a MagicDNS name, and no private LAN address is available.".into());
                        }
                    });
                    return;
                };
                let owned = route_claim_path().and_then(|path| load_route_claim_at(&path));
                let owned = match owned {
                    Ok(owned) => owned,
                    Err(err) => {
                        self.apply_if_current(epoch, |status| {
                            status.tailscale_installed = true;
                            status.tailscale_running = true;
                            status.tailscale_address = None;
                            status.dns_name = ts.dns_name.clone();
                            status.lan_address = lan_address.clone();
                            status.lan_message = lan_message.clone();
                            status.address = lan_address.clone();
                            if lan_address.is_some() {
                                status.state = RemoteState::Available;
                                status.message = None;
                            } else {
                                status.state = RemoteState::Failed;
                                status.message = Some(err.clone());
                            }
                        });
                        return;
                    }
                };
                let route_result = tailscale::root_route(&cli, &ts.dns_name.clone().unwrap_or_default())
                    .and_then(|route| {
                        if route_is_available(&route, ts.dns_name.as_deref().unwrap_or_default(), owned.as_ref()) {
                            Ok(())
                        } else {
                            Err("Tailscale's HTTPS root route is already in use. Swapper will not replace it.".into())
                        }
                    })
                    .and_then(|()| save_route_claim_at(&route_claim_path()?, &OwnedRoute {
                        dns_name: ts.dns_name.clone().unwrap_or_default(),
                        port,
                    }))
                    .and_then(|()| tailscale::serve_set(&cli, port));
                match route_result {
                    Ok(()) => {
                        *lock(&self.owned_route) = Some(OwnedRoute {
                            dns_name: ts.dns_name.clone().unwrap_or_default(),
                            port,
                        });
                        self.apply_if_current(epoch, |status| {
                            status.state = RemoteState::Available;
                            status.tailscale_installed = true;
                            status.tailscale_running = true;
                            status.dns_name = ts.dns_name.clone();
                            status.address = Some(address.clone());
                            status.tailscale_address = Some(address.clone());
                            status.lan_address = lan_address.clone();
                            status.lan_message = lan_message.clone();
                            status.message = None;
                        });
                    }
                    Err(err) => self.apply_if_current(epoch, |status| {
                        status.tailscale_installed = true;
                        status.tailscale_running = true;
                        status.tailscale_address = None;
                        status.dns_name = ts.dns_name.clone();
                        status.lan_address = lan_address.clone();
                        status.lan_message = lan_message.clone();
                        status.address = lan_address.clone();
                        if lan_address.is_some() {
                            status.state = RemoteState::Available;
                            status.message = None;
                        } else {
                            status.state = RemoteState::Failed;
                            status.message = Some(err);
                        }
                    }),
                }
            }
        }
    }

}

fn new_secret() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn service_thread(
    core: Arc<RemoteCore>,
    ready: mpsc::SyncSender<Result<ServiceInfo, String>>,
    shutdown: oneshot::Receiver<()>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            let _ = ready.send(Err(format!("Could not start the remote service: {err}")));
            return;
        }
    };
    runtime.block_on(async move {
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", 0)).await {
            Ok(listener) => listener,
            Err(err) => {
                let _ = ready.send(Err(format!(
                    "Could not listen for remote connections: {err}"
                )));
                return;
            }
        };
        let port = match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(err) => {
                let _ = ready.send(Err(format!(
                    "Could not read the remote service address: {err}"
                )));
                return;
            }
        };
        let mut lan_listener = None;
        let mut lan = None;
        let lan_message = match network::default_interface() {
            Err(error) => Some(error),
            Ok(None) => Some("No active Ethernet or Wi-Fi interface with a private IPv4 default route was found.".into()),
            Ok(Some(interface)) => match network::is_private_profile(interface.index) {
                Err(error) => Some(error),
                Ok(false) => Some("LAN access is off because the active Windows network is not set to Private.".into()),
                Ok(true) => match tokio::net::TcpListener::bind((interface.address, LAN_REMOTE_PORT)).await {
                    Err(error) => Some(format!(
                        "Could not listen on {} ({}:{LAN_REMOTE_PORT}): {error}",
                        interface.name,
                        interface.address
                    )),
                    Ok(listener) => match listener.local_addr() {
                        Err(error) => Some(format!("Could not read the LAN listener address: {error}")),
                        Ok(address) => match network::allow_private_app() {
                            Err(error) => Some(error),
                            Ok(()) => {
                                lan = Some(LanEndpoint {
                                    address: interface.address,
                                    port: address.port(),
                                });
                                lan_listener = Some(listener);
                                None
                            }
                        },
                    },
                },
            },
        };
        let _ = ready.send(Ok(ServiceInfo { port, lan, lan_message }));
        tokio::spawn(lcu::watch(core.clone()));
        let router = server::router(core.clone());
        let lan_router = server::lan_router(core);
        let mut shutdown = shutdown;
        let _ = tokio::select! {
            result = async move {
                if let Some(lan_listener) = lan_listener {
                    let lan_service = lan_router.into_make_service_with_connect_info::<std::net::SocketAddr>();
                    let (local_result, lan_result) = tokio::join!(axum::serve(listener, router), axum::serve(lan_listener, lan_service));
                    local_result.map_err(|error| error.to_string())?;
                    lan_result.map_err(|error| error.to_string())?;
                    Ok::<(), String>(())
                } else {
                    axum::serve(listener, router).await.map_err(|error| error.to_string())
                }
            } => result,
            _ = &mut shutdown => Ok(()),
        };
    });
}

fn maintain(weak: Weak<RemoteCore>) {
    loop {
        thread::sleep(Duration::from_secs(30));
        let Some(core) = weak.upgrade() else {
            return;
        };
        let (enabled, state, crashed) = {
            let inner = lock(&core.inner);
            let crashed = lock(&core.service).as_ref().is_some_and(|service| {
                service
                    .join
                    .as_ref()
                    .is_some_and(|join| join.is_finished())
            });
            (inner.status.enabled, inner.status.state, crashed)
        };
        if !enabled {
            // Keep the maintainer alive for a later Settings toggle.
            continue;
        }
        let unhealthy = crashed
            || matches!(
                state,
                RemoteState::NotInstalled | RemoteState::Disconnected | RemoteState::Failed
            );
        if !unhealthy {
            continue;
        }
        let _ops = lock(&core.ops);
        let _ = RemoteCore::ensure_service(&core);
        core.reconcile();
    }
}

#[cfg(test)]
mod route_recovery_tests {
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

    #[test]
    fn a_recorded_route_can_be_replaced_after_restart() {
        let claim = OwnedRoute { dns_name: "pc.tail.ts.net".into(), port: 1234 };
        assert!(route_is_available(
            &tailscale::RootRoute::Proxy(tailscale::proxy_target(1234)),
            "pc.tail.ts.net",
            Some(&claim),
        ));
        assert!(!route_is_available(
            &tailscale::RootRoute::Proxy(tailscale::proxy_target(9999)),
            "pc.tail.ts.net",
            Some(&claim),
        ));
        assert!(!route_is_available(
            &tailscale::RootRoute::Proxy(tailscale::proxy_target(1234)),
            "other.tail.ts.net",
            Some(&claim),
        ));
    }

    #[test]
    fn route_claim_survives_a_new_process() {
        let path = std::env::temp_dir().join(format!("swapper-route-test-{}.json", uuid::Uuid::new_v4()));
        let claim = OwnedRoute { dns_name: "pc.tail.ts.net".into(), port: 1234 };
        save_route_claim_at(&path, &claim).unwrap();
        let restored = load_route_claim_at(&path).unwrap().unwrap();
        assert_eq!(restored.dns_name, claim.dns_name);
        assert_eq!(restored.port, claim.port);
        let replacement = OwnedRoute { dns_name: claim.dns_name, port: 5678 };
        save_route_claim_at(&path, &replacement).unwrap();
        assert_eq!(load_route_claim_at(&path).unwrap().unwrap().port, 5678);
        std::fs::remove_file(path).unwrap();
    }
}
