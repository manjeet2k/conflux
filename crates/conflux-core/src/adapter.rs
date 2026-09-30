use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkAdapter {
    pub id: String,
    pub ip: IpAddr,
    pub is_ipv4: bool,
    pub is_loopback: bool,
    pub enabled: bool,
}

/// Enumerates all active local network adapter IP addresses available on the system.
pub fn discover_adapters() -> Result<Vec<NetworkAdapter>> {
    let mut adapters = Vec::new();

    // Use local_ip_address crate to inspect interfaces
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in interfaces {
            let is_loopback =
                ip.is_loopback() || name.starts_with("lo") || name.contains("loopback");
            let is_ipv4 = ip.is_ipv4();

            adapters.push(NetworkAdapter {
                id: format!("{}:{}", name, ip),
                ip,
                is_ipv4,
                is_loopback,
                enabled: !is_loopback && is_ipv4,
            });
        }
    }

    Ok(adapters)
}

/// Builds an HTTP client explicitly bound to the designated `local_ip` address.
/// Outgoing packets from this client will be routed through the network adapter
/// owning `local_ip`.
pub fn build_bound_http_client(local_ip: Option<IpAddr>) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30));

    if let Some(ip) = local_ip {
        builder = builder.local_address(Some(ip));
    }

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_adapters() {
        let adapters = discover_adapters().expect("Should discover adapters");
        println!("Discovered adapters: {:?}", adapters);
        assert!(
            !adapters.is_empty(),
            "Should discover at least loopback or eth0"
        );
    }

    #[test]
    fn test_build_client_unbound() {
        let client = build_bound_http_client(None);
        assert!(client.is_ok());
    }
}
