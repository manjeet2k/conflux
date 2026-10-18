//! Slow mode must pace both transfer paths without changing file bytes.
mod support;

use conflux_core::{BandwidthLimiter, DownloadCancelled, DownloadEngine};
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::{payload, sha256_hex, RangeMode, ServerConfig, TestServer};
use tokio::sync::{mpsc, watch};

#[tokio::test]
async fn range_and_single_stream_downloads_share_the_same_limit() {
    let data = payload(256 * 1024, 41);
    let ranged = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let mut config = ServerConfig::new(Arc::clone(&data));
    config.range_mode = RangeMode::Ignore;
    let single = TestServer::start(config).await;
    let limiter = Arc::new(BandwidthLimiter::new(256 * 1024));
    let engine = DownloadEngine::new(4)
        .unwrap()
        .with_block_size(16 * 1024)
        .with_bandwidth_limiter(limiter);
    let a = engine.probe(&ranged.url("/a.bin")).await.unwrap();
    let b = engine.probe(&single.url("/b.bin")).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let a_path = dir.path().join("a.bin");
    let b_path = dir.path().join("b.bin");
    let (_cancel, cancel) = watch::channel(false);
    let start = Instant::now();
    let (a, b) = tokio::join!(
        engine.download(&a, &a_path, &[], None, cancel.clone()),
        engine.download(&b, &b_path, &[], None, cancel),
    );
    assert_eq!(a.unwrap(), sha256_hex(&data));
    assert_eq!(b.unwrap(), sha256_hex(&data));
    assert!(start.elapsed() >= Duration::from_millis(1800));
    assert_eq!(std::fs::read(a_path).unwrap(), *data);
    assert_eq!(std::fs::read(b_path).unwrap(), *data);
}

#[tokio::test]
async fn active_download_observes_enabling_changing_and_disabling_the_limit() {
    let data = payload(1024 * 1024, 42);
    let mut config = ServerConfig::new(Arc::clone(&data));
    config.throttle = Some(Duration::from_millis(10));
    let server = TestServer::start(config).await;
    let limiter = Arc::new(BandwidthLimiter::new(0));
    let engine = DownloadEngine::new(1)
        .unwrap()
        .with_bandwidth_limiter(Arc::clone(&limiter));
    let probe = engine.probe(&server.url("/data.bin")).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.bin");
    let output = path.clone();
    let (_cancel, cancel) = watch::channel(false);
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move {
        engine
            .download(&probe, &output, &[], Some(tx), cancel)
            .await
    });
    // Wait for actual received bytes before toggling a running transfer.
    tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(progress) = rx.recv().await {
            if progress.downloaded_bytes > 0 {
                break;
            }
        }
    })
    .await
    .unwrap();
    limiter.set_limit(16 * 1024);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!task.is_finished());
    limiter.set_limit(32 * 1024);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!task.is_finished());
    limiter.set_limit(0);
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert_eq!(std::fs::read(path).unwrap(), *data);
}

#[tokio::test]
async fn cancelling_a_throttled_download_is_prompt_and_can_resume() {
    let data = payload(256 * 1024, 43);
    let server = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let limiter = Arc::new(BandwidthLimiter::new(1));
    let engine = DownloadEngine::new(4)
        .unwrap()
        .with_block_size(16 * 1024)
        .with_bandwidth_limiter(Arc::clone(&limiter));
    let probe = engine.probe(&server.url("/data.bin")).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.bin");
    let (cancel, rx) = watch::channel(false);
    let download = engine.download(&probe, &path, &[], None, rx);
    let stop = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.send_replace(true);
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(download, stop)
    })
    .await
    .unwrap();
    assert!(result
        .unwrap_err()
        .downcast_ref::<DownloadCancelled>()
        .is_some());
    limiter.set_limit(0);
    let (_cancel, rx) = watch::channel(false);
    assert_eq!(
        engine.resume(&probe, &path, &[], None, rx).await.unwrap(),
        sha256_hex(&data)
    );
    assert_eq!(std::fs::read(path).unwrap(), *data);
}
