//! One live bandwidth budget shared by all downloads and adapter workers.

use std::time::Duration;
use tokio::sync::{watch, Mutex};
use tokio::time::Instant;

struct Bucket {
    rate: u64,
    tokens: f64,
    updated: Instant,
}

/// A cancellation-safe token bucket. Zero means unlimited. At most 100 ms of idle
/// credit is retained; changing the limit wakes sleepers and discards old credit.
pub struct BandwidthLimiter {
    rate: watch::Sender<u64>,
    bucket: Mutex<Bucket>,
}

impl BandwidthLimiter {
    pub fn new(bytes_per_second: u64) -> Self {
        Self {
            rate: watch::channel(bytes_per_second).0,
            bucket: Mutex::new(Bucket {
                rate: bytes_per_second,
                tokens: 0.0,
                updated: Instant::now(),
            }),
        }
    }

    pub fn set_limit(&self, bytes_per_second: u64) {
        self.rate.send_if_modified(|rate| {
            if *rate == bytes_per_second {
                false
            } else {
                *rate = bytes_per_second;
                true
            }
        });
    }

    /// Accounts for received body bytes before they are committed. Dropping this
    /// future releases its place in the FIFO mutex queue without reserving future
    /// bandwidth. Small portions let other workers share the same budget fairly.
    pub async fn consume(&self, mut bytes: usize) {
        let mut changes = self.rate.subscribe();
        while bytes > 0 {
            if *changes.borrow_and_update() == 0 {
                return;
            }
            let mut bucket = self.bucket.lock().await;
            loop {
                let rate = *changes.borrow_and_update();
                if rate == 0 {
                    return;
                }
                let now = Instant::now();
                let capacity = (rate as f64 / 10.0).max(1.0);
                if bucket.rate != rate {
                    bucket.rate = rate;
                    bucket.tokens = 0.0;
                } else {
                    bucket.tokens = (bucket.tokens
                        + now.duration_since(bucket.updated).as_secs_f64() * rate as f64)
                        .min(capacity);
                }
                bucket.updated = now;
                let portion = bytes.min(16 * 1024).min(capacity.floor() as usize);
                if bucket.tokens >= portion as f64 {
                    bucket.tokens -= portion as f64;
                    bytes -= portion;
                    break;
                }
                let delay = Duration::from_secs_f64((portion as f64 - bucket.tokens) / rate as f64);
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {},
                    _ = changes.changed() => {},
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test(start_paused = true)]
    async fn workers_share_one_budget_and_idle_credit_is_bounded() {
        let limiter = Arc::new(BandwidthLimiter::new(100_000));
        let start = Instant::now();
        let a = Arc::clone(&limiter);
        let b = Arc::clone(&limiter);
        let a = tokio::spawn(async move { a.consume(50_000).await });
        let b = tokio::spawn(async move { b.consume(50_000).await });
        a.await.unwrap();
        b.await.unwrap();
        assert!(start.elapsed() >= Duration::from_secs(1));
        tokio::time::sleep(Duration::from_secs(10)).await;
        let start = Instant::now();
        limiter.consume(100_000).await;
        assert!(start.elapsed() >= Duration::from_millis(900));
    }

    #[tokio::test(start_paused = true)]
    async fn live_changes_wake_waiters_and_cancellation_leaves_no_debt() {
        let limiter = Arc::new(BandwidthLimiter::new(1));
        let a = Arc::clone(&limiter);
        let waiting = tokio::spawn(async move { a.consume(100_000).await });
        tokio::task::yield_now().await;
        waiting.abort();
        let _ = waiting.await;
        limiter.set_limit(100_000);
        let start = Instant::now();
        limiter.consume(10_000).await;
        assert!(start.elapsed() <= Duration::from_millis(102));
        let a = Arc::clone(&limiter);
        let waiting = tokio::spawn(async move { a.consume(1_000_000).await });
        tokio::task::yield_now().await;
        limiter.set_limit(0);
        let start = Instant::now();
        waiting.await.unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
        limiter.consume(usize::MAX).await;
        limiter.set_limit(100_000);
        let start = Instant::now();
        limiter.consume(10_000).await;
        assert!(start.elapsed() >= Duration::from_millis(100));
    }
}
