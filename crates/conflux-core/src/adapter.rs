use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkAdapter {
    pub id: String,
    pub ip: IpAddr,
    pub is_ipv4: bool,
    pub is_loopback: bool,
    pub enabled: bool,
}

/// TCP connect timeout for every client built by Conflux.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default stall timeout: an attempt fails if no bytes arrive for this long.
pub const DEFAULT_STALL_TIMEOUT: Duration = Duration::from_secs(20);

/// Case-insensitive name fragments that indicate virtual / tunnel / container adapters.
/// Such adapters are listed but disabled by default: they don't add real bandwidth.
const VIRTUAL_NAME_HINTS: &[&str] = &[
    "vethernet",
    "docker",
    "veth",
    "br-",
    "virbr",
    "vmnet",
    "virtualbox",
    "hyper-v",
    "wsl",
    "tailscale",
    "zerotier",
    "tun",
    "tap",
    "utun",
    "loopback",
];

fn is_loopback_interface(name: &str, ip: &IpAddr) -> bool {
    ip.is_loopback()
        || name == "lo"
        || name == "lo0"
        || name.to_ascii_lowercase().contains("loopback")
}

fn looks_virtual(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    VIRTUAL_NAME_HINTS.iter().any(|hint| lower.contains(hint))
}

fn is_link_local(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_link_local(),
        IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// Classifies one (interface name, address) pair. Pure function so it can be unit-tested
/// without depending on the machine's real interfaces.
pub(crate) fn classify_adapter(name: &str, ip: IpAddr) -> NetworkAdapter {
    let is_loopback = is_loopback_interface(name, &ip);
    let is_ipv4 = ip.is_ipv4();
    let enabled = is_ipv4 && !is_loopback && !is_link_local(&ip) && !looks_virtual(name);
    NetworkAdapter {
        id: format!("{}:{}", name, ip),
        ip,
        is_ipv4,
        is_loopback,
        enabled,
    }
}

/// Enumerates all local network adapter IP addresses available on the system.
///
/// Every address is returned; `enabled` is `true` only for non-loopback, non-link-local
/// IPv4 addresses on interfaces that don't look virtual (Docker, WSL, Hyper-V, VPN tunnels...).
pub fn discover_adapters() -> Result<Vec<NetworkAdapter>> {
    let mut adapters = Vec::new();

    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in interfaces {
            adapters.push(classify_adapter(&name, ip));
        }
    }

    Ok(adapters)
}

/// Builds an HTTP client whose sockets are bound to the local address `local_ip`
/// (or left unbound for default OS routing when `None`).
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
/// or per-interface routes/metrics on Windows.
pub fn build_bound_http_client(
    local_ip: Option<IpAddr>,
    stall_timeout: Duration,
) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(stall_timeout);

    if let Some(ip) = local_ip {
        builder = builder.local_address(Some(ip));
    }

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

    #[test]
    fn test_build_client_unbound() {
        assert!(build_bound_http_client(None, DEFAULT_STALL_TIMEOUT).is_ok());
        assert!(build_bound_http_client(Some(v4(127, 0, 0, 1)), DEFAULT_STALL_TIMEOUT).is_ok());
    }
}
