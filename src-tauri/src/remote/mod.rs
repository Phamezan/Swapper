mod control;
mod lcu;
mod server;
mod tailscale;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;
use tokio::sync::{broadcast, oneshot};

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
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<thread::JoinHandle<()>>,
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
    owned_route: Mutex<Option<OwnedRoute>>,
}

impl RemoteCore {
    pub fn new(app: AppHandle, enabled: bool) -> Arc<Self> {
        let (events, _) = broadcast::channel(32);
        let status = if enabled {
            RemoteStatus {
                enabled: true,
                state: RemoteState::Starting,
                ..RemoteStatus::disabled()
            }
        } else {
            RemoteStatus::disabled()
        };
        let core = Arc::new(Self {
            app,
            ops: Mutex::new(()),
            inner: Mutex::new(Inner { status, epoch: 0 }),
            service: Mutex::new(None),
            events,
            owned_route: Mutex::new(None),
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

    pub fn subscribe(&self) -> broadcast::Receiver<RemoteStatus> {
        self.events.subscribe()
    }

    pub fn set_enabled(self: Arc<Self>, enabled: bool) {
        let _ops = lock(&self.ops);
        if !enabled {
            self.apply_disabled();
            let recorded = route_claim_path()
                .and_then(|path| load_route_claim_at(&path))
                .ok()
                .flatten();
            if let Some(owned) = lock(&self.owned_route).clone().or(recorded) {
                if let Some(cli) = tailscale::find_cli() {
                    if matches!(tailscale::root_route(&cli, &owned.dns_name), Ok(tailscale::RootRoute::Proxy(ref target)) if target == &tailscale::proxy_target(owned.port))
                        && tailscale::serve_off(&cli).is_ok() {
                            *lock(&self.owned_route) = None;
                            if let Ok(path) = route_claim_path() {
                                let _ = fs::remove_file(path);
                            }
                    }
                }
            }
            self.stop_service();
            self.probe_tailscale_without_remote();
            return;
        }
        self.apply(|status| {
            status.enabled = true;
            status.state = RemoteState::Starting;
            status.message = None;
        });
        if let Err(err) = Self::ensure_service(&self) {
            self.apply(|status| {
                status.state = RemoteState::Failed;
                status.message = Some(err);
            });
            return;
        }
        self.reconcile();
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

    fn service_port(&self) -> Option<u16> {
        lock(&self.service).as_ref().map(|service| service.port)
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
            let _ = join.join();
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
        let port = match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(port)) => port,
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
            port,
            shutdown: Some(shutdown_tx),
            join: Some(join),
        });
        Ok(())
    }

    fn reconcile(&self) {
        let epoch = lock(&self.inner).epoch;
        let Some(cli) = tailscale::find_cli() else {
            self.apply_if_current(epoch, |status| {
                status.state = RemoteState::NotInstalled;
                status.tailscale_installed = false;
                status.tailscale_running = false;
                status.address = None;
                status.message =
                    Some("Tailscale is not installed. Install Tailscale, then turn Remote Control off and on again.".into());
            });
            return;
        };
        match tailscale::status(&cli) {
            Err(err) => self.apply_if_current(epoch, |status| {
                status.state = RemoteState::Disconnected;
                status.tailscale_installed = true;
                status.tailscale_running = false;
                status.address = None;
                status.message = Some(err);
            }),
            Ok(ts) => {
                let running = ts.backend_state.eq_ignore_ascii_case("Running");
                if !running {
                    self.apply_if_current(epoch, |status| {
                        status.state = RemoteState::Disconnected;
                        status.tailscale_installed = true;
                        status.tailscale_running = false;
                        status.dns_name = ts.dns_name.clone();
                        status.address = None;
                        status.message =
                            Some("Tailscale is not connected. Open Tailscale and sign in, then turn Remote Control off and on again.".into());
                    });
                    return;
                }
                let address = ts
                    .dns_name
                    .as_deref()
                    .and_then(tailscale::address_for);
                let Some(address) = address else {
                    self.apply_if_current(epoch, |status| {
                        status.state = RemoteState::Failed;
                        status.tailscale_installed = true;
                        status.tailscale_running = true;
                        status.dns_name = ts.dns_name.clone();
                        status.address = None;
                        status.message =
                            Some("Tailscale is connected but did not report a MagicDNS name.".into());
                    });
                    return;
                };
                let Some(port) = self.service_port() else {
                    self.apply_if_current(epoch, |status| {
                        status.state = RemoteState::Failed;
                        status.message = Some("The remote service is not running.".into());
                    });
                    return;
                };
                let owned = route_claim_path()
                    .and_then(|path| load_route_claim_at(&path))
                    .inspect_err(|err| {
                        self.apply_if_current(epoch, |status| {
                            status.state = RemoteState::Failed;
                            status.address = None;
                            status.message = Some(err.clone());
                        });
                    });
                let Ok(owned) = owned else { return; };
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
                            status.message = None;
                        });
                    }
                    Err(err) => self.apply_if_current(epoch, |status| {
                        status.state = RemoteState::Failed;
                        status.tailscale_installed = true;
                        status.tailscale_running = true;
                        status.dns_name = ts.dns_name.clone();
                        status.address = None;
                        status.message = Some(err);
                    }),
                }
            }
        }
    }
}

fn service_thread(
    core: Arc<RemoteCore>,
    ready: mpsc::SyncSender<Result<u16, String>>,
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
        let _ = ready.send(Ok(port));
        tokio::spawn(lcu::watch(core.clone()));
        let router = server::router(core);
        let mut shutdown = shutdown;
        let _ = tokio::select! {
            result = axum::serve(listener, router) => result.map_err(|e| e.to_string()),
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
