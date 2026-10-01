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

/// Why discovery leaves an adapter disabled by default (shown as a hint on the Network page).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisabledReason {
    Loopback,
    LinkLocal,
    NoIpv4,
    /// Usable address, but the OS description / interface type / name marks it virtual
    /// (VPN, TAP, bridge, container). A real uplink behind such a name can be enabled by hand.
    Virtual,
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
    /// Set when discovery disables the adapter by default, whatever the user chose since.
    pub disabled_reason: Option<DisabledReason>,
}

/// OS interface name of the adapter (falls back to the name part of its `"<name>:<ip>"` id).
pub fn adapter_name(adapter: &NetworkAdapter) -> String {
    if !adapter.name.is_empty() {
        return adapter.name.clone();
    }
    adapter
        .id
        .strip_suffix(&format!(":{}", adapter.ip))
        .unwrap_or(&adapter.id)
        .to_string()
}

/// Splits an adapter id `"<name>:<ip>"` into its parts. The name is everything before the
/// first ':' whose remainder parses as an IP address (IPv6 addresses contain ':' themselves).
/// `None` if no such split exists, i.e. `id` is a plain interface name.
pub fn split_adapter_id(id: &str) -> Option<(&str, IpAddr)> {
    id.match_indices(':').find_map(|(idx, _)| {
        let ip = id[idx + 1..].parse::<IpAddr>().ok()?;
        Some((&id[..idx], ip))
    })
}

/// The adapter-overrides key for an adapter id sent by the UI: the interface name, so the
/// override survives DHCP address changes. Uses the discovered adapter when known, else
/// parses the id.
pub fn override_key(id: &str, known: &[NetworkAdapter]) -> String {
    if let Some(adapter) = known.iter().find(|a| a.id == id) {
        return adapter_name(adapter);
    }
    match split_adapter_id(id) {
        Some((name, _)) => name.to_string(),
        None => id.to_string(),
    }
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

/// Applies the user's enable/disable choice. Overrides are keyed by interface name; a legacy
/// full-id key (`"<name>:<ip>"`, from before keys were migrated) is used only when there is
/// no name key. Unusable adapters are always disabled.
pub fn apply_overrides(adapter: &mut NetworkAdapter, overrides: &HashMap<String, bool>) {
    if !is_usable(adapter) {
        adapter.enabled = false;
        return;
    }
    let name = adapter_name(adapter);
    if let Some(&enabled) = overrides.get(&name).or_else(|| overrides.get(&adapter.id)) {
        adapter.enabled = enabled;
    }
}

/// Rewrites legacy `"<name>:<ip>"` override keys to `"<name>"`. Conflicts resolve
/// deterministically: an existing plain-name key always wins; among several legacy keys for
/// the same name (different addresses), disabled wins, so an opt-out (e.g. a metered link)
/// is never silently turned back on.
pub fn migrate_overrides(overrides: HashMap<String, bool>) -> HashMap<String, bool> {
    let mut migrated: HashMap<String, bool> = HashMap::new();
    let mut legacy: HashMap<String, bool> = HashMap::new();
    for (key, enabled) in overrides {
        match split_adapter_id(&key) {
            Some((name, _)) => {
                let entry = legacy.entry(name.to_string()).or_insert(enabled);
                *entry = *entry && enabled;
            }
            None => {
                migrated.insert(key, enabled);
            }
        }
    }
    for (name, enabled) in legacy {
        migrated.entry(name).or_insert(enabled);
    }
    migrated
}

/// Why discovery disabled `adapter` by default. `discovery_default` is the adapter's
/// `enabled` flag as discovery reported it, before user overrides: for an otherwise usable
/// adapter, discovery only says "disabled" when its Windows description, interface type or
/// name marks it virtual, signals the desktop app cannot see on `NetworkAdapter`.
pub fn disabled_reason(
    adapter: &NetworkAdapter,
    discovery_default: bool,
) -> Option<DisabledReason> {
    if adapter.is_loopback {
        Some(DisabledReason::Loopback)
    } else if is_link_local(&adapter.ip) {
        Some(DisabledReason::LinkLocal)
    } else if !(adapter.is_ipv4 && adapter.ip.is_ipv4()) {
        Some(DisabledReason::NoIpv4)
    } else if !discovery_default {
        Some(DisabledReason::Virtual)
    } else {
        None
    }
}

/// `discovery_default`: see [`disabled_reason`].
pub fn to_info(adapter: NetworkAdapter, discovery_default: bool) -> AdapterInfo {
    let name = adapter_name(&adapter);
    let disabled_reason = disabled_reason(&adapter, discovery_default);
    let mut kind = classify_kind(&name, adapter.is_loopback);
    if disabled_reason == Some(DisabledReason::Virtual) {
        // e.g. a TAP adapter called "Local Area Connection 2": only the description gives it away.
        kind = AdapterKind::Virtual;
    }
    AdapterInfo {
        kind,
        usable: is_usable(&adapter),
        name,
        ip: adapter.ip.to_string(),
        is_ipv4: adapter.is_ipv4,
        is_loopback: adapter.is_loopback,
        enabled: adapter.enabled,
        disabled_reason,
        id: adapter.id,
    }
}

/// Converts adapters to UI records; `defaults` maps adapter id -> discovery default
/// (missing = enabled by default).
pub fn to_infos(
    adapters: Vec<NetworkAdapter>,
    defaults: &HashMap<String, bool>,
) -> Vec<AdapterInfo> {
    adapters
        .into_iter()
        .map(|a| {
            let default = defaults.get(&a.id).copied().unwrap_or(true);
            to_info(a, default)
        })
        .collect()
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
        last_error: p.last_error.as_deref().map(crate::redact::redact_urls),
        drop_reason: p.drop_reason.as_deref().map(crate::redact::redact_urls),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, ip: &str, loopback: bool) -> NetworkAdapter {
        let ip: std::net::IpAddr = ip.parse().unwrap();
        NetworkAdapter {
            id: format!("{name}:{ip}"),
            name: name.to_string(),
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
    fn disabled_reasons_and_virtual_kind_from_discovery_default() {
        // A TAP adapter with a generic Windows name: only discovery's description check
        // knows it is virtual (it reported enabled = false for a usable address).
        let tap = adapter("Local Area Connection 2", "10.8.0.2", false);
        assert_eq!(classify_kind(&tap.name, false), AdapterKind::Ethernet);
        let info = to_info(tap.clone(), false);
        assert_eq!(info.kind, AdapterKind::Virtual);
        assert_eq!(info.disabled_reason, Some(DisabledReason::Virtual));
        assert_eq!(
            serde_json::to_value(&info).unwrap()["disabled_reason"],
            "virtual"
        );
        // The same adapter that discovery enabled keeps its name-based kind.
        let info = to_info(tap, true);
        assert_eq!(info.kind, AdapterKind::Ethernet);
        assert_eq!(info.disabled_reason, None);

        // Other reasons do not depend on the discovery flag.
        let cases = [
            (adapter("lo", "127.0.0.1", true), DisabledReason::Loopback),
            (
                adapter("Ethernet", "169.254.3.4", false),
                DisabledReason::LinkLocal,
            ),
            (
                adapter("Ethernet", "fe80::1", false),
                DisabledReason::LinkLocal,
            ),
            (
                adapter("Ethernet", "2401:4900::1", false),
                DisabledReason::NoIpv4,
            ),
            (
                adapter("br0", "192.168.1.9", false),
                DisabledReason::Virtual,
            ),
        ];
        for (a, reason) in cases {
            assert_eq!(disabled_reason(&a, false), Some(reason), "{}", a.id);
        }
        // A user override never invents or hides the default reason.
        let mut overridden = adapter("br0", "192.168.1.9", false);
        overridden.enabled = true;
        assert_eq!(
            to_info(overridden, false).disabled_reason,
            Some(DisabledReason::Virtual)
        );
        assert_eq!(
            disabled_reason(&adapter("Wi-Fi", "192.168.1.5", false), true),
            None
        );

        let defaults = HashMap::from([("br0:192.168.1.9".to_string(), false)]);
        let infos = to_infos(
            vec![
                adapter("br0", "192.168.1.9", false),
                adapter("Wi-Fi", "192.168.1.5", false),
            ],
            &defaults,
        );
        assert_eq!(infos[0].disabled_reason, Some(DisabledReason::Virtual));
        assert_eq!(infos[1].disabled_reason, None);
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

        let info = to_info(v4, true);
        assert_eq!(info.disabled_reason, None);
        assert_eq!(info.kind, AdapterKind::Wifi);
        assert_eq!(serde_json::to_value(&info).unwrap()["kind"], "wifi");
    }

    #[test]
    fn adapter_stats_map_back_to_adapter_ids() {
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        let adapters = vec![NetworkAdapter {
            id: format!("Wi-Fi:{ip}"),
            name: "Wi-Fi".into(),
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
            last_error: None,
            drop_reason: None,
        };

        let bound = adapter_stat(&progress(Some(ip), "192.168.1.5"), &names);
        assert_eq!(bound.adapter_id.as_deref(), Some("Wi-Fi:192.168.1.5"));
        assert_eq!(bound.name, "Wi-Fi");
        assert_eq!(bound.ip.as_deref(), Some("192.168.1.5"));

        let mut failing = progress(Some(ip), "192.168.1.5");
        failing.last_error = Some("GET https://u:p@h.example/f?token=1 failed".into());
        failing.drop_reason = Some("dropped after 3 consecutive failures".into());
        let stat = adapter_stat(&failing, &names);
        let shown = stat.last_error.unwrap();
        assert!(
            !shown.contains("u:p") && !shown.contains("token=1"),
            "{shown}"
        );
        assert_eq!(
            stat.drop_reason.as_deref(),
            Some("dropped after 3 consecutive failures")
        );

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
            "last_error",
            "drop_reason",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn test_apply_overrides() {
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        let mut wifi = NetworkAdapter {
            id: format!("Wi-Fi:{ip}"),
            name: "Wi-Fi".into(),
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

        // A legacy full-id key never beats the name key.
        overrides.insert(format!("Wi-Fi:{ip}"), true);
        apply_overrides(&mut wifi, &overrides);
        assert!(!wifi.enabled);

        // Without a name key the legacy full-id key applies.
        overrides.remove("Wi-Fi");
        wifi.enabled = false;
        apply_overrides(&mut wifi, &overrides);
        assert!(wifi.enabled);

        // A name key survives a DHCP address change.
        let mut moved = adapter("Ethernet", "10.0.0.7", false);
        overrides.insert("Ethernet".into(), false);
        overrides.insert("Ethernet:10.0.0.3".into(), true);
        apply_overrides(&mut moved, &overrides);
        assert!(!moved.enabled);

        // Non-usable (loopback) cannot be enabled
        let mut loopback = NetworkAdapter {
            id: "lo:127.0.0.1".into(),
            name: "lo".into(),
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
            name: "Ethernet".into(),
            ip,
            is_ipv4: true,
            is_loopback: false,
            enabled: true,
        };
        let mut overrides = HashMap::new();
        overrides.insert("Ethernet".to_string(), false);
        apply_overrides(&mut adapter, &overrides);

        let info = to_info(adapter, true);
        assert_eq!(info.name, "Ethernet");
        assert!(!info.enabled);
        assert!(info.usable);
    }

    #[test]
    fn split_ids_with_ipv4_and_ipv6() {
        let v4: IpAddr = "192.168.1.5".parse().unwrap();
        let v6: IpAddr = "fe80::1".parse().unwrap();
        assert_eq!(split_adapter_id("Wi-Fi:192.168.1.5"), Some(("Wi-Fi", v4)));
        assert_eq!(
            split_adapter_id("Ethernet 2:fe80::1"),
            Some(("Ethernet 2", v6))
        );
        assert_eq!(
            split_adapter_id("odd:name:192.168.1.5"),
            Some(("odd:name", v4))
        );
        assert_eq!(split_adapter_id("Wi-Fi"), None);
        assert_eq!(split_adapter_id("a:b"), None);
    }

    #[test]
    fn override_key_uses_interface_name() {
        let known = vec![adapter("Wi-Fi", "192.168.1.5", false)];
        assert_eq!(override_key("Wi-Fi:192.168.1.5", &known), "Wi-Fi");
        // Not discovered (adapter went away): parse the id.
        assert_eq!(override_key("Ethernet:fe80::1", &known), "Ethernet");
        assert_eq!(override_key("Ethernet", &known), "Ethernet");
    }

    #[test]
    fn migrate_legacy_override_keys() {
        let overrides: HashMap<String, bool> = [
            ("Wi-Fi:192.168.1.5", false),
            ("Ethernet", true),
            ("Ethernet:10.0.0.3", false),
            ("usb0:10.1.1.2", true),
            ("usb0:10.1.1.9", false),
            ("eth1:fe80::2", true),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let migrated = migrate_overrides(overrides);
        let expected: HashMap<String, bool> = [
            ("Wi-Fi", false),
            ("Ethernet", true),
            ("usb0", false),
            ("eth1", true),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        assert_eq!(migrated, expected);
        // Idempotent.
        assert_eq!(migrate_overrides(migrated.clone()), migrated);
    }
}
