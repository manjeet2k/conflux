use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkAdapter {
    pub id: String,
    /// OS interface name (e.g. "Wi-Fi", "wlan0"). Empty when unknown.
    #[serde(default)]
    pub name: String,
    pub ip: IpAddr,
    pub is_ipv4: bool,
    pub is_loopback: bool,
    pub enabled: bool,
}

/// TCP connect timeout for every client built by Conflux.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default stall timeout: an attempt fails if no bytes arrive for this long.
pub const DEFAULT_STALL_TIMEOUT: Duration = Duration::from_secs(20);

/// Case-insensitive fragments of an interface name or (Windows) adapter description that
/// indicate virtual / tunnel / container adapters. Such adapters are listed but disabled by
/// default: they don't add real bandwidth.
const VIRTUAL_NAME_HINTS: &[&str] = &[
    "vethernet",
    "docker",
    "veth",
    "virbr",
    "vmnet",
    "virtualbox",
    "hyper-v",
    "wsl",
    "tailscale",
    "zerotier",
    "wireguard",
    "openvpn",
    "tun",
    "tap",
    "utun",
    "loopback",
];

/// Short Linux-style name prefixes (WireGuard `wg0`, bridges `br0` / `br-3f2a`). Matched only
/// at the start of the name and only when followed by a digit, `-`, `_` or nothing, so that
/// e.g. "Broadcom" or "brcmfmac0" are not mistaken for bridges.
const VIRTUAL_NAME_PREFIXES: &[&str] = &["wg", "br"];

/// Windows `IfOperStatusUp` (`IF_OPER_STATUS`).
const WIN_IF_OPER_STATUS_UP: i32 = 1;
/// Windows `IpDadStatePreferred` (`NL_DAD_STATE`): duplicate address detection succeeded.
const WIN_IP_DAD_STATE_PREFERRED: i32 = 4;
/// Windows `IF_TYPE_PROP_VIRTUAL` (Wintun / WireGuard and other software adapters).
const WIN_IF_TYPE_PROP_VIRTUAL: u32 = 53;
/// Windows `IF_TYPE_TUNNEL` (encapsulation tunnels: Teredo, 6to4, IP-HTTPS...).
const WIN_IF_TYPE_TUNNEL: u32 = 131;

/// One operational address as reported by the OS, before classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InterfaceAddress {
    /// Stable OS interface name ("Wi-Fi", "wlan0"); becomes [`NetworkAdapter::name`].
    pub name: String,
    /// Driver description ("TAP-Windows Adapter V9"); empty where the OS has none.
    pub description: String,
    /// The OS reports a virtual or tunnel interface type.
    pub virtual_if_type: bool,
    pub ip: IpAddr,
}

fn is_loopback_interface(name: &str, ip: &IpAddr) -> bool {
    ip.is_loopback()
        || name == "lo"
        || name == "lo0"
        || name.to_ascii_lowercase().contains("loopback")
}

fn contains_virtual_hint(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    VIRTUAL_NAME_HINTS.iter().any(|hint| lower.contains(hint))
}

fn has_virtual_prefix(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    VIRTUAL_NAME_PREFIXES
        .iter()
        .any(|prefix| match lower.strip_prefix(prefix) {
            Some(rest) => match rest.chars().next() {
                None => true,
                Some(c) => c.is_ascii_digit() || c == '-' || c == '_',
            },
            None => false,
        })
}

/// `true` if the interface name looks like a virtual / tunnel / container adapter.
pub fn looks_virtual(name: &str) -> bool {
    contains_virtual_hint(name) || has_virtual_prefix(name)
}

/// `true` for Windows interface types that never carry real uplink bandwidth of their own.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn is_virtual_if_type(if_type: u32) -> bool {
    if_type == WIN_IF_TYPE_PROP_VIRTUAL || if_type == WIN_IF_TYPE_TUNNEL
}

/// `true` if a Windows unicast address is usable right now: its adapter is operationally up
/// and the address passed duplicate address detection (not Tentative/Duplicate/Deprecated).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn is_windows_address_operational(oper_status: i32, dad_state: i32) -> bool {
    oper_status == WIN_IF_OPER_STATUS_UP && dad_state == WIN_IP_DAD_STATE_PREFERRED
}

/// `true` if a Linux interface is administratively up (`IFF_UP`) and has carrier
/// (`IFF_RUNNING`). An unplugged cable with a static IP keeps its address but loses RUNNING.
#[cfg(target_os = "linux")]
pub(crate) fn is_linux_interface_operational(flags: u32) -> bool {
    let required = (libc::IFF_UP | libc::IFF_RUNNING) as u32;
    flags & required == required
}

/// `true` for IPv4 `169.254.0.0/16` and IPv6 `fe80::/10` addresses.
pub fn is_link_local(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_link_local(),
        IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// Classifies one OS-reported address. Pure function so it can be unit-tested without
/// depending on the machine's real interfaces.
pub(crate) fn classify_interface(iface: &InterfaceAddress) -> NetworkAdapter {
    let name = iface.name.as_str();
    let ip = iface.ip;
    let is_loopback = is_loopback_interface(name, &ip);
    let is_ipv4 = ip.is_ipv4();
    let is_virtual =
        iface.virtual_if_type || looks_virtual(name) || contains_virtual_hint(&iface.description);
    let enabled = is_ipv4 && !is_loopback && !is_link_local(&ip) && !is_virtual;
    NetworkAdapter {
        id: format!("{}:{}", name, ip),
        name: name.to_string(),
        ip,
        is_ipv4,
        is_loopback,
        enabled,
    }
}

/// Classifies one (interface name, address) pair with no description or interface type.
#[cfg(test)]
pub(crate) fn classify_adapter(name: &str, ip: IpAddr) -> NetworkAdapter {
    classify_interface(&InterfaceAddress {
        name: name.to_string(),
        description: String::new(),
        virtual_if_type: false,
        ip,
    })
}

/// Enumerates the local network adapter addresses that are operational right now.
///
/// Addresses on interfaces that are down or have no carrier, and (on Windows) addresses still
/// in duplicate address detection, are omitted entirely, so the watcher reports them as
/// removed. `enabled` is `true` only for non-loopback, non-link-local IPv4 addresses on
/// interfaces that don't look virtual (Docker, WSL, Hyper-V, VPN tunnels, bridges...).
pub fn discover_adapters() -> Result<Vec<NetworkAdapter>> {
    // A failed enumeration must be an error, not an empty list: the watcher would otherwise
    // report every adapter as removed and retire all workers mid-download.
    let interfaces = sys::list_operational_addresses()?;
    Ok(interfaces.iter().map(classify_interface).collect())
}

#[cfg(target_os = "linux")]
mod sys {
    use super::{is_linux_interface_operational, InterfaceAddress};
    use anyhow::{bail, Result};
    use std::ffi::CStr;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    /// Lists IPv4/IPv6 addresses of interfaces that are UP and RUNNING (`getifaddrs`).
    pub(super) fn list_operational_addresses() -> Result<Vec<InterfaceAddress>> {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        // SAFETY: `head` is a valid out-pointer; on success it must be freed with freeifaddrs.
        if unsafe { libc::getifaddrs(&mut head) } != 0 {
            bail!(
                "Failed to enumerate network interfaces: getifaddrs: {}",
                std::io::Error::last_os_error()
            );
        }

        let mut out = Vec::new();
        let mut cur = head;
        while !cur.is_null() {
            // SAFETY: `cur` is a node of the list returned by getifaddrs, not yet freed.
            let ifa = unsafe { &*cur };
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null()
                || ifa.ifa_name.is_null()
                || !is_linux_interface_operational(ifa.ifa_flags)
            {
                continue;
            }
            // SAFETY: `ifa_addr` is non-null and points at a sockaddr whose concrete type is
            // given by `sa_family`; `ifa_name` is a NUL-terminated C string.
            let ip = unsafe {
                match i32::from((*ifa.ifa_addr).sa_family) {
                    libc::AF_INET => {
                        let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                        IpAddr::V4(Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)))
                    }
                    libc::AF_INET6 => {
                        let sin6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                        IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.s6_addr))
                    }
                    _ => continue,
                }
            };
            let name = unsafe { CStr::from_ptr(ifa.ifa_name) }
                .to_string_lossy()
                .into_owned();
            out.push(InterfaceAddress {
                name,
                description: String::new(),
                virtual_if_type: false,
                ip,
            });
        }

        // SAFETY: `head` came from a successful getifaddrs and is freed exactly once.
        unsafe { libc::freeifaddrs(head) };
        Ok(out)
    }
}

#[cfg(windows)]
mod sys {
    use super::{
        is_virtual_if_type, is_windows_address_operational, InterfaceAddress,
        WIN_IF_OPER_STATUS_UP, WIN_IF_TYPE_PROP_VIRTUAL, WIN_IF_TYPE_TUNNEL,
        WIN_IP_DAD_STATE_PREFERRED,
    };
    use anyhow::{bail, Result};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IF_TYPE_PROP_VIRTUAL, IF_TYPE_TUNNEL, IP_ADAPTER_ADDRESSES_LH,
        IP_ADAPTER_UNICAST_ADDRESS_LH,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows_sys::Win32::Networking::WinSock::{
        IpDadStatePreferred, AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6,
    };

    // The pure classifiers use plain integers so they are testable on any host.
    const _: () = assert!(IfOperStatusUp == WIN_IF_OPER_STATUS_UP);
    const _: () = assert!(IpDadStatePreferred == WIN_IP_DAD_STATE_PREFERRED);
    const _: () = assert!(IF_TYPE_PROP_VIRTUAL == WIN_IF_TYPE_PROP_VIRTUAL);
    const _: () = assert!(IF_TYPE_TUNNEL == WIN_IF_TYPE_TUNNEL);

    /// Reads a NUL-terminated UTF-16 string; null yields an empty string.
    ///
    /// # Safety
    /// `ptr` must be null or point at a NUL-terminated UTF-16 string.
    unsafe fn wide_to_string(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
    }

    /// # Safety
    /// `addr` must be null or point at a valid socket address of its `sa_family`.
    unsafe fn sockaddr_to_ip(addr: *const SOCKADDR) -> Option<IpAddr> {
        if addr.is_null() {
            return None;
        }
        match (*addr).sa_family {
            AF_INET => {
                let sin = &*(addr as *const SOCKADDR_IN);
                Some(IpAddr::V4(Ipv4Addr::from(
                    sin.sin_addr.S_un.S_addr.to_ne_bytes(),
                )))
            }
            AF_INET6 => {
                let sin6 = &*(addr as *const SOCKADDR_IN6);
                Some(IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.u.Byte)))
            }
            _ => None,
        }
    }

    /// Lists unicast addresses of adapters that are up, with DAD complete
    /// (`GetAdaptersAddresses`). Names are the adapter friendly names ("Wi-Fi", "Ethernet").
    pub(super) fn list_operational_addresses() -> Result<Vec<InterfaceAddress>> {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        // 15 KB is Microsoft's recommended starting size; u64 storage keeps the
        // IP_ADAPTER_ADDRESSES_LH structs correctly aligned.
        let mut size: u32 = 15_000;
        let mut buf: Vec<u64>;
        let mut attempts = 0;
        loop {
            attempts += 1;
            buf = vec![0u64; (size as usize).div_ceil(8)];
            // SAFETY: `buf` provides `size` writable, 8-byte-aligned bytes.
            let ret = unsafe {
                GetAdaptersAddresses(
                    u32::from(AF_UNSPEC),
                    flags,
                    std::ptr::null(),
                    buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                    &mut size,
                )
            };
            match ret {
                ERROR_SUCCESS => break,
                ERROR_NO_DATA => return Ok(Vec::new()),
                // The adapter list grew between calls; retry with the size the OS asked for.
                ERROR_BUFFER_OVERFLOW if attempts < 4 => continue,
                err => bail!(
                    "Failed to enumerate network interfaces: GetAdaptersAddresses error {}",
                    err
                ),
            }
        }

        let mut out = Vec::new();
        let mut adapter = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !adapter.is_null() {
            // SAFETY: `adapter` is a node of the linked list GetAdaptersAddresses wrote into
            // `buf`, which outlives this loop; all string/address pointers point into it.
            let a = unsafe { &*adapter };
            adapter = a.Next;
            let name = unsafe { wide_to_string(a.FriendlyName) };
            let description = unsafe { wide_to_string(a.Description) };
            let virtual_if_type = is_virtual_if_type(a.IfType);

            let mut unicast = a.FirstUnicastAddress as *const IP_ADAPTER_UNICAST_ADDRESS_LH;
            while !unicast.is_null() {
                // SAFETY: as above, a node of the unicast list inside `buf`.
                let u = unsafe { &*unicast };
                unicast = u.Next;
                if !is_windows_address_operational(a.OperStatus, u.DadState) {
                    continue;
                }
                if let Some(ip) = unsafe { sockaddr_to_ip(u.Address.lpSockaddr) } {
                    out.push(InterfaceAddress {
                        name: name.clone(),
                        description: description.clone(),
                        virtual_if_type,
                        ip,
                    });
                }
            }
        }
        Ok(out)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod sys {
    use super::InterfaceAddress;
    use anyhow::Result;

    /// Fallback without link-state filtering for platforms lacking a native implementation.
    pub(super) fn list_operational_addresses() -> Result<Vec<InterfaceAddress>> {
        let interfaces = local_ip_address::list_afinet_netifas()
            .map_err(|e| anyhow::anyhow!("Failed to enumerate network interfaces: {e}"))?;
        Ok(interfaces
            .into_iter()
            .map(|(name, ip)| InterfaceAddress {
                name,
                description: String::new(),
                virtual_if_type: false,
                ip,
            })
            .collect())
    }
}

/// Builds an HTTP client whose sockets are bound to the local address `local_ip`
/// (or left unbound for default OS routing when `None`). On Linux, a non-empty `interface`
/// also pins egress to that device (`SO_BINDTODEVICE`); elsewhere it is ignored.
///
/// Timeouts: a connect timeout of [`CONNECT_TIMEOUT`] and a per-read stall timeout of
/// `stall_timeout` (no bytes received for that long fails the request). There is
/// deliberately no total request timeout, so large chunks on slow links are not killed.
///
/// Limitation: binding the *source address* does not by itself pin the *egress interface*
/// on weak-host-model stacks (Linux by default, and Windows for sends unless the route
/// table cooperates). The OS still picks the outgoing interface from the routing table;
/// with a single default route, packets may leave via another adapter carrying this
/// adapter's source IP. True per-interface egress needs policy routing (Linux `ip rule`)
/// or per-interface routes/metrics on Windows. `interface` closes this gap on Linux.
pub fn build_bound_http_client(
    local_ip: Option<IpAddr>,
    interface: Option<&str>,
    stall_timeout: Duration,
) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(stall_timeout);

    if let Some(ip) = local_ip {
        builder = builder.local_address(Some(ip));
    }
    #[cfg(target_os = "linux")]
    if let Some(name) = interface.filter(|n| !n.is_empty()) {
        builder = builder.interface(name);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = interface;

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn test_discover_adapters_invariants() {
        // Environment-independent: only check invariants of whatever is discovered.
        let adapters = discover_adapters().expect("discovery must not error");
        for a in &adapters {
            assert_eq!(a.is_ipv4, a.ip.is_ipv4(), "{:?}", a);
            if a.enabled {
                assert!(a.is_ipv4 && !a.is_loopback, "{:?}", a);
                assert!(!is_link_local(&a.ip), "{:?}", a);
            }
            if a.ip.is_loopback() {
                assert!(a.is_loopback && !a.enabled, "{:?}", a);
            }
        }
    }

    #[test]
    fn test_classify_physical_adapters_enabled() {
        assert!(classify_adapter("eth0", v4(192, 168, 1, 10)).enabled);
        assert!(classify_adapter("Wi-Fi", v4(10, 0, 0, 5)).enabled);
        assert!(classify_adapter("Ethernet 2", v4(172, 20, 1, 2)).enabled);
        assert!(classify_adapter("wlan0", v4(192, 168, 43, 100)).enabled);
        let a = classify_adapter("eth0", v4(192, 168, 1, 10));
        assert_eq!(a.id, "eth0:192.168.1.10");
        assert!(a.is_ipv4 && !a.is_loopback);
    }

    #[test]
    fn test_classify_loopback_link_local_ipv6_disabled() {
        let lo = classify_adapter("lo", v4(127, 0, 0, 1));
        assert!(lo.is_loopback && !lo.enabled);
        let win_lo = classify_adapter("Loopback Pseudo-Interface 1", v4(127, 0, 0, 1));
        assert!(win_lo.is_loopback && !win_lo.enabled);

        let apipa = classify_adapter("Ethernet", v4(169, 254, 12, 34));
        assert!(!apipa.enabled && !apipa.is_loopback);

        let v6 = classify_adapter("eth0", IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert!(!v6.enabled && !v6.is_ipv4);
        let v6_ll = classify_adapter("eth0", "fe80::1".parse().unwrap());
        assert!(!v6_ll.enabled);
    }

    #[test]
    fn test_classify_virtual_adapters_disabled_but_listed() {
        for name in [
            "vEthernet (WSL)",
            "docker0",
            "veth12ab",
            "br-3f2a1b",
            "virbr0",
            "VMware Network Adapter VMnet8",
            "VirtualBox Host-Only Network",
            "Hyper-V Virtual Ethernet Adapter",
            "Tailscale",
            "ZeroTier One",
            "tun0",
            "tap0",
            "utun3",
        ] {
            let a = classify_adapter(name, v4(10, 1, 2, 3));
            assert!(!a.enabled, "{} should be disabled by default", name);
            assert!(a.is_ipv4 && !a.is_loopback);
        }
    }

    fn iface(name: &str, description: &str, virtual_if_type: bool) -> InterfaceAddress {
        InterfaceAddress {
            name: name.to_string(),
            description: description.to_string(),
            virtual_if_type,
            ip: v4(10, 1, 2, 3),
        }
    }

    #[test]
    fn test_classify_vpn_and_bridge_names_disabled() {
        for name in [
            "wg0",
            "wg-home",
            "WG1",
            "br0",
            "br-3f2a1b",
            "WireGuard Tunnel",
            "OpenVPN Wintun",
            "TAP-Windows Adapter V9",
        ] {
            let a = classify_adapter(name, v4(10, 1, 2, 3));
            assert!(!a.enabled, "{} should be disabled by default", name);
            assert!(looks_virtual(name), "{}", name);
        }
    }

    #[test]
    fn test_classify_uses_windows_description_and_if_type() {
        // OpenVPN TAP / WireGuard on Windows often have a generic friendly name.
        for (name, description, virtual_type) in [
            ("Local Area Connection 2", "TAP-Windows Adapter V9", false),
            ("Local Area Connection 3", "TAP-Win32 Adapter OAS", false),
            ("home", "WireGuard Tunnel", false),
            ("OpenVPN Connect", "OpenVPN Data Channel Offload", false),
            ("Ethernet 3", "Some Vendor Adapter", true),
        ] {
            let a = classify_interface(&iface(name, description, virtual_type));
            assert!(!a.enabled, "{} / {} should be disabled", name, description);
            assert_eq!(a.name, name, "name must stay the OS friendly name");
            assert_eq!(a.id, format!("{}:10.1.2.3", name));
        }
        let wifi = classify_interface(&iface("Wi-Fi", "Intel(R) Wi-Fi 6 AX201 160MHz", false));
        assert!(wifi.enabled);
    }

    #[test]
    fn test_virtual_detection_no_false_positives() {
        for name in [
            "eth0",
            "enp3s0",
            "wlan0",
            "wlp2s0",
            "Wi-Fi",
            "Ethernet",
            "Ethernet 2",
            "usb0",
            "Broadcom",
            "brcmfmac0",
            "wgx",
            "Cellular",
        ] {
            assert!(!looks_virtual(name), "{} must not look virtual", name);
            assert!(
                classify_adapter(name, v4(192, 168, 1, 7)).enabled,
                "{}",
                name
            );
        }
        for (name, description) in [
            ("Wi-Fi", "Broadcom 802.11ac Network Adapter"),
            ("Ethernet", "Intel(R) Ethernet Connection (7) I219-V"),
            ("Ethernet 4", "Remote NDIS based Internet Sharing Device"),
            ("Ethernet 5", "Realtek USB GbE Family Controller"),
        ] {
            let a = classify_interface(&iface(name, description, false));
            assert!(a.enabled, "{} / {} must stay enabled", name, description);
        }
    }

    #[test]
    fn test_windows_virtual_if_types() {
        assert!(is_virtual_if_type(53)); // IF_TYPE_PROP_VIRTUAL
        assert!(is_virtual_if_type(131)); // IF_TYPE_TUNNEL
        assert!(!is_virtual_if_type(6)); // Ethernet
        assert!(!is_virtual_if_type(71)); // 802.11
        assert!(!is_virtual_if_type(243)); // WWAN
    }

    #[test]
    fn test_windows_operational_state() {
        const UP: i32 = 1;
        const DOWN: i32 = 2;
        const DORMANT: i32 = 5;
        const TENTATIVE: i32 = 1;
        const DUPLICATE: i32 = 2;
        const DEPRECATED: i32 = 3;
        const PREFERRED: i32 = 4;
        assert!(is_windows_address_operational(UP, PREFERRED));
        assert!(!is_windows_address_operational(DOWN, PREFERRED));
        assert!(!is_windows_address_operational(DORMANT, PREFERRED));
        assert!(!is_windows_address_operational(UP, TENTATIVE));
        assert!(!is_windows_address_operational(UP, DUPLICATE));
        assert!(!is_windows_address_operational(UP, DEPRECATED));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_operational_flags() {
        let up = libc::IFF_UP as u32;
        let running = libc::IFF_RUNNING as u32;
        let multicast = libc::IFF_MULTICAST as u32;
        assert!(is_linux_interface_operational(up | running));
        assert!(is_linux_interface_operational(up | running | multicast));
        // Cable unplugged with a static IP: administratively up, no carrier.
        assert!(!is_linux_interface_operational(up | multicast));
        assert!(!is_linux_interface_operational(running));
        assert!(!is_linux_interface_operational(0));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_discovery_only_reports_operational_interfaces() {
        // Cross-check against the kernel's view: every reported name must be UP+RUNNING.
        for a in discover_adapters().unwrap() {
            let base = a.name.split(':').next().unwrap();
            let flags_path = format!("/sys/class/net/{}/flags", base);
            let carrier_path = format!("/sys/class/net/{}/carrier", base);
            if let Ok(flags) = std::fs::read_to_string(&flags_path) {
                let flags = u32::from_str_radix(flags.trim().trim_start_matches("0x"), 16).unwrap();
                assert!(flags & libc::IFF_UP as u32 != 0, "{:?} is down", a);
            }
            if let Ok(carrier) = std::fs::read_to_string(&carrier_path) {
                assert_eq!(carrier.trim(), "1", "{:?} has no carrier", a);
            }
        }
    }

    #[test]
    fn test_build_client_unbound() {
        assert!(build_bound_http_client(None, None, DEFAULT_STALL_TIMEOUT).is_ok());
        assert!(
            build_bound_http_client(Some(v4(127, 0, 0, 1)), None, DEFAULT_STALL_TIMEOUT).is_ok()
        );
    }
}
