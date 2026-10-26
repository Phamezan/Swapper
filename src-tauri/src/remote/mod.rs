mod control;
mod handoff;
mod lcu;
mod mdns;
pub(crate) mod network;
mod pairing;
mod server;
mod service;
mod tailscale;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Instant;

use serde::Serialize;
use tauri::AppHandle;
use tokio::sync::broadcast;

pub use pairing::PairedDeviceView;
use pairing::{load_paired_devices_at, paired_devices_path, LanAccess};
use service::{maintain, Service, FIREWALL_NEEDED_MESSAGE};

pub(crate) fn network_settings_page() -> &'static str {
    network::settings_page()
}

/// The full app's champion select hovers and locks with the remote's actions.
pub(crate) async fn prepick_champion(champion_id: i64) -> Result<(), String> {
    control::prepick_champion(champion_id)
        .await
        .map_err(|e| e.message)
}

/// Hovers the champion on your current pick or ban action.
pub(crate) async fn select_champion(champion_id: i64) -> Result<(), String> {
    control::select_champion(champion_id)
        .await
        .map_err(|e| e.message)
}

pub(crate) use control::PickConfirmation;

pub(crate) async fn lock_confirmed_pick(expected: PickConfirmation) -> Result<(), String> {
    control::lock_confirmed_pick(expected)
        .await
        .map_err(|e| e.message)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

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
    /// LAN is waiting for the user to allow Swapper in Windows Firewall.
    /// Swapper never asks on its own; the Remote tab offers the button.
    pub lan_firewall_needed: bool,
    pub message: Option<String>,
    pub tailscale_installed: bool,
    pub tailscale_running: bool,
    pub dns_name: Option<String>,
    /// Stable `http://swapper.local:<port>` address, present only while the
    /// mDNS service is actually advertised.
    pub local_address: Option<String>,
    pub league_running: bool,
    pub lcu_connected: bool,
    /// Set while paired phones must scan the QR again because the PC's LAN
    /// address changed. Never set on startup or while LAN is simply off.
    pub lan_reconnect_needed: bool,
    /// Private LAN address the phone paired against, before the change.
    pub lan_reconnect_old_address: Option<String>,
    /// Private LAN address the PC serves now. Phones must re-pair to it.
    pub lan_reconnect_new_address: Option<String>,
    /// Unix time of the change. Devices not seen since then are "old" and can
    /// be removed in one action.
    pub lan_reconnect_since: Option<u64>,
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
            lan_firewall_needed: false,
            message: None,
            tailscale_installed: false,
            tailscale_running: false,
            dns_name: None,
            local_address: None,
            league_running: false,
            lcu_connected: false,
            lan_reconnect_needed: false,
            lan_reconnect_old_address: None,
            lan_reconnect_new_address: None,
            lan_reconnect_since: None,
        }
    }
}

/// Read-only Remote Control summary for Swapper Doctor. Carries interface
/// names only — never addresses, credentials, or pairing data. Deliberately
/// excludes Tailscale: the doctor probes it separately, and only when the
/// Tailscale transport is selected.
pub(crate) struct DoctorSummary {
    pub enabled: bool,
    pub state: RemoteState,
    pub lan_ready: bool,
    pub lan_message: Option<String>,
    pub message: Option<String>,
    /// Selected adapter's friendly name, without its address.
    pub interface: Option<String>,
}

struct Inner {
    status: RemoteStatus,
    epoch: u64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct OwnedRoute {
    dns_name: String,
    port: u16,
}

fn route_claim_path() -> Result<PathBuf, String> {
    Ok(crate::vault::data_root()?.join("remote-route.json"))
}

fn load_route_claim_at(path: &Path) -> Result<Option<OwnedRoute>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("Could not read Swapper's Tailscale route record: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!(
            "Could not read Swapper's Tailscale route record: {e}"
        )),
    }
}

fn save_route_claim_at(path: &Path, claim: &OwnedRoute) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid Tailscale route record path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let pending = parent.join(format!("remote-route-{}.tmp", uuid::Uuid::new_v4()));
    fs::write(
        &pending,
        serde_json::to_vec(claim).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&pending, path) {
        let _ = fs::remove_file(&pending);
        return Err(format!(
            "Could not save Swapper's Tailscale route record: {e}"
        ));
    }
    Ok(())
}

fn route_is_available(
    route: &tailscale::RootRoute,
    dns_name: &str,
    owned: Option<&OwnedRoute>,
) -> bool {
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
    /// Stable public identity for mDNS discovery. Not a secret; `None` only
    /// when the Swapper data folder is unavailable, which silently disables
    /// advertising.
    installation_id: Option<String>,
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
                handoff: handoff::HandoffTokens::default(),
            }),
            installation_id: mdns::load_or_create_installation_id().ok(),
        });
        let weak = Arc::downgrade(&core);
        thread::Builder::new()
            .name("swapper-remote-maintainer".into())
            .spawn(move || maintain(weak))
            .expect("Could not start the remote maintainer");
        core
    }

    pub fn status(&self) -> RemoteStatus {
        let mut status = lock(&self.inner).status.clone();
        status.lan_firewall_needed = status.lan_message.as_deref() == Some(FIREWALL_NEEDED_MESSAGE);
        status
    }

    /// Asks Windows (one elevation prompt showing Swapper) to allow Swapper on
    /// Private networks, then restarts the service so LAN starts listening.
    pub(crate) fn allow_lan_firewall(self: &Arc<Self>) -> Result<(), String> {
        network::request_private_app_rule()?;
        let _ops = lock(&self.ops);
        self.stop_service();
        let _ = Self::ensure_service(self);
        self.reconcile();
        Ok(())
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
            crate::refresh_tray_tooltip(&self.app);
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

    /// Read-only diagnostics for Swapper Doctor: the cached service status
    /// plus a fresh adapter lookup. Starts nothing and changes nothing. This
    /// never probes Tailscale; see [`Self::tailscale_availability`].
    pub(crate) fn doctor_summary(&self) -> DoctorSummary {
        let status = self.status();
        let interface = network::default_interface().ok().flatten();
        DoctorSummary {
            enabled: status.enabled,
            state: status.state,
            lan_ready: status.lan_address.is_some(),
            lan_message: status.lan_message,
            message: status.message,
            interface: interface.map(|item| item.name),
        }
    }

    /// Read-only Tailscale availability for Swapper Doctor: installed, and
    /// whether the backend is running. Runs one bounded `tailscale status`
    /// call, so the doctor invokes it only for the Tailscale check.
    pub(crate) fn tailscale_availability(&self) -> (bool, bool) {
        match tailscale::find_cli() {
            Some(cli) => match tailscale::status(&cli) {
                Ok(ts) => (true, ts.backend_state.eq_ignore_ascii_case("Running")),
                Err(_) => (true, false),
            },
            None => (false, false),
        }
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

    fn reconcile(&self) {
        let epoch = lock(&self.inner).epoch;
        let (port, lan_address, lan_message, local_address) = {
            let service = lock(&self.service);
            let Some(service) = service.as_ref() else {
                self.apply_if_current(epoch, |status| {
                    status.state = RemoteState::Failed;
                    status.address = None;
                    status.lan_address = None;
                    status.local_address = None;
                    status.message = Some("The remote service is not running.".into());
                });
                return;
            };
            let lan_address = service
                .lan
                .as_ref()
                .map(|lan| format!("http://{}:{}", lan.address, lan.port));
            let local_address = if service.advertising {
                service.lan.as_ref().map(|lan| mdns::local_url(lan.port))
            } else {
                None
            };
            (
                service.port,
                lan_address,
                service.lan_message.clone(),
                local_address,
            )
        };
        // The discovery address is derived from whichever LAN endpoint is
        // serving, so set it before the transport branches below.
        self.apply_if_current(epoch, |status| {
            status.local_address = local_address;
        });
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
                    status.message = lan_message.clone().or_else(|| {
                        Some("Tailscale is not installed and LAN access is unavailable.".into())
                    });
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
                            status.message = lan_message
                                .clone()
                                .or(Some("Tailscale is not connected.".into()));
                        }
                    });
                    return;
                }
                let address = ts.dns_name.as_deref().and_then(tailscale::address_for);
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

#[cfg(test)]
mod route_recovery_tests {
    use super::*;

    #[test]
    fn a_recorded_route_can_be_replaced_after_restart() {
        let claim = OwnedRoute {
            dns_name: "pc.tail.ts.net".into(),
            port: 1234,
        };
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
        let path =
            std::env::temp_dir().join(format!("swapper-route-test-{}.json", uuid::Uuid::new_v4()));
        let claim = OwnedRoute {
            dns_name: "pc.tail.ts.net".into(),
            port: 1234,
        };
        save_route_claim_at(&path, &claim).unwrap();
        let restored = load_route_claim_at(&path).unwrap().unwrap();
        assert_eq!(restored.dns_name, claim.dns_name);
        assert_eq!(restored.port, claim.port);
        let replacement = OwnedRoute {
            dns_name: claim.dns_name,
            port: 5678,
        };
        save_route_claim_at(&path, &replacement).unwrap();
        assert_eq!(load_route_claim_at(&path).unwrap().unwrap().port, 5678);
        std::fs::remove_file(path).unwrap();
    }
}
