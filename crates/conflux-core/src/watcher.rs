use crate::adapter::{discover_adapters, NetworkAdapter};
use anyhow::Result;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
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

/// Rescan period used when no OS change notifications are available (or they stopped).
pub const FALLBACK_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Keeps the OS notification registration alive; dropping it unregisters.
type OsGuard = Box<dyn std::any::Any + Send + Sync>;

/// Watches for OS-level network interface and IP address changes without polling.
///
/// Uses native Win32 IP Helper `NotifyUnicastIpAddressChange` on Windows,
/// Linux kernel Netlink route sockets (`NETLINK_ROUTE`) on Linux, and periodic
/// rescans ([`FALLBACK_POLL_INTERVAL`]) when those are unavailable or stop working.
pub struct NetworkWatcher {
    receiver: watch::Receiver<Vec<NetworkAdapter>>,
    _stop_tx: tokio::sync::oneshot::Sender<()>,
    _os_watcher: Option<OsGuard>,
}

/// Sends a trigger every `period` (first one immediately) until the receiver is gone.
fn spawn_fallback_poller(
    handle: &tokio::runtime::Handle,
    tx: UnboundedSender<()>,
    period: Duration,
) {
    handle.spawn(async move {
        let mut interval = tokio::time::interval(period);
        loop {
            interval.tick().await;
            if tx.send(()).is_err() {
                break;
            }
        }
    });
}

impl NetworkWatcher {
    /// Starts watching network adapter changes with the specified debounce interval.
    pub fn start(debounce: Duration) -> Result<Self> {
        Self::start_with(
            debounce,
            FALLBACK_POLL_INTERVAL,
            |tx| os::OsWatcher::start(tx).map(|w| Box::new(w) as OsGuard),
            discover_adapters,
        )
    }

    /// [`Self::start`] with injectable OS registration and discovery, for tests.
    fn start_with<R, D>(
        debounce: Duration,
        fallback_period: Duration,
        register: R,
        discover: D,
    ) -> Result<Self>
    where
        R: FnOnce(UnboundedSender<()>) -> Result<OsGuard>,
        D: Fn() -> Result<Vec<NetworkAdapter>> + Send + 'static,
    {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| {
            anyhow::anyhow!("NetworkWatcher must be started within a Tokio runtime context")
        })?;
        let (trigger_tx, mut trigger_rx) = mpsc::unbounded_channel::<()>();
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();

        // Register for OS notifications *before* the initial scan: a change between the two
        // is then either reflected in the scan or queued as a trigger, never lost.
        let os_watcher = match register(trigger_tx.clone()) {
            Ok(w) => {
                info!("Kernel-level non-polling network adapter watcher active");
                Some(w)
            }
            Err(e) => {
                warn!(
                    "Failed to start OS network listener ({:#}); falling back to quiet periodic refresh",
                    e
                );
                spawn_fallback_poller(&handle, trigger_tx.clone(), fallback_period);
                None
            }
        };
        // Only the OS listener / poller keep senders, so `recv() == None` means they died.
        drop(trigger_tx);

        let initial = discover().unwrap_or_else(|e| {
            error!("Initial network adapter discovery failed: {:#}", e);
            Vec::new()
        });
        let (watch_tx, watch_rx) = watch::channel(initial);

        let loop_handle = handle.clone();
        handle.spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stop_rx => break,
                    trigger = trigger_rx.recv() => {
                        if trigger.is_none() {
                            // The OS listener task exited (e.g. netlink socket error). Without
                            // this, change detection would silently stop forever.
                            warn!(
                                "OS network listener stopped; falling back to periodic refresh every {:?}",
                                fallback_period
                            );
                            let (tx, rx) = mpsc::unbounded_channel::<()>();
                            spawn_fallback_poller(&loop_handle, tx, fallback_period);
                            trigger_rx = rx;
                            continue;
                        }
                        // Settle burst notifications (e.g. link UP -> DHCP request -> ACK -> IP bound)
                        tokio::select! {
                            biased;
                            _ = &mut stop_rx => break,
                            _ = tokio::time::sleep(debounce) => {}
                        }
                        // Drain any additional triggers accumulated during the debounce sleep
                        while trigger_rx.try_recv().is_ok() {}

                        match discover() {
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use tokio::sync::mpsc::UnboundedSender;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        CancelMibChangeNotify2, NotifyIpInterfaceChange, NotifyUnicastIpAddressChange,
        MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE, MIB_UNICASTIPADDRESS_ROW,
    };
    use windows_sys::Win32::Networking::WinSock::AF_UNSPEC;

    struct CallbackState {
        active: AtomicBool,
        tx: Mutex<Option<UnboundedSender<()>>>,
    }

    pub struct OsWatcher {
        address_handle: HANDLE,
        interface_handle: HANDLE,
        state: *const CallbackState,
    }

    unsafe impl Send for OsWatcher {}
    unsafe impl Sync for OsWatcher {}

    impl OsWatcher {
        pub fn start(tx: UnboundedSender<()>) -> Result<Self> {
            let state = Box::into_raw(Box::new(CallbackState {
                active: AtomicBool::new(true),
                tx: Mutex::new(Some(tx)),
            }));

            let mut address_handle: HANDLE = std::ptr::null_mut();
            let err = unsafe {
                NotifyUnicastIpAddressChange(
                    AF_UNSPEC as _,
                    Some(unicast_ip_change_callback),
                    state as *const c_void,
                    false,
                    &mut address_handle,
                )
            };
            if err != 0 {
                unsafe { drop(Box::from_raw(state)) };
                bail!(
                    "NotifyUnicastIpAddressChange failed with Win32 error {}",
                    err
                );
            }

            let mut interface_handle: HANDLE = std::ptr::null_mut();
            let err = unsafe {
                NotifyIpInterfaceChange(
                    AF_UNSPEC as _,
                    Some(ip_interface_change_callback),
                    state as *const c_void,
                    false,
                    &mut interface_handle,
                )
            };
            if err != 0 {
                unsafe {
                    CancelMibChangeNotify2(address_handle);
                    (*state).active.store(false, Ordering::SeqCst);
                }
                bail!("NotifyIpInterfaceChange failed with Win32 error {}", err);
            }

            Ok(Self {
                address_handle,
                interface_handle,
                state,
            })
        }
    }

    impl Drop for OsWatcher {
        fn drop(&mut self) {
            if !self.state.is_null() {
                // Signal in-flight callbacks to stand down immediately and drop sender.
                unsafe {
                    (*self.state).active.store(false, Ordering::SeqCst);
                    if let Ok(mut lock) = (*self.state).tx.lock() {
                        lock.take();
                    }
                }
            }
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
            // Note: `self.state` is intentionally not freed with from_raw() here because
            // `CancelMibChangeNotify2` returns asynchronously and in-flight callbacks may
            // still be executing on a Windows threadpool thread. Leaking this small (~48 byte)
            // structure guarantees memory safety against use-after-free crashes.
        }
    }

    unsafe extern "system" fn unicast_ip_change_callback(
        callercontext: *const c_void,
        _row: *const MIB_UNICASTIPADDRESS_ROW,
        _notificationtype: MIB_NOTIFICATION_TYPE,
    ) {
        if !callercontext.is_null() {
            let state = &*(callercontext as *const CallbackState);
            if state.active.load(Ordering::SeqCst) {
                if let Ok(guard) = state.tx.lock() {
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.send(());
                    }
                }
            }
        }
    }

    unsafe extern "system" fn ip_interface_change_callback(
        callercontext: *const c_void,
        _row: *const MIB_IPINTERFACE_ROW,
        _notificationtype: MIB_NOTIFICATION_TYPE,
    ) {
        if !callercontext.is_null() {
            let state = &*(callercontext as *const CallbackState);
            if state.active.load(Ordering::SeqCst) {
                if let Ok(guard) = state.tx.lock() {
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.send(());
                    }
                }
            }
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
            name: name.to_string(),
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

    #[test]
    fn test_diff_adapters_detects_toggle_state() {
        let mut a1 = make_adapter("eth0", (192, 168, 1, 10), true);
        let mut a2 = make_adapter("wlan0", (192, 168, 1, 20), false);

        let initial = vec![a1.clone(), a2.clone()];

        // Toggling a1 from true to false -> removed
        // Toggling a2 from false to true -> added
        a1.enabled = false;
        a2.enabled = true;
        let updated = vec![a1.clone(), a2.clone()];

        let (added, removed) = diff_adapters(&initial, &updated);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].id, a2.id);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, a1.id);
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_watcher_registers_before_initial_scan() {
        let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let reg_log = log.clone();
        let scan_log = log.clone();
        let _watcher = NetworkWatcher::start_with(
            Duration::from_millis(1),
            Duration::from_secs(3600),
            move |_tx| {
                reg_log.lock().unwrap().push("register");
                Ok(Box::new(()) as OsGuard)
            },
            move || {
                scan_log.lock().unwrap().push("scan");
                Ok(Vec::new())
            },
        )
        .unwrap();
        assert_eq!(log.lock().unwrap().first(), Some(&"register"));
        assert_eq!(log.lock().unwrap().get(1), Some(&"scan"));
    }

    #[tokio::test]
    async fn test_watcher_falls_back_to_polling_when_os_listener_dies() {
        let scans = Arc::new(AtomicUsize::new(0));
        let counter = scans.clone();
        let watcher = NetworkWatcher::start_with(
            Duration::from_millis(1),
            Duration::from_millis(20),
            // The "OS listener" drops its sender right away, like a netlink task that errored.
            |tx| {
                drop(tx);
                Ok(Box::new(()) as OsGuard)
            },
            move || {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                // A new adapter shows up only after the OS listener is already gone.
                Ok(if n == 0 {
                    Vec::new()
                } else {
                    vec![make_adapter("usb0", (192, 168, 42, 5), true)]
                })
            },
        )
        .unwrap();
        let mut rx = watcher.receiver();
        tokio::time::timeout(Duration::from_secs(5), rx.changed())
            .await
            .expect("change must still be detected after the OS listener died")
            .unwrap();
        assert_eq!(rx.borrow().len(), 1);
        // Polling keeps going.
        let before = scans.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(scans.load(Ordering::SeqCst) > before);
    }

    #[tokio::test]
    async fn test_watcher_polls_when_os_listener_unavailable() {
        let scans = Arc::new(AtomicUsize::new(0));
        let counter = scans.clone();
        let _watcher = NetworkWatcher::start_with(
            Duration::from_millis(1),
            Duration::from_millis(20),
            |_tx| Err(anyhow::anyhow!("not supported")),
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Vec::new())
            },
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(scans.load(Ordering::SeqCst) >= 3);
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
