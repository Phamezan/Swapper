use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailscaleStatus {
    pub backend_state: String,
    pub dns_name: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RootRoute {
    Vacant,
    Proxy(String),
    Other,
}

#[derive(Deserialize)]
struct StatusFile {
    #[serde(rename = "BackendState")]
    backend_state: Option<String>,
    #[serde(rename = "Self")]
    node: Option<Node>,
    #[serde(rename = "MagicDNSSuffix")]
    magic_dns_suffix: Option<String>,
}

#[derive(Deserialize)]
struct Node {
    #[serde(rename = "DNSName")]
    dns_name: Option<String>,
    #[serde(rename = "HostName")]
    host_name: Option<String>,
}

pub fn find_cli() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for env in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(env) {
            candidates.push(PathBuf::from(root).join("Tailscale").join("tailscale.exe"));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Tailscale")
                .join("tailscale.exe"),
        );
    }
    if let Some(found) = candidates.into_iter().find(|path| path.is_file()) {
        return Some(found);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("tailscale.exe"))
        .find(|path| path.is_file())
}

pub fn status(cli: &Path) -> Result<TailscaleStatus, String> {
    let output = capture(cli, &["status", "--json"])?;
    if !output.status.success() {
        return Err(failure("Tailscale is not reachable", &output));
    }
    let body = String::from_utf8_lossy(&output.stdout);
    parse_status(&body)
}

pub fn serve_set(cli: &Path, port: u16) -> Result<(), String> {
    let target = format!("http://127.0.0.1:{port}");
    let output = capture(cli, &["serve", "--bg", "--set-path", "/", &target])?;
    if !output.status.success() {
        return Err(failure("Could not expose Swapper through Tailscale Serve", &output));
    }
    Ok(())
}

pub fn root_route(cli: &Path, dns_name: &str) -> Result<RootRoute, String> {
    let output = capture(cli, &["serve", "status", "--json"])?;
    if !output.status.success() {
        return Err(failure("Could not inspect Tailscale Serve routes", &output));
    }
    parse_root_route(&String::from_utf8_lossy(&output.stdout), dns_name)
}

fn parse_root_route(body: &str, dns_name: &str) -> Result<RootRoute, String> {
    if body.trim().is_empty() || body.trim() == "null" {
        return Ok(RootRoute::Vacant);
    }
    let config: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| format!("Tailscale returned unreadable Serve routes: {e}"))?;
    let Some(web) = config.get("Web") else {
        return Ok(RootRoute::Vacant);
    };
    let web = web.as_object().ok_or("Tailscale returned invalid Serve routes")?;
    let host_port = format!("{}:443", dns_name.trim_end_matches('.'));
    let Some((_, server)) = web.iter().find(|(key, _)| key.eq_ignore_ascii_case(&host_port)) else {
        return Ok(RootRoute::Vacant);
    };
    let Some(root) = server.get("Handlers").and_then(|handlers| handlers.get("/")) else {
        return Ok(RootRoute::Vacant);
    };
    Ok(match root.get("Proxy").and_then(|proxy| proxy.as_str()) {
        Some(proxy) => RootRoute::Proxy(proxy.to_string()),
        None => RootRoute::Other,
    })
}

pub fn proxy_target(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

pub fn serve_off(cli: &Path) -> Result<(), String> {
    let output = capture(cli, &["serve", "--set-path", "/", "off"])?;
    if !output.status.success() {
        let text = describe(&output);
        if text.contains("does not exist") {
            return Ok(());
        }
        return Err(failure("Could not remove the Tailscale Serve route", &output));
    }
    Ok(())
}

pub fn address_for(dns_name: &str) -> Option<String> {
    let host = dns_name.trim().trim_end_matches('.');
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    Some(format!("https://{host}"))
}

pub fn parse_status(body: &str) -> Result<TailscaleStatus, String> {
    let parsed: StatusFile =
        serde_json::from_str(body).map_err(|e| format!("Tailscale returned unreadable status: {e}"))?;
    let backend_state = parsed
        .backend_state
        .filter(|state| !state.trim().is_empty())
        .ok_or("Tailscale did not report its state")?;
    let node = parsed.node;
    let dns_name = node
        .as_ref()
        .and_then(|node| node.dns_name.as_deref())
        .map(|name| name.trim().trim_end_matches('.').to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            let host = node.as_ref()?.host_name.as_deref()?.trim();
            if host.is_empty() {
                return None;
            }
            let suffix_source = parsed.magic_dns_suffix?;
            let suffix = suffix_source.trim().trim_end_matches('.');
            if suffix.is_empty() {
                return None;
            }
            Some(format!("{}.{}", host.to_lowercase(), suffix))
        });
    Ok(TailscaleStatus {
        backend_state,
        dns_name,
    })
}

fn capture(cli: &Path, args: &[&str]) -> Result<Output, String> {
    let mut command = Command::new(cli);
    command.args(args).creation_flags(CREATE_NO_WINDOW);
    command
        .output()
        .map_err(|e| format!("Could not run Tailscale: {e}"))
}

fn describe(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text.trim().to_string()
}

fn failure(prefix: &str, output: &Output) -> String {
    let detail = describe(output);
    if detail.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_magic_dns_name_from_status() {
        let body = r#"{"BackendState":"Running","Self":{"DNSName":"gaming-pc.tailnet.ts.net.","HostName":"GAMING-PC"},"MagicDNSSuffix":"tailnet.ts.net"}"#;
        let status = parse_status(body).unwrap();
        assert_eq!(status.backend_state, "Running");
        assert_eq!(status.dns_name.as_deref(), Some("gaming-pc.tailnet.ts.net"));
    }

    #[test]
    fn falls_back_to_host_and_suffix_when_dns_name_is_missing() {
        let body = r#"{"BackendState":"Running","Self":{"HostName":"GAMING-PC"},"MagicDNSSuffix":"tailnet.ts.net."}"#;
        let status = parse_status(body).unwrap();
        assert_eq!(status.dns_name.as_deref(), Some("gaming-pc.tailnet.ts.net"));
    }

    #[test]
    fn rejects_addresses_without_a_domain() {
        assert_eq!(address_for("gaming-pc"), None);
        assert_eq!(
            address_for("gaming-pc.tailnet.ts.net."),
            Some("https://gaming-pc.tailnet.ts.net".to_string())
        );
    }

    #[test]
    fn reads_only_the_node_https_root_route() {
        let body = r#"{"Web":{"pc.tail.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:1234"},"/other":{"Proxy":"http://127.0.0.1:9999"}}},"pc.tail.ts.net:80":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:8888"}}}}}"#;
        assert_eq!(parse_root_route(body, "pc.tail.ts.net").unwrap(), RootRoute::Proxy(proxy_target(1234)));
        assert_eq!(parse_root_route(body, "different.tail.ts.net").unwrap(), RootRoute::Vacant);
    }

    #[test]
    fn preserves_non_proxy_root_and_rejects_unreadable_status() {
        let body = r#"{"Web":{"pc.tail.ts.net:443":{"Handlers":{"/":{"Path":"C:\\site"}}}}}"#;
        assert_eq!(parse_root_route(body, "pc.tail.ts.net").unwrap(), RootRoute::Other);
        assert!(parse_root_route("not json", "pc.tail.ts.net").is_err());
        assert_eq!(parse_root_route("{}", "pc.tail.ts.net").unwrap(), RootRoute::Vacant);
    }
}
