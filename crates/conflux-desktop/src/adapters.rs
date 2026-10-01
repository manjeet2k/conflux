//! Adapter presentation for the UI: display name, usability and a coarse media kind.

use crate::state::AdapterStat;
use conflux_core::{is_link_local, looks_virtual, AdapterProgress, NetworkAdapter};
use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterKind {
    Ethernet,
    Wifi,
    Cellular,
    Virtual,
    Loopback,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdapterInfo {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub is_ipv4: bool,
    pub is_loopback: bool,
    /// Discovery default: selected for new downloads.
    pub enabled: bool,
    /// The engine can bind to it (IPv4, not loopback, not link-local).
    pub usable: bool,
    pub kind: AdapterKind,
}

/// Interface name part of an adapter id (`"<name>:<ip>"`; the ip may itself contain ':').
pub fn adapter_name(adapter: &NetworkAdapter) -> String {
    adapter
        .id
        .strip_suffix(&format!(":{}", adapter.ip))
        .unwrap_or(&adapter.id)
        .to_string()
}

pub fn is_usable(adapter: &NetworkAdapter) -> bool {
    adapter.is_ipv4 && adapter.ip.is_ipv4() && !adapter.is_loopback && !is_link_local(&adapter.ip)
}

/// Best-effort media kind from the interface name (Windows friendly names and Linux names).
pub fn classify_kind(name: &str, is_loopback: bool) -> AdapterKind {
    let lower = name.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if is_loopback {
        return AdapterKind::Loopback;
    }
    // Windows Wi-Fi Direct / hosted-network pseudo adapters: "Local Area Connection* 10".
    if looks_virtual(name) || lower.contains("local area connection*") {
        return AdapterKind::Virtual;
    }
    if has(&["wi-fi", "wifi", "wlan", "wireless", "802.11"]) || lower.starts_with("wl") {
        return AdapterKind::Wifi;
    }
    // Checked before Ethernet: tethering adapters often have "Ethernet" in their name
    // ("Apple Mobile Device Ethernet", "Remote NDIS based Internet Sharing Device").
    if has(&[
        "cellular", "mobile", "wwan", "lte", "5g", "4g", "ndis", "rmnet", "usb",
    ]) {
        return AdapterKind::Cellular;
    }
    if has(&["ethernet", "local area connection"])
        || lower.starts_with("eth")
        || lower.starts_with("en")
    {
        return AdapterKind::Ethernet;
    }
    AdapterKind::Other
}

pub fn apply_overrides(adapter: &mut NetworkAdapter, overrides: &HashMap<String, bool>) {
    if !is_usable(adapter) {
        adapter.enabled = false;
        return;
    }
    let name = adapter_name(adapter);
    if let Some(&enabled) = overrides.get(&adapter.id).or_else(|| overrides.get(&name)) {
        adapter.enabled = enabled;
    }
}

pub fn to_info(adapter: NetworkAdapter) -> AdapterInfo {
    let name = adapter_name(&adapter);
    AdapterInfo {
        kind: classify_kind(&name, adapter.is_loopback),
        usable: is_usable(&adapter),
        name,
        ip: adapter.ip.to_string(),
        is_ipv4: adapter.is_ipv4,
        is_loopback: adapter.is_loopback,
        enabled: adapter.enabled,
        id: adapter.id,
    }
}

/// Bound IP -> (adapter id, interface name) for the adapters of one run.
pub fn adapter_names(adapters: &[NetworkAdapter]) -> HashMap<IpAddr, (String, String)> {
    adapters
        .iter()
        .filter(|a| a.enabled)
        .map(|a| (a.ip, (a.id.clone(), adapter_name(a))))
        .collect()
}

pub fn adapter_stat(p: &AdapterProgress, names: &HashMap<IpAddr, (String, String)>) -> AdapterStat {
    let (adapter_id, name) = match p.ip.and_then(|ip| names.get(&ip)) {
        Some((id, name)) => (Some(id.clone()), name.clone()),
        None if p.ip.is_none() => (None, "Default route".to_string()),
        None => (None, p.label.clone()),
    };
    AdapterStat {
        adapter_id,
        name,
        ip: p.ip.map(|ip| ip.to_string()),
        downloaded_bytes: p.downloaded_bytes,
        speed_bytes_sec: p.speed_bytes_sec,
        active_connections: p.active_connections,
        dropped: p.dropped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, ip: &str, loopback: bool) -> NetworkAdapter {
        let ip: std::net::IpAddr = ip.parse().unwrap();
        NetworkAdapter {
            id: format!("{name}:{ip}"),
            ip,
            is_ipv4: ip.is_ipv4(),
            is_loopback: loopback,
            enabled: true,
        }
    }

    #[test]
    fn kinds_from_names() {
        use AdapterKind::*;
        let cases = [
            ("Ethernet", Ethernet),
            ("Ethernet 2", Ethernet),
            ("eth0", Ethernet),
            ("enp3s0", Ethernet),
            ("Wi-Fi", Wifi),
            ("Wi-Fi 5G", Wifi),
            ("wlan0", Wifi),
            ("wlp2s0", Wifi),
            ("Cellular", Cellular),
            ("Apple Mobile Device Ethernet", Cellular),
            ("Remote NDIS based Internet Sharing Device", Cellular),
            ("wwan0", Cellular),
            ("usb0", Cellular),
            ("vEthernet (WSL)", Virtual),
            ("docker0", Virtual),
            ("Local Area Connection* 10", Virtual),
            ("Tailscale", Virtual),
            ("Bluetooth Network Connection", Other),
        ];
        for (name, kind) in cases {
            assert_eq!(classify_kind(name, false), kind, "{name}");
        }
        assert_eq!(classify_kind("lo", true), Loopback);
    }

    #[test]
    fn name_and_usability() {
        let v6 = adapter("Ethernet", "fe80::4022:7688:97e1:1", false);
        assert_eq!(adapter_name(&v6), "Ethernet");
        assert!(!is_usable(&v6));
        let v4 = adapter("Wi-Fi", "192.168.1.5", false);
        assert_eq!(adapter_name(&v4), "Wi-Fi");
        assert!(is_usable(&v4));
        assert!(!is_usable(&adapter("Ethernet", "169.254.1.1", false)));
        assert!(!is_usable(&adapter("lo", "127.0.0.1", true)));
        assert!(!is_usable(&adapter("Ethernet", "2401:4900::1", false)));

        let info = to_info(v4);
        assert_eq!(info.kind, AdapterKind::Wifi);
        assert_eq!(serde_json::to_value(&info).unwrap()["kind"], "wifi");
    }

    #[test]
    fn adapter_stats_map_back_to_adapter_ids() {
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        let adapters = vec![NetworkAdapter {
            id: format!("Wi-Fi:{ip}"),
            ip,
            is_ipv4: true,
            is_loopback: false,
            enabled: true,
        }];
        let names = adapter_names(&adapters);
        let progress = |ip: Option<IpAddr>, label: &str| AdapterProgress {
            label: label.into(),
            ip,
            downloaded_bytes: 10,
            speed_bytes_sec: 2.0,
            active_connections: 1,
            dropped: false,
        };

        let bound = adapter_stat(&progress(Some(ip), "192.168.1.5"), &names);
        assert_eq!(bound.adapter_id.as_deref(), Some("Wi-Fi:192.168.1.5"));
        assert_eq!(bound.name, "Wi-Fi");
        assert_eq!(bound.ip.as_deref(), Some("192.168.1.5"));

        let unbound = adapter_stat(&progress(None, "default-route"), &names);
        assert_eq!(unbound.adapter_id, None);
        assert_eq!(unbound.name, "Default route");

        let json = serde_json::to_value(&bound).unwrap();
        for key in [
            "adapter_id",
            "name",
            "ip",
            "downloaded_bytes",
            "speed_bytes_sec",
            "active_connections",
            "dropped",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn test_apply_overrides() {
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        let mut wifi = NetworkAdapter {
            id: format!("Wi-Fi:{ip}"),
            ip,
            is_ipv4: true,
            is_loopback: false,
            enabled: true,
        };

        let mut overrides = HashMap::new();
        // Override by name: disable Wi-Fi
        overrides.insert("Wi-Fi".to_string(), false);
        apply_overrides(&mut wifi, &overrides);
        assert!(!wifi.enabled);

        // Override by full id: re-enable Wi-Fi
        overrides.insert(format!("Wi-Fi:{ip}"), true);
        apply_overrides(&mut wifi, &overrides);
        assert!(wifi.enabled);

        // Non-usable (loopback) cannot be enabled
        let mut loopback = NetworkAdapter {
            id: "lo:127.0.0.1".into(),
            ip: "127.0.0.1".parse().unwrap(),
            is_ipv4: true,
            is_loopback: true,
            enabled: false,
        };
        overrides.insert("lo".to_string(), true);
        apply_overrides(&mut loopback, &overrides);
        assert!(!loopback.enabled);
    }

    #[test]
    fn test_apply_overrides_to_info_integration() {
        let ip: IpAddr = "192.168.1.10".parse().unwrap();
        let mut adapter = NetworkAdapter {
            id: format!("Ethernet:{ip}"),
            ip,
            is_ipv4: true,
            is_loopback: false,
            enabled: true,
        };
        let mut overrides = HashMap::new();
        overrides.insert("Ethernet".to_string(), false);
        apply_overrides(&mut adapter, &overrides);

        let info = to_info(adapter);
        assert_eq!(info.name, "Ethernet");
        assert!(!info.enabled);
        assert!(info.usable);
    }
}
