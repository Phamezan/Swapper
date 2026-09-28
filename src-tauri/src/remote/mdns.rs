//! LAN discovery for Remote Control.
//!
//! Swapper advertises a single DNS-SD service (`_swapper._tcp.local.`) from a
//! stable `swapper.local` hostname while the LAN listener is actually serving on
//! a Private network. The TXT record carries only the installation id and a
//! protocol version: discovery answers *where* a trusted PC is, never *how* to
//! authenticate to it. Pairing credentials, cookies, and account data stay out
//! of mDNS entirely.
//!
//! Advertising is best effort. If the mdns-sd daemon cannot start, `start`
//! returns an error and the caller keeps serving the existing IP-based address
//! without surfacing anything to the user.

use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// The single advertised service type. The label before `._tcp` is at most 15
/// bytes, and `_swapper` is 8.
pub const SERVICE_TYPE: &str = "_swapper._tcp.local.";
/// Stable hostname phones can open directly, e.g. `http://swapper.local:38127`.
pub const LOCAL_HOST: &str = "swapper.local.";
/// The same name without the DNS root label, as a browser sends it in `Host`.
pub const LOCAL_NAME: &str = "swapper.local";
/// Stable, human-readable service instance name. mDNS resolves conflicts by
/// renaming, but a single Swapper PC per LAN keeps this name.
pub const INSTANCE_NAME: &str = "Swapper";
/// Bumped if the TXT contract or remote protocol changes in a breaking way.
pub const PROTOCOL_VERSION: &str = "1";

/// Owns the mDNS daemon while a service is registered. Dropping it withdraws
/// the records and shuts the daemon down, so LAN Remote Control stopping or
/// switching interfaces also stops the advertisement.
pub struct Advertiser {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertiser {
    /// Starts advertising one installation at `address:port`. The returned
    /// handle keeps the daemon alive; drop it to withdraw.
    pub fn start(installation_id: &str, address: Ipv4Addr, port: u16) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|error| error.to_string())?;
        let info = service_info(installation_id, address, port)?;
        let fullname = info.get_fullname().to_string();
        daemon
            .register(info)
            .map_err(|error| error.to_string())?;
        Ok(Self { daemon, fullname })
    }
}

impl Drop for Advertiser {
    fn drop(&mut self) {
        // Withdraw first so other devices see a goodbye; the daemon processes
        // queued commands in order before shutting down. Best effort: mDNS
        // failures must never interfere with taking LAN Remote Control down.
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// Builds the advertised service record. Separated from [`Advertiser`] so the
/// naming and TXT contents can be asserted without a live daemon.
pub fn service_info(
    installation_id: &str,
    address: Ipv4Addr,
    port: u16,
) -> Result<ServiceInfo, String> {
    let properties = txt_records(installation_id);
    ServiceInfo::new(
        SERVICE_TYPE,
        INSTANCE_NAME,
        LOCAL_HOST,
        address.to_string(),
        port,
        properties.as_slice(),
    )
    .map_err(|error| error.to_string())
}

/// The only metadata Swapper publishes: which installation this is and which
/// protocol version it speaks. Never pairing credentials, cookies, account
/// data, or tokens.
pub fn txt_records(installation_id: &str) -> Vec<(&'static str, String)> {
    vec![
        ("id", installation_id.to_string()),
        ("v", PROTOCOL_VERSION.to_string()),
    ]
}

/// The address Settings shows and a phone browser can open. Uses the stable
/// hostname so it survives LAN IP changes.
pub fn local_url(port: u16) -> String {
    format!("http://{LOCAL_NAME}:{port}")
}

/// Whether a request's `Host` header addresses the mDNS name (with or without
/// a port or trailing dot). The handoff endpoint only accepts this origin, so
/// the credential cookie lands on `swapper.local` rather than an IP.
pub fn is_local_host(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.');
    let name = host.split(':').next().unwrap_or_default();
    name.eq_ignore_ascii_case(LOCAL_NAME)
}

/// Whether to advertise, and at what address, given the current request. Kept
/// pure so the advertise/withdraw decision is unit tested: advertising happens
/// only when Remote Control is enabled and a private LAN endpoint is serving.
pub fn advertise_target(
    enabled: bool,
    lan: Option<(Ipv4Addr, u16)>,
) -> Option<(Ipv4Addr, u16)> {
    if enabled {
        lan
    } else {
        None
    }
}

/// 32 lowercase hex characters, the simple form of a UUIDv4.
pub fn valid_installation_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Keeps a stored id that is still valid, otherwise mints a fresh one. Pure so
/// the "generate once, then reuse" behaviour can be tested.
pub fn installation_id_from(raw: Option<&str>) -> String {
    raw.map(str::trim)
        .filter(|value| valid_installation_id(value))
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string())
}

fn installation_id_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    Ok(PathBuf::from(root).join("Swapper").join("installation-id"))
}

/// Loads the persisted installation id, creating and saving a new one the
/// first time. The id is not a secret, but it must be stable across restarts
/// so a phone can tell this PC apart from another Swapper installation.
pub fn load_or_create_installation_id() -> Result<String, String> {
    let path = installation_id_path()?;
    let existing = fs::read_to_string(&path).ok();
    let id = installation_id_from(existing.as_deref());
    if existing.as_deref().map(str::trim) != Some(id.as_str()) {
        save_installation_id(&path, &id)?;
    }
    Ok(id)
}

fn save_installation_id(path: &Path, id: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid installation id path")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::write(path, id.as_bytes()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_records_expose_only_installation_id_and_version() {
        let records = txt_records("0123456789abcdef0123456789abcdef");
        assert_eq!(
            records,
            vec![
                ("id", "0123456789abcdef0123456789abcdef".to_string()),
                ("v", "1".to_string()),
            ]
        );
        let joined = records
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&")
            .to_ascii_lowercase();
        for secret in [
            "credential", "secret", "token", "cookie", "session", "password", "puuid", "vault",
        ] {
            assert!(!joined.contains(secret), "TXT leaked {secret}");
        }
    }

    #[test]
    fn service_info_uses_stable_hostname_instance_and_type() {
        let info = service_info(
            "0123456789abcdef0123456789abcdef",
            Ipv4Addr::new(192, 168, 1, 20),
            38127,
        )
        .expect("service info");
        assert_eq!(info.get_type(), SERVICE_TYPE);
        assert_eq!(info.get_hostname(), LOCAL_HOST);
        assert_eq!(info.get_port(), 38127);
        assert_eq!(
            info.get_property_val_str("id"),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert_eq!(info.get_property_val_str("v"), Some("1"));
        assert!(info.get_fullname().ends_with(SERVICE_TYPE));
        assert!(info.get_fullname().starts_with(INSTANCE_NAME));
        assert_eq!(
            info.get_addresses_v4().into_iter().copied().collect::<Vec<_>>(),
            vec![Ipv4Addr::new(192, 168, 1, 20)]
        );
    }

    #[test]
    fn local_url_is_stable_and_uses_the_hostname() {
        assert_eq!(local_url(38127), "http://swapper.local:38127");
    }

    #[test]
    fn only_the_local_hostname_is_accepted_for_handoff() {
        assert!(is_local_host("swapper.local"));
        assert!(is_local_host("swapper.local:38127"));
        assert!(is_local_host("SWAPPER.LOCAL:38127"));
        assert!(is_local_host("swapper.local."));
        assert!(!is_local_host("192.168.1.20:38127"));
        assert!(!is_local_host("swapper.local.evil.example"));
        assert!(!is_local_host("notswapper.local"));
        assert!(!is_local_host(""));
    }

    #[test]
    fn advertising_requires_enabled_and_a_private_lan_endpoint() {
        let endpoint = (Ipv4Addr::new(192, 168, 1, 20), 38127);
        assert_eq!(advertise_target(true, Some(endpoint)), Some(endpoint));
        assert_eq!(advertise_target(false, Some(endpoint)), None);
        assert_eq!(advertise_target(true, None), None);
        assert_eq!(advertise_target(false, None), None);
    }

    #[test]
    fn installation_id_is_reused_when_valid_and_regenerated_otherwise() {
        let valid = "0123456789abcdef0123456789abcdef";
        assert_eq!(installation_id_from(Some(valid)), valid);
        assert_eq!(installation_id_from(Some(&format!("  {valid}  "))), valid);

        let generated = installation_id_from(Some("not-a-uuid"));
        assert_ne!(generated, "not-a-uuid");
        assert!(valid_installation_id(&generated));
        assert!(valid_installation_id(&installation_id_from(None)));
        assert!(!valid_installation_id(""));
        assert!(!valid_installation_id(&"z".repeat(32)));
    }

    #[test]
    fn installation_id_survives_a_new_process() {
        let directory =
            std::env::temp_dir().join(format!("swapper-mdns-test-{}", uuid::Uuid::new_v4()));
        let path = directory.join("installation-id");
        let first = installation_id_from(None);
        save_installation_id(&path, &first).unwrap();
        let reloaded = installation_id_from(fs::read_to_string(&path).ok().as_deref());
        assert_eq!(reloaded, first);
        fs::remove_dir_all(directory).unwrap();
    }
}
