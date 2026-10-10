use std::net::Ipv4Addr;

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
    /// The adapter's `{GUID}` name, which the Network List Manager uses.
    pub adapter_id: String,
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
                                    adapter_id: ansi_string(current.AdapterName),
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
                if selected.is_none_or(|best| candidate < best) {
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

/// Whether Windows files the network behind this adapter as **Private**.
/// Asks the Network List Manager directly, so no process or window is started.
pub fn is_private_profile(adapter_id: &str) -> Result<bool, String> {
    let adapter_id = adapter_id
        .trim_matches(|c| c == '{' || c == '}')
        .to_ascii_uppercase();
    com::run(move || unsafe {
        use windows::Win32::Networking::NetworkListManager::{
            INetworkConnection, INetworkListManager, NetworkListManager,
            NLM_NETWORK_CATEGORY_PRIVATE,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
        let fail = |error: windows::core::Error| {
            format!("Could not check the Windows network profile ({error}).")
        };
        let manager: INetworkListManager =
            CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).map_err(fail)?;
        let connections = manager.GetNetworkConnections().map_err(fail)?;
        loop {
            let mut item: [Option<INetworkConnection>; 1] = [None];
            let mut fetched = 0u32;
            if connections.Next(&mut item, Some(&mut fetched)).is_err() || fetched == 0 {
                return Ok(false);
            }
            let Some(connection) = item[0].take() else {
                return Ok(false);
            };
            let Ok(id) = connection.GetAdapterId() else {
                continue;
            };
            if format!("{id:?}").to_ascii_uppercase() != adapter_id {
                continue;
            }
            let category = connection
                .GetNetwork()
                .and_then(|network| network.GetCategory())
                .map_err(fail)?;
            return Ok(category == NLM_NETWORK_CATEGORY_PRIVATE);
        }
    })
}

/// How long a Private/Public answer is reused for the same adapter. Asking
/// Windows costs ~100 ms of CPU, too much for the 30-second maintenance loop;
/// a new adapter or address is always asked immediately.
const PROFILE_REUSE: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// [`is_private_profile`], reusing a recent answer for the same adapter.
pub fn is_private_profile_cached(adapter_id: &str) -> Result<bool, String> {
    use std::sync::Mutex;
    use std::time::Instant;
    static LAST: Mutex<Option<(String, bool, Instant)>> = Mutex::new(None);
    if let Some((id, private, at)) = LAST.lock().ok().and_then(|last| last.clone()) {
        if id == adapter_id && at.elapsed() < PROFILE_REUSE {
            return Ok(private);
        }
    }
    let private = is_private_profile(adapter_id)?;
    if let Ok(mut last) = LAST.lock() {
        *last = Some((adapter_id.to_string(), private, Instant::now()));
    }
    Ok(private)
}

/// Command-line argument that makes an elevated Swapper add its firewall rule
/// and exit, so the Windows prompt names Swapper rather than a shell.
pub const ALLOW_FIREWALL_ARG: &str = "--allow-lan-firewall";

/// One Private-profile rule per Swapper executable, so a dev build and an
/// installed build do not replace each other's rule.
fn firewall_rule_name(executable: &str) -> String {
    format!("Swapper LAN Remote Control ({executable})")
}

fn current_executable() -> Result<String, String> {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| format!("Could not locate Swapper for the firewall rule: {error}"))
}

/// Whether Windows Firewall already allows this Swapper executable inbound on
/// Private networks. Reading rules needs no elevation and opens no window.
pub fn private_app_allowed() -> bool {
    let Ok(executable) = current_executable() else {
        return false;
    };
    com::run(move || unsafe {
        use windows::core::BSTR;
        use windows::Win32::NetworkManagement::WindowsFirewall::{
            INetFwPolicy2, NetFwPolicy2, NET_FW_ACTION_ALLOW, NET_FW_PROFILE2_PRIVATE,
            NET_FW_RULE_DIR_IN,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
        let Ok(policy) = CoCreateInstance::<_, INetFwPolicy2>(&NetFwPolicy2, None, CLSCTX_ALL)
        else {
            return false;
        };
        let Ok(rule) = policy
            .Rules()
            .and_then(|rules| rules.Item(&BSTR::from(firewall_rule_name(&executable))))
        else {
            return false;
        };
        rule.Enabled().is_ok_and(|on| on.as_bool())
            && rule.Direction().ok() == Some(NET_FW_RULE_DIR_IN)
            && rule.Action().ok() == Some(NET_FW_ACTION_ALLOW)
            && rule
                .Profiles()
                .is_ok_and(|profiles| profiles & NET_FW_PROFILE2_PRIVATE.0 != 0)
            && rule
                .ApplicationName()
                .is_ok_and(|name| name.to_string().eq_ignore_ascii_case(&executable))
    })
}

/// Adds the Private-profile rule for this executable. Only works elevated; it
/// runs in the `--allow-lan-firewall` child that [`request_private_app_rule`]
/// starts.
pub fn add_private_app_rule() -> Result<(), String> {
    let executable = current_executable()?;
    com::run(move || unsafe {
        use windows::core::BSTR;
        use windows::Win32::Foundation::VARIANT_TRUE;
        use windows::Win32::NetworkManagement::WindowsFirewall::{
            INetFwPolicy2, INetFwRule, NetFwPolicy2, NetFwRule, NET_FW_ACTION_ALLOW,
            NET_FW_IP_PROTOCOL_TCP, NET_FW_PROFILE2_PRIVATE, NET_FW_RULE_DIR_IN,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
        let fail = |error: windows::core::Error| {
            format!("Could not add the Windows Firewall rule ({error}).")
        };
        let policy: INetFwPolicy2 =
            CoCreateInstance(&NetFwPolicy2, None, CLSCTX_ALL).map_err(fail)?;
        let rules = policy.Rules().map_err(fail)?;
        let name = BSTR::from(firewall_rule_name(&executable));
        let _ = rules.Remove(&name);
        let rule: INetFwRule = CoCreateInstance(&NetFwRule, None, CLSCTX_ALL).map_err(fail)?;
        rule.SetName(&name).map_err(fail)?;
        rule.SetDescription(&BSTR::from(
            "Lets phones on your Private network use Swapper Remote Control.",
        ))
        .map_err(fail)?;
        rule.SetGrouping(&BSTR::from("Swapper")).map_err(fail)?;
        rule.SetApplicationName(&BSTR::from(executable.as_str()))
            .map_err(fail)?;
        rule.SetProtocol(NET_FW_IP_PROTOCOL_TCP.0).map_err(fail)?;
        rule.SetDirection(NET_FW_RULE_DIR_IN).map_err(fail)?;
        rule.SetAction(NET_FW_ACTION_ALLOW).map_err(fail)?;
        rule.SetProfiles(NET_FW_PROFILE2_PRIVATE.0).map_err(fail)?;
        rule.SetEnabled(VARIANT_TRUE).map_err(fail)?;
        rules.Add(&rule).map_err(fail)
    })
}

/// Why an elevated launch did not complete. Carries no wording; each caller
/// names its own action.
pub enum ElevateError {
    /// The user declined the Windows elevation prompt.
    Cancelled,
    /// Windows could not start the elevated process at all.
    Failed,
}

/// Runs `exe args` elevated (a `runas` ShellExecuteExW), blocking the calling
/// thread until the child exits or `wait_ms` elapses, and returns its exit code.
/// A timeout still returns the code observed so far (the still-running process
/// reports `STILL_ACTIVE`), so callers treat any non-zero code as a failure.
///
/// Both the firewall rule and the LTK Manager install need admin and use this,
/// so the elevation prompt shows the named executable and no console window.
pub fn run_elevated(exe: &str, args: &str, wait_ms: u32) -> Result<u32, ElevateError> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let executable = wide(exe);
    let verb = wide("runas");
    let arguments = wide(args);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = executable.as_ptr();
    info.lpParameters = arguments.as_ptr();
    info.nShow = SW_HIDE;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(if unsafe { GetLastError() } == ERROR_CANCELLED {
            ElevateError::Cancelled
        } else {
            ElevateError::Failed
        });
    }
    let mut code = 1u32;
    unsafe {
        WaitForSingleObject(info.hProcess, wait_ms);
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
    }
    Ok(code)
}

/// Asks Windows to run Swapper elevated just to add its firewall rule. The
/// prompt shows Swapper's name and icon. Blocks until that child exits.
pub fn request_private_app_rule() -> Result<(), String> {
    match run_elevated(&current_executable()?, ALLOW_FIREWALL_ARG, 60_000) {
        Ok(0) => Ok(()),
        Ok(_) => Err("Could not add the Windows Firewall rule.".into()),
        Err(ElevateError::Cancelled) => {
            Err("Windows Firewall permission was not given, so LAN access stays off.".into())
        }
        Err(ElevateError::Failed) => Err("Could not ask Windows for firewall permission.".into()),
    }
}

/// Runs COM work on a short-lived thread of its own, so it never depends on
/// (or changes) how the calling thread initialized COM.
mod com {
    pub fn run<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::spawn(move || unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
            let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let result = work();
            if initialized {
                CoUninitialize();
            }
            result
        })
        .join()
        .expect("COM worker thread panicked")
    }
}

unsafe fn ansi_string(value: *const u8) -> String {
    if value.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(value.cast()).to_string_lossy().into_owned()
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
