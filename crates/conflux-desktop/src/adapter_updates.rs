//! Reliable, ordered delivery of adapter add/remove events to a running download.
//!
//! `refresh_adapters` records the new adapter list before forwarding the difference, so a
//! dropped event is never re-derived later. Delivery therefore waits for room in the
//! download's channel instead of giving up on a full queue, but only ever from a spawned task
//! so the watcher / UI caller never blocks.

use conflux_core::{AdapterUpdate, NetworkAdapter};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::warn;

/// How long one queued update may wait for room in a download's channel before the download
/// is considered stuck and the update is dropped (with a warning).
pub const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// The engine updates for one adapter-set change, in delivery order: additions then removals.
pub fn adapter_updates(added: &[NetworkAdapter], removed: &[NetworkAdapter]) -> Vec<AdapterUpdate> {
    added
        .iter()
        .cloned()
        .map(AdapterUpdate::Add)
        .chain(removed.iter().map(|a| AdapterUpdate::Remove(a.ip)))
        .collect()
}

/// Sends `updates` in order, waiting up to `timeout` for room for each. Stops when the
/// download has finished (channel closed). Returns how many updates were delivered.
pub async fn deliver(
    task_id: String,
    tx: mpsc::Sender<AdapterUpdate>,
    updates: Vec<AdapterUpdate>,
    timeout: Duration,
) -> usize {
    let mut delivered = 0;
    for update in updates {
        match tx.send_timeout(update, timeout).await {
            Ok(()) => delivered += 1,
            Err(mpsc::error::SendTimeoutError::Closed(_)) => break,
            Err(mpsc::error::SendTimeoutError::Timeout(update)) => {
                warn!(task_id = %task_id, "Download did not accept {update:?} within {timeout:?}; dropped");
            }
        }
    }
    delivered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, ip: &str) -> NetworkAdapter {
        let ip: std::net::IpAddr = ip.parse().unwrap();
        NetworkAdapter {
            id: format!("{name}:{ip}"),
            name: name.to_string(),
            ip,
            is_ipv4: true,
            is_loopback: false,
            enabled: true,
        }
    }

    #[test]
    fn updates_are_additions_then_removals() {
        let a = adapter("Wi-Fi", "10.0.0.2");
        let b = adapter("Eth", "10.0.0.3");
        assert_eq!(
            adapter_updates(std::slice::from_ref(&a), std::slice::from_ref(&b)),
            vec![AdapterUpdate::Add(a), AdapterUpdate::Remove(b.ip)]
        );
        assert!(adapter_updates(&[], &[]).is_empty());
    }

    #[tokio::test]
    async fn full_queue_is_waited_out_not_dropped() {
        let (tx, mut rx) = mpsc::channel(1);
        let updates: Vec<_> = (1..=5u8)
            .map(|i| AdapterUpdate::Remove(format!("10.0.0.{i}").parse().unwrap()))
            .collect();
        let sender = tokio::spawn(deliver(
            "t".into(),
            tx,
            updates.clone(),
            Duration::from_secs(5),
        ));
        // A slow consumer: every update must still arrive, in order.
        let mut got = Vec::new();
        while let Some(u) = rx.recv().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
            got.push(u);
        }
        assert_eq!(sender.await.unwrap(), 5);
        assert_eq!(got, updates);
    }

    #[tokio::test]
    async fn stuck_receiver_times_out_and_closed_channel_stops() {
        let (tx, rx) = mpsc::channel(1);
        let ups = vec![
            AdapterUpdate::Remove("10.0.0.1".parse().unwrap()),
            AdapterUpdate::Remove("10.0.0.2".parse().unwrap()),
        ];
        // Never drained: first fits, second times out and is dropped.
        let n = deliver(
            "t".into(),
            tx.clone(),
            ups.clone(),
            Duration::from_millis(30),
        )
        .await;
        assert_eq!(n, 1);
        drop(rx);
        assert_eq!(
            deliver("t".into(), tx, ups, Duration::from_millis(30)).await,
            0
        );
    }
}
