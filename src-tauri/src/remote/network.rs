use std::net::Ipv4Addr;
use std::process::Command;

use base64::Engine;
use windows_sys::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
    GAA_FLAG_SKIP_MULTICAST, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

pub struct LanInterface {
    pub address: Ipv4Addr,
    pub name: String,
    pub index: u32,
    metric: u32,
}

/// Returns the active default-route Ethernet or Wi-Fi interface, excluding
/// tunnels, virtual adapters, loopback, link-local, and non-private addresses.
pub fn default_interface() -> Result<Option<LanInterface>, String> {
    let mut size = 0u32;
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
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
            let virtual_adapter = ["tailscale", "wsl", "docker", "hyper-v", "vmware", "virtual", "vpn", "wireguard", "tunnel", "tap"]
                .iter()
                .any(|marker| normalized_name.contains(marker));
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

/// Adds a Private-profile-only rule for Swapper once. Windows asks for
/// elevation when the rule is installed; denying it leaves LAN closed.
pub fn allow_private_app() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not locate Swapper for the firewall rule: {error}"))?;
    let executable = executable.to_string_lossy().replace('\'', "''");
    let elevated = format!(
        "Get-NetFirewallRule -DisplayName 'Swapper LAN Remote Control' -ErrorAction SilentlyContinue | Remove-NetFirewallRule; New-NetFirewallRule -DisplayName 'Swapper LAN Remote Control' -Program '{executable}' -Direction Inbound -Action Allow -Protocol TCP -Profile Private | Out-Null"
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        elevated.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>(),
    );
    let script = format!(
        "$existing=Get-NetFirewallRule -DisplayName 'Swapper LAN Remote Control' -ErrorAction SilentlyContinue; $programs=$existing | Get-NetFirewallApplicationFilter | ForEach-Object Program; if ($programs -contains '{executable}') {{ exit 0 }}; try {{ $p=Start-Process -FilePath 'powershell.exe' -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList @('-NoProfile','-NonInteractive','-EncodedCommand','{encoded}'); exit $p.ExitCode }} catch {{ exit 1 }}"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .output()
        .map_err(|error| format!("Could not configure Windows Firewall for LAN access: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err("Windows Firewall did not allow LAN access. Approve the Swapper firewall request and try again.".into())
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
