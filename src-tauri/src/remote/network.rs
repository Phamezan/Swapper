use std::net::Ipv4Addr;
use std::process::Command;

use base64::Engine;
use windows_sys::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST,
    GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

const VIRTUAL_ADAPTER_MARKERS: [&str; 10] = [
    "tailscale", "wsl", "docker", "hyper-v", "vmware", "virtual", "vpn", "wireguard", "tunnel", "tap",
];

pub struct LanInterface {
    pub address: Ipv4Addr,
    pub name: String,
    pub index: u32,
    pub is_wifi: bool,
    metric: u32,
}

/// Returns the active default-route Ethernet or Wi-Fi interface, excluding
/// tunnels, virtual adapters, loopback, link-local, and non-private addresses.
pub fn default_interface() -> Result<Option<LanInterface>, String> {
    let mut size = 0u32;
    // FirstGatewayAddress is only filled in when gateways are requested.
    let flags = GAA_FLAG_INCLUDE_GATEWAYS
        | GAA_FLAG_SKIP_ANYCAST
        | GAA_FLAG_SKIP_MULTICAST
        | GAA_FLAG_SKIP_DNS_SERVER;
    let first = unsafe {
        GetAdaptersAddresses(AF_INET as u32, flags, std::ptr::null(), std::ptr::null_mut(), &mut size)
    };
    if size == 0 {
        return Ok(None);
    }
    if first != ERROR_BUFFER_OVERFLOW {
        return Err(format!("Could not inspect network adapters (Windows error {first})."));
    }

    let mut storage = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    let mut actual_size = size;
    let result = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            flags,
            std::ptr::null(),
            storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
            &mut actual_size,
        )
    };
    if result != 0 {
        return Err(format!("Could not inspect network adapters (Windows error {result})."));
    }

    let mut candidates = Vec::new();
    let mut adapter = storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    unsafe {
        while !adapter.is_null() {
            let current = &*adapter;
            let name = wide_string(current.FriendlyName);
            let normalized_name = name.to_ascii_lowercase();
            let description = wide_string(current.Description).to_ascii_lowercase();
            let virtual_adapter = is_virtual_adapter(&normalized_name, &description);
            if current.OperStatus == IfOperStatusUp
                && (current.IfType == IF_TYPE_ETHERNET_CSMACD || current.IfType == IF_TYPE_IEEE80211)
                && !current.FirstGatewayAddress.is_null()
                && current.Anonymous1.Anonymous.IfIndex != 0
                && !virtual_adapter
            {
                let mut unicast = current.FirstUnicastAddress;
                while !unicast.is_null() {
                    let item = &*unicast;
                    if !item.Address.lpSockaddr.is_null()
                        && item.Address.iSockaddrLength as usize >= std::mem::size_of::<SOCKADDR_IN>()
                    {
                        let sockaddr = &*item.Address.lpSockaddr.cast::<SOCKADDR_IN>();
                        if sockaddr.sin_family == AF_INET {
                            let raw = sockaddr.sin_addr.S_un.S_addr;
                            let address = Ipv4Addr::from(u32::from_be(raw));
                            if address.is_private() {
                                candidates.push(LanInterface {
                                    address,
                                    name: name.clone(),
                                    index: current.Anonymous1.Anonymous.IfIndex,
                                    is_wifi: current.IfType == IF_TYPE_IEEE80211,
                                    metric: current.Ipv4Metric,
                                });
                            }
                        }
                    }
                    unicast = item.Next;
                }
            }
            adapter = current.Next;
        }
    }
    candidates.sort_by_key(|item| item.metric);
    Ok(candidates.into_iter().next())
}

/// Opens the Settings category for the selected LAN adapter. Windows does not
/// document a URI that skips directly to an individual connection's profile.
pub fn settings_page() -> &'static str {
    match default_interface() {
        Ok(Some(interface)) if interface.is_wifi => "ms-settings:network-wifi",
        Ok(Some(_)) => "ms-settings:network-ethernet",
        _ => match active_physical_adapter_is_wifi() {
            Ok(Some(true)) => "ms-settings:network-wifi",
            Ok(Some(false)) => "ms-settings:network-ethernet",
            _ => "ms-settings:network-status",
        },
    }
}

/// Selects the most likely physical Ethernet or Wi-Fi adapter even when it
/// cannot be used for LAN sharing (for example, if it has no private IPv4).
fn active_physical_adapter_is_wifi() -> Result<Option<bool>, String> {
    let mut size = 0u32;
    // FirstGatewayAddress is only filled in when gateways are requested.
    let flags = GAA_FLAG_INCLUDE_GATEWAYS
        | GAA_FLAG_SKIP_ANYCAST
        | GAA_FLAG_SKIP_MULTICAST
        | GAA_FLAG_SKIP_DNS_SERVER;
    let first = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            flags,
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if size == 0 {
        return Ok(None);
    }
    if first != ERROR_BUFFER_OVERFLOW {
        return Err(format!("Could not inspect network adapters (Windows error {first})."));
    }

    let mut storage = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    let mut actual_size = size;
    let result = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            flags,
            std::ptr::null(),
            storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
            &mut actual_size,
        )
    };
    if result != 0 {
        return Err(format!("Could not inspect network adapters (Windows error {result})."));
    }

    let mut selected: Option<(bool, u32, bool)> = None;
    let mut adapter = storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    unsafe {
        while !adapter.is_null() {
            let current = &*adapter;
            let name = wide_string(current.FriendlyName).to_ascii_lowercase();
            let description = wide_string(current.Description).to_ascii_lowercase();
            let virtual_adapter = is_virtual_adapter(&name, &description);
            let is_wifi = current.IfType == IF_TYPE_IEEE80211;
            let ethernet_or_wifi = is_wifi || current.IfType == IF_TYPE_ETHERNET_CSMACD;
            if current.OperStatus == IfOperStatusUp
                && ethernet_or_wifi
                && current.Anonymous1.Anonymous.IfIndex != 0
                && !virtual_adapter
            {
                let candidate = (
                    current.FirstGatewayAddress.is_null(),
                    current.Ipv4Metric,
                    is_wifi,
                );
                if selected.map_or(true, |best| candidate < best) {
                    selected = Some(candidate);
                }
            }
            adapter = current.Next;
        }
    }
    Ok(selected.map(|(_, _, is_wifi)| is_wifi))
}

fn is_virtual_adapter(normalized_name: &str, normalized_description: &str) -> bool {
    VIRTUAL_ADAPTER_MARKERS.iter().any(|marker| {
        normalized_name.contains(marker) || normalized_description.contains(marker)
    })
}

#[cfg(test)]
mod tests {
    use super::is_virtual_adapter;

    #[test]
    fn excludes_virtualbox_when_friendly_name_is_generic() {
        assert!(is_virtual_adapter(
            "ethernet",
            "virtualbox host-only ethernet adapter"
        ));
    }

    #[test]
    fn keeps_wifi_adapter_with_generic_description() {
        assert!(!is_virtual_adapter(
            "wi-fi",
            "rz616 wi-fi 6e 160mhz"
        ));
    }
}

pub fn is_private_profile(interface_index: u32) -> Result<bool, String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "(Get-NetConnectionProfile -InterfaceIndex {interface_index} -ErrorAction Stop).NetworkCategory"
            ),
        ])
        .output()
        .map_err(|error| format!("Could not check the Windows network profile: {error}"))?;
    if !output.status.success() {
        return Err("Could not check the Windows network profile for the active interface.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().eq_ignore_ascii_case("Private"))
}

const FIREWALL_RULE: &str = "Swapper LAN Remote Control";

fn current_exe_for_powershell() -> Result<String, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not locate Swapper for the firewall rule: {error}"))?;
    Ok(executable.to_string_lossy().replace('\'', "''"))
}

/// PowerShell that exits 0 when a Swapper rule already allows this executable
/// and otherwise falls through to whatever follows it.
fn rule_present_check(executable: &str) -> String {
    format!(
        "$programs=Get-NetFirewallRule -DisplayName '{FIREWALL_RULE}' -ErrorAction SilentlyContinue | Get-NetFirewallApplicationFilter | ForEach-Object Program; if ($programs -contains '{executable}') {{ exit 0 }}"
    )
}

/// Whether Windows Firewall already allows this Swapper executable on Private
/// networks. Querying needs no elevation, so Swapper can explain the prompt
/// before asking for it.
pub fn private_app_allowed() -> bool {
    let Ok(executable) = current_exe_for_powershell() else {
        return false;
    };
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &format!("{}; exit 1", rule_present_check(&executable))])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Adds a Private-profile-only rule for this Swapper executable once. Windows
/// asks for elevation when the rule is installed; denying it leaves LAN closed.
/// Rules for other Swapper executables (an installed build next to a dev
/// build) are kept, so switching between them does not prompt every time;
/// rules whose executable no longer exists are removed.
pub fn allow_private_app() -> Result<(), String> {
    let executable = current_exe_for_powershell()?;
    let elevated = format!(
        "Get-NetFirewallRule -DisplayName '{FIREWALL_RULE}' -ErrorAction SilentlyContinue | ForEach-Object {{ $program=($_ | Get-NetFirewallApplicationFilter).Program; if (-not (Test-Path -LiteralPath $program)) {{ $_ | Remove-NetFirewallRule }} }}; New-NetFirewallRule -DisplayName '{FIREWALL_RULE}' -Program '{executable}' -Direction Inbound -Action Allow -Protocol TCP -Profile Private | Out-Null"
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        elevated.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>(),
    );
    let check = rule_present_check(&executable);
    let script = format!(
        "{check}; try {{ $p=Start-Process -FilePath 'powershell.exe' -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList @('-NoProfile','-NonInteractive','-EncodedCommand','{encoded}'); exit $p.ExitCode }} catch {{ exit 1 }}"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .output()
        .map_err(|error| format!("Could not configure Windows Firewall for LAN access: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err("LAN access needs a Windows Firewall rule for Private networks. Turn Remote Control off and on, then approve the Windows prompt.".into())
    }
}

unsafe fn wide_string(value: *const u16) -> String {
    if value.is_null() {
        return "Ethernet or Wi-Fi".into();
    }
    let mut length = 0;
    while *value.add(length) != 0 {
        length += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(value, length))
}
