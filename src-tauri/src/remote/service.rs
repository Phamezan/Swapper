use super::pairing::lan_reconnect_change;
use super::{lcu, lock, mdns, network, server, RemoteCore, RemoteState};
use std::sync::{mpsc, Arc, Weak};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

// How long shutdown waits for the axum service thread to stop before letting
// the process exit without it.
const SERVICE_STOP_TIMEOUT: Duration = Duration::from_secs(2);

// Keep a paired phone's saved address usable after Swapper restarts.
const LAN_REMOTE_PORT: u16 = 38127;

/// Shown instead of prompting when the firewall rule is missing.
pub(super) const FIREWALL_NEEDED_MESSAGE: &str =
    "LAN access needs permission in Windows Firewall. Choose Allow on private networks.";

pub(super) struct Service {
    pub(super) port: u16,
    pub(super) lan: Option<LanEndpoint>,
    pub(super) lan_message: Option<String>,
    /// True while the mDNS service for this LAN endpoint is registered.
    pub(super) advertising: bool,
    /// The address the service intended to serve on, even if binding or the
    /// firewall check later failed. Used to notice interface/profile changes
    /// without repeatedly restarting when LAN cannot start.
    lan_target: Option<std::net::Ipv4Addr>,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<thread::JoinHandle<()>>,
}

#[derive(Clone)]
pub(super) struct LanEndpoint {
    pub(super) address: std::net::Ipv4Addr,
    pub(super) port: u16,
}

struct ServiceInfo {
    port: u16,
    lan: Option<LanEndpoint>,
    lan_message: Option<String>,
    advertising: bool,
    lan_target: Option<std::net::Ipv4Addr>,
}

impl RemoteCore {
    pub(super) fn stop_service(&self) {
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

    pub(super) fn ensure_service(core: &Arc<Self>) -> Result<(), String> {
        {
            let mut guard = lock(&core.service);
            let alive = guard.as_ref().is_some_and(|service| {
                service
                    .join
                    .as_ref()
                    .is_some_and(|join| !join.is_finished())
            });
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
            advertising: info.advertising,
            lan_target: info.lan_target,
            shutdown: Some(shutdown_tx),
            join: Some(join),
        });
        Ok(())
    }
}

fn service_thread(
    core: Arc<RemoteCore>,
    ready: mpsc::SyncSender<Result<ServiceInfo, String>>,
    shutdown: oneshot::Receiver<()>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
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
        let (lan, lan_message, lan_target): (
            Option<LanEndpoint>,
            Option<String>,
            Option<std::net::Ipv4Addr>,
        ) = match network::default_interface() {
            Err(error) => (None, Some(error), None),
            Ok(None) => (None, Some("No active Ethernet or Wi-Fi interface with a private IPv4 default route was found.".into()), None),
            Ok(Some(interface)) => match network::is_private_profile(&interface.adapter_id) {
                Err(error) => (None, Some(error), None),
                Ok(false) => (None, Some("LAN access is off because the active Windows network is not set to Private.".into()), None),
                // Checked before binding: listening without a rule would make
                // Windows pop its own firewall prompt on startup.
                Ok(true) if !network::private_app_allowed() => {
                    (None, Some(FIREWALL_NEEDED_MESSAGE.into()), Some(interface.address))
                }
                Ok(true) => {
                    let target = Some(interface.address);
                    match tokio::net::TcpListener::bind((interface.address, LAN_REMOTE_PORT)).await {
                        Err(error) => (None, Some(format!(
                            "Could not listen on {} ({}:{LAN_REMOTE_PORT}): {error}",
                            interface.name,
                            interface.address
                        )), target),
                        Ok(listener) => match listener.local_addr() {
                            Err(error) => (None, Some(format!("Could not read the LAN listener address: {error}")), target),
                            Ok(address) => {
                                let endpoint = LanEndpoint {
                                    address: interface.address,
                                    port: address.port(),
                                };
                                lan_listener = Some(listener);
                                (Some(endpoint), None, target)
                            }
                        },
                    }
                }
            },
        };
        // Advertise only when Remote Control is enabled and a private LAN
        // endpoint is actually serving. A failed daemon just means no
        // discovery; the IP-based flow is untouched.
        let target = mdns::advertise_target(
            core.installation_id.is_some(),
            lan.as_ref().map(|endpoint| (endpoint.address, endpoint.port)),
        );
        let advertiser = match (core.installation_id.as_deref(), target) {
            (Some(id), Some((address, port))) => {
                mdns::Advertiser::start(id, address, port).ok()
            }
            _ => None,
        };
        let advertising = advertiser.is_some();
        // Hold the advertiser for the lifetime of the service thread so the
        // records are withdrawn when this listener stops.
        let _advertiser = advertiser;
        let _ = ready.send(Ok(ServiceInfo { port, lan, lan_message, advertising, lan_target }));
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

pub(super) fn maintain(weak: Weak<RemoteCore>) {
    loop {
        thread::sleep(Duration::from_secs(30));
        let Some(core) = weak.upgrade() else {
            return;
        };
        let (enabled, state, crashed) = {
            let inner = lock(&core.inner);
            let crashed = lock(&core.service).as_ref().is_some_and(|service| {
                service.join.as_ref().is_some_and(|join| join.is_finished())
            });
            (inner.status.enabled, inner.status.state, crashed)
        };
        if !enabled {
            // Keep the maintainer alive for a later Settings toggle.
            continue;
        }
        // The LAN interface (or its Windows profile) can change while Swapper
        // runs: a DHCP lease change, a Wi-Fi/Ethernet switch, or a Private to
        // Public toggle. Restart the service so the listener rebinds and mDNS
        // re-advertises the new address, or withdraws entirely when LAN stops.
        // The probe runs outside the service lock, which must stay cheap.
        let desired_lan = desired_lan_address();
        let previous_lan = lock(&core.service)
            .as_ref()
            .map(|service| service.lan_target);
        let lan_changed = previous_lan.is_some_and(|target| target != desired_lan);
        let unhealthy = crashed
            || lan_changed
            || matches!(
                state,
                RemoteState::NotInstalled | RemoteState::Disconnected | RemoteState::Failed
            );
        if !unhealthy {
            continue;
        }
        let _ops = lock(&core.ops);
        if lan_changed {
            // A paired phone's cookie is bound to the old address, so flag the
            // change before the service drops the old listener.
            if let Some((previous, next)) =
                lan_reconnect_change(previous_lan.flatten(), desired_lan)
            {
                core.record_lan_reconnect(previous, next);
            }
            core.stop_service();
        }
        let _ = RemoteCore::ensure_service(&core);
        core.reconcile();
    }
}

/// The LAN address Remote Control should currently be serving, or `None` when
/// there is no usable interface or the active network is not Private.
fn desired_lan_address() -> Option<std::net::Ipv4Addr> {
    let interface = network::default_interface().ok().flatten()?;
    network::is_private_profile_cached(&interface.adapter_id)
        .ok()
        .filter(|private| *private)
        .map(|_| interface.address)
}
