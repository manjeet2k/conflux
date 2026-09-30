use crate::adapter::{discover_adapters, NetworkAdapter};
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};

/// Default debounce duration to allow network configuration (DHCP, routing) to settle
/// when a new physical or virtual adapter appears.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(300);

/// Compares two adapter lists and returns `(added, removed)` usable/enabled adapters.
pub fn diff_adapters(
    old: &[NetworkAdapter],
    new: &[NetworkAdapter],
) -> (Vec<NetworkAdapter>, Vec<NetworkAdapter>) {
    let old_usable: Vec<&NetworkAdapter> = old.iter().filter(|a| a.enabled).collect();
    let new_usable: Vec<&NetworkAdapter> = new.iter().filter(|a| a.enabled).collect();

    let added: Vec<NetworkAdapter> = new_usable
        .iter()
        .filter(|n| !old_usable.iter().any(|o| o.ip == n.ip))
        .copied()
        .cloned()
        .collect();

    let removed: Vec<NetworkAdapter> = old_usable
        .iter()
        .filter(|o| !new_usable.iter().any(|n| n.ip == o.ip))
        .copied()
        .cloned()
        .collect();

    (added, removed)
}

/// Watches for OS-level network interface and IP address changes without polling.
///
/// Uses native Win32 IP Helper `NotifyUnicastIpAddressChange` on Windows,
/// Linux kernel Netlink route sockets (`NETLINK_ROUTE`) on Linux, and a safe
/// fallback for other environments.
pub struct NetworkWatcher {
    receiver: watch::Receiver<Vec<NetworkAdapter>>,
    _stop_tx: tokio::sync::oneshot::Sender<()>,
    _os_watcher: Option<Arc<os::OsWatcher>>,
}

impl NetworkWatcher {
    /// Starts watching network adapter changes with the specified debounce interval.
    pub fn start(debounce: Duration) -> Result<Self> {
        let initial = discover_adapters().unwrap_or_default();
        let (watch_tx, watch_rx) = watch::channel(initial);
        let (trigger_tx, mut trigger_rx) = mpsc::unbounded_channel::<()>();
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();

        let os_watcher = match os::OsWatcher::start(trigger_tx.clone()) {
            Ok(w) => {
                info!("Kernel-level non-polling network adapter watcher active");
                Some(Arc::new(w))
            }
            Err(e) => {
                warn!(
                    "Failed to start OS network listener ({:#}); falling back to quiet periodic refresh",
                    e
                );
                // Fallback timer trigger (every 5 seconds)
                let fallback_tx = trigger_tx.clone();
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(5));
                    loop {
                        interval.tick().await;
                        if fallback_tx.send(()).is_err() {
                            break;
                        }
                    }
                });
                None
            }
        };

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stop_rx => break,
                    Some(()) = trigger_rx.recv() => {
                        // Settle burst notifications (e.g. link UP -> DHCP request -> ACK -> IP bound)
                        tokio::select! {
                            biased;
                            _ = &mut stop_rx => break,
                            _ = tokio::time::sleep(debounce) => {}
                        }
                        // Drain any additional triggers accumulated during the debounce sleep
                        while trigger_rx.try_recv().is_ok() {}

                        match discover_adapters() {
                            Ok(current) => {
                                let changed = {
                                    let prev = watch_tx.borrow();
                                    *prev != current
                                };
                                if changed {
                                    let (added, removed) = {
                                        let prev = watch_tx.borrow();
                                        diff_adapters(&prev, &current)
                                    };
                                    debug!(
                                        "Network adapter change detected: {} current, +{} added, -{} removed",
                                        current.len(),
                                        added.len(),
                                        removed.len()
                                    );
                                    let _ = watch_tx.send(current);
                                }
                            }
                            Err(e) => {
                                error!("Failed discovering network adapters: {:#}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            receiver: watch_rx,
            _stop_tx: stop_tx,
            _os_watcher: os_watcher,
        })
    }

    /// Subscribes to network adapter updates.
    pub fn receiver(&self) -> watch::Receiver<Vec<NetworkAdapter>> {
        self.receiver.clone()
    }
}

// ─────────────────────────────────────────────────────────────
// OS-Specific Event Listeners
// ─────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod os {
    use anyhow::{bail, Result};
    use std::ffi::c_void;
    use tokio::sync::mpsc::UnboundedSender;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        CancelMibChangeNotify2, NotifyIpInterfaceChange, NotifyUnicastIpAddressChange,
        MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE, MIB_UNICASTIPADDRESS_ROW,
    };
    use windows_sys::Win32::Networking::WinSock::AF_UNSPEC;

    pub struct OsWatcher {
        address_handle: HANDLE,
        interface_handle: HANDLE,
        _address_tx: Box<UnboundedSender<()>>,
        _interface_tx: Box<UnboundedSender<()>>,
    }

    unsafe impl Send for OsWatcher {}
    unsafe impl Sync for OsWatcher {}

    impl OsWatcher {
        pub fn start(tx: UnboundedSender<()>) -> Result<Self> {
            let address_tx = Box::new(tx.clone());
            let mut address_handle: HANDLE = std::ptr::null_mut();
            let err = unsafe {
                NotifyUnicastIpAddressChange(
                    AF_UNSPEC as _,
                    Some(unicast_ip_change_callback),
                    address_tx.as_ref() as *const _ as *const c_void,
                    false,
                    &mut address_handle,
                )
            };
            if err != 0 {
                bail!(
                    "NotifyUnicastIpAddressChange failed with Win32 error {}",
                    err
                );
            }

            let interface_tx = Box::new(tx);
            let mut interface_handle: HANDLE = std::ptr::null_mut();
            let err = unsafe {
                NotifyIpInterfaceChange(
                    AF_UNSPEC as _,
                    Some(ip_interface_change_callback),
                    interface_tx.as_ref() as *const _ as *const c_void,
                    false,
                    &mut interface_handle,
                )
            };
            if err != 0 {
                unsafe {
                    CancelMibChangeNotify2(address_handle);
                }
                bail!("NotifyIpInterfaceChange failed with Win32 error {}", err);
            }

            Ok(Self {
                address_handle,
                interface_handle,
                _address_tx: address_tx,
                _interface_tx: interface_tx,
            })
        }
    }

    impl Drop for OsWatcher {
        fn drop(&mut self) {
            if !self.address_handle.is_null() {
                unsafe {
                    CancelMibChangeNotify2(self.address_handle);
                }
            }
            if !self.interface_handle.is_null() {
                unsafe {
                    CancelMibChangeNotify2(self.interface_handle);
                }
            }
        }
    }

    unsafe extern "system" fn unicast_ip_change_callback(
        callercontext: *const c_void,
        _row: *const MIB_UNICASTIPADDRESS_ROW,
        _notificationtype: MIB_NOTIFICATION_TYPE,
    ) {
        if !callercontext.is_null() {
            let tx = &*(callercontext as *const UnboundedSender<()>);
            let _ = tx.send(());
        }
    }

    unsafe extern "system" fn ip_interface_change_callback(
        callercontext: *const c_void,
        _row: *const MIB_IPINTERFACE_ROW,
        _notificationtype: MIB_NOTIFICATION_TYPE,
    ) {
        if !callercontext.is_null() {
            let tx = &*(callercontext as *const UnboundedSender<()>);
            let _ = tx.send(());
        }
    }
}

#[cfg(target_os = "linux")]
mod os {
    use anyhow::{bail, Result};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use tokio::io::unix::AsyncFd;
    use tokio::sync::mpsc::UnboundedSender;
    use tracing::{debug, warn};

    pub struct OsWatcher {
        stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl OsWatcher {
        pub fn start(tx: UnboundedSender<()>) -> Result<Self> {
            let fd = unsafe {
                libc::socket(
                    libc::AF_NETLINK,
                    libc::SOCK_RAW | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                    libc::NETLINK_ROUTE,
                )
            };
            if fd < 0 {
                bail!(
                    "Failed to create AF_NETLINK socket: {}",
                    std::io::Error::last_os_error()
                );
            }
            let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };

            let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
            addr.nl_family = libc::AF_NETLINK as u16;
            addr.nl_groups = (libc::RTMGRP_IPV4_IFADDR | libc::RTMGRP_LINK) as u32;

            let ret = unsafe {
                libc::bind(
                    owned_fd.as_raw_fd(),
                    &addr as *const _ as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
                )
            };
            if ret < 0 {
                bail!(
                    "Failed to bind AF_NETLINK socket to route multicast groups: {}",
                    std::io::Error::last_os_error()
                );
            }

            let async_fd = AsyncFd::new(owned_fd)?;
            let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();

            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    tokio::select! {
                        biased;
                        _ = &mut stop_rx => break,
                        guard = async_fd.readable() => {
                            match guard {
                                Ok(mut ready) => {
                                    let res = unsafe {
                                        libc::recv(
                                            async_fd.as_raw_fd(),
                                            buf.as_mut_ptr() as *mut libc::c_void,
                                            buf.len(),
                                            0,
                                        )
                                    };
                                    if res > 0 {
                                        debug!("Netlink route change event received ({} bytes)", res);
                                        let _ = tx.send(());
                                    }
                                    ready.clear_ready();
                                }
                                Err(e) => {
                                    warn!("Netlink AsyncFd read error: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                }
            });

            Ok(Self {
                stop_tx: Some(stop_tx),
            })
        }
    }

    impl Drop for OsWatcher {
        fn drop(&mut self) {
            if let Some(stop) = self.stop_tx.take() {
                let _ = stop.send(());
            }
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod os {
    use anyhow::Result;
    use tokio::sync::mpsc::UnboundedSender;

    pub struct OsWatcher;

    impl OsWatcher {
        pub fn start(_tx: UnboundedSender<()>) -> Result<Self> {
            anyhow::bail!("Kernel-level network notification not implemented on this OS");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn make_adapter(name: &str, ip: (u8, u8, u8, u8), enabled: bool) -> NetworkAdapter {
        let ip = std::net::IpAddr::V4(Ipv4Addr::new(ip.0, ip.1, ip.2, ip.3));
        NetworkAdapter {
            id: format!("{}:{}", name, ip),
            ip,
            is_ipv4: true,
            is_loopback: false,
            enabled,
        }
    }

    #[test]
    fn test_diff_adapters_detects_add_and_remove() {
        let a1 = make_adapter("eth0", (192, 168, 1, 10), true);
        let a2 = make_adapter("wlan0", (192, 168, 1, 20), true);
        let a3 = make_adapter("usb0", (192, 168, 42, 5), true);

        let initial = vec![a1.clone(), a2.clone()];
        let updated = vec![a2.clone(), a3.clone()];

        let (added, removed) = diff_adapters(&initial, &updated);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].id, a3.id);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, a1.id);
    }

    #[test]
    fn test_diff_adapters_ignores_disabled() {
        let a1 = make_adapter("eth0", (192, 168, 1, 10), true);
        let vpn = make_adapter("tun0", (10, 8, 0, 1), false);

        let initial = vec![a1.clone()];
        let updated = vec![a1.clone(), vpn];

        let (added, removed) = diff_adapters(&initial, &updated);
        assert!(added.is_empty());
        assert!(removed.is_empty());
    }

    #[tokio::test]
    async fn test_network_watcher_lifecycle() {
        let watcher = NetworkWatcher::start(Duration::from_millis(50));
        assert!(watcher.is_ok());
        let watcher = watcher.unwrap();
        let rx = watcher.receiver();
        let current = rx.borrow().clone();
        // Invariant: whatever adapters exist right now match discover_adapters
        let discovered = discover_adapters().unwrap();
        assert_eq!(current.len(), discovered.len());
    }
}
