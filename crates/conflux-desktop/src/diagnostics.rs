//! The "Copy diagnostics" payload. It is built field by field from an allow-list so nothing
//! path-, user- or credential-bearing can sneak in: no folders, no URLs, no full IP addresses.

use crate::adapters::{AdapterInfo, AdapterKind, DisabledReason};
use crate::redact::mask_ip;
use crate::settings::{Settings, ThemePreference};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DiagAdapter {
    pub name: String,
    pub kind: AdapterKind,
    pub enabled: bool,
    /// The engine can bind to it (IPv4, not loopback, not link-local). Discovery only lists
    /// adapters that are operationally up, so a listed adapter has a working link.
    pub usable: bool,
    pub disabled_reason: Option<DisabledReason>,
    /// Address with the host part masked, e.g. `192.168.1.x`.
    pub subnet: String,
}

#[derive(Debug, Serialize)]
pub struct DiagSettings {
    pub schema_version: u32,
    pub theme: ThemePreference,
    pub connections_per_adapter: u32,
    pub chunk_size_mb: u32,
    pub notify_on_complete: bool,
    pub close_to_tray: bool,
    pub auto_aggregate_adapters: bool,
    /// Whether a custom download folder is set (the folder itself is not included).
    pub custom_save_dir: bool,
    pub adapter_override_count: usize,
}

#[derive(Debug, Serialize)]
pub struct Diagnostics {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub webview_version: Option<String>,
    pub adapters: Vec<DiagAdapter>,
    pub settings: DiagSettings,
    /// Last WARN/ERROR log lines with URLs, paths and IP addresses scrubbed.
    pub recent_errors: Vec<String>,
}

pub fn build(
    settings: &Settings,
    adapters: &[AdapterInfo],
    webview_version: Option<String>,
    recent_errors: Vec<String>,
) -> Diagnostics {
    Diagnostics {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        webview_version,
        adapters: adapters
            .iter()
            .map(|a| DiagAdapter {
                name: a.name.clone(),
                kind: a.kind,
                enabled: a.enabled,
                usable: a.usable,
                disabled_reason: a.disabled_reason,
                subnet: mask_ip(&a.ip).unwrap_or_else(|| "<unknown>".to_string()),
            })
            .collect(),
        settings: DiagSettings {
            schema_version: settings.schema_version,
            theme: settings.theme,
            connections_per_adapter: settings.connections_per_adapter,
            chunk_size_mb: settings.chunk_size_mb,
            notify_on_complete: settings.notify_on_complete,
            close_to_tray: settings.close_to_tray,
            auto_aggregate_adapters: settings.auto_aggregate_adapters,
            custom_save_dir: settings.default_save_dir.is_some(),
            adapter_override_count: settings.adapter_overrides.len(),
        },
        recent_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, ip: &str) -> AdapterInfo {
        AdapterInfo {
            id: format!("{name}:{ip}"),
            name: name.into(),
            ip: ip.into(),
            is_ipv4: !ip.contains(':'),
            is_loopback: false,
            enabled: true,
            usable: true,
            kind: AdapterKind::Wifi,
            disabled_reason: None,
        }
    }

    #[test]
    fn diagnostics_exclude_paths_ips_and_credentials() {
        let mut settings = Settings {
            default_save_dir: Some(r"C:\Users\bob\Downloads".into()),
            ..Settings::default()
        };
        settings
            .adapter_overrides
            .insert("bob's phone".into(), false);
        let adapters = [
            info("Wi-Fi", "192.168.1.57"),
            info("Ethernet", "fe80::4022:7688:97e1:1"),
        ];
        let d = build(&settings, &adapters, Some("130.0.1".into()), vec![]);
        let json = serde_json::to_string_pretty(&d).unwrap();

        for leaked in [
            "bob",
            "Users",
            "Downloads",
            "192.168.1.57",
            "4022",
            "default_save_dir",
        ] {
            assert!(!json.contains(leaked), "{leaked} leaked:\n{json}");
        }
        assert!(json.contains("192.168.1.x"));
        assert!(json.contains("<ipv6>"));
        assert!(d.settings.custom_save_dir);
        assert_eq!(d.settings.adapter_override_count, 1);

        // Structure guard: only these top-level keys exist.
        let value = serde_json::to_value(&d).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "adapters",
                "app_version",
                "arch",
                "os",
                "recent_errors",
                "settings",
                "webview_version"
            ]
        );
    }
}
