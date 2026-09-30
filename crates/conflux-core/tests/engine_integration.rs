//! End-to-end engine tests against a local fault-injecting HTTP server (see `support`).

mod support;

use anyhow::Result;
use conflux_core::{
    DownloadCancelled, DownloadEngine, DownloadProbe, NetworkAdapter, ProgressUpdate,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::{payload, sha256_hex, RangeMode, ServerConfig, TestServer};
use tokio::sync::{mpsc, watch};

const CHUNK: u64 = 256 * 1024;
const PAYLOAD_LEN: usize = 4 * 1024 * 1024 + 12_345; // 17 chunks, last one partial
const EMPTY_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn test_engine() -> DownloadEngine {
    DownloadEngine::new(CHUNK, 4)
        .unwrap()
        .with_retry_backoff(Duration::from_millis(10))
        .with_stall_timeout(Duration::from_secs(3))
}

struct Outcome {
    result: Result<String>,
    probe: DownloadProbe,
    path: PathBuf,
    progress: Vec<ProgressUpdate>,
    elapsed: Duration,
    _dir: tempfile::TempDir,
}

async fn run_download(
    engine: &DownloadEngine,
    server: &TestServer,
    adapters: &[NetworkAdapter],
) -> Outcome {
    let dir = tempfile::tempdir().unwrap();
    let probe = engine.probe(&server.url("/files/data.bin")).await.unwrap();
    let path = dir.path().join(&probe.suggested_filename);

    let (tx, mut rx) = mpsc::channel(1024);
    let collector = tokio::spawn(async move {
        let mut all = Vec::new();
        while let Some(p) = rx.recv().await {
            all.push(p);
        }
        all
    });
    let (_cancel_tx, cancel_rx) = watch::channel(false);

    let start = Instant::now();
    let result = engine
        .download(&probe, &path, adapters, Some(tx), cancel_rx)
        .await;
    let elapsed = start.elapsed();
    let progress = collector.await.unwrap();

    Outcome {
        result,
        probe,
        path,
        progress,
        elapsed,
        _dir: dir,
    }
}

fn assert_progress_sane(progress: &[ProgressUpdate], total: u64) {
    assert!(!progress.is_empty(), "no progress updates");
    for p in progress {
        assert!(
            p.downloaded_bytes <= total,
            "downloaded {} exceeds total {}",
            p.downloaded_bytes,
            total
        );
        assert!(p.completed_chunks <= p.total_chunks);
    }
    assert_eq!(progress.last().unwrap().downloaded_bytes, total);
}

fn assert_file_matches(outcome: &Outcome, data: &[u8]) {
    let sha = outcome.result.as_ref().expect("download should succeed");
    assert_eq!(sha, &sha256_hex(data));
    let on_disk = std::fs::read(&outcome.path).unwrap();
    assert_eq!(on_disk.len(), data.len());
    assert!(on_disk == data, "file content differs");
}

fn fake_adapter(ip: &str) -> NetworkAdapter {
    // Loopback aliases flagged as non-loopback so the engine will bind to them in tests.
    NetworkAdapter {
        id: format!("test:{}", ip),
        ip: ip.parse().unwrap(),
        is_ipv4: true,
        is_loopback: false,
        enabled: true,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn normal_ranged_download_matches_sha() {
    let data = payload(PAYLOAD_LEN, 1);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.content_disposition = Some(
        "attachment; filename=\"plain.bin\"; filename*=UTF-8''..%2F..%2Fr%C3%A9sum%C3%A9 v1.bin"
            .into(),
    );
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    assert!(out.probe.supports_ranges);
    assert_eq!(out.probe.total_bytes, PAYLOAD_LEN as u64);
    assert_eq!(out.probe.suggested_filename, "résumé v1.bin");
    assert_file_matches(&out, &data);
    assert_progress_sane(&out.progress, PAYLOAD_LEN as u64);
    let last = out.progress.last().unwrap();
    assert_eq!(last.total_chunks, 17);
    assert_eq!(last.completed_chunks, 17);
    // 1 probe + 17 chunk GETs, no retries, no HEAD.
    assert_eq!(server.stats.chunk_requests.load(Ordering::SeqCst), 17);
    assert_eq!(server.stats.head_requests.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_ignoring_range_uses_single_stream() {
    let data = payload(PAYLOAD_LEN, 2);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.range_mode = RangeMode::Ignore;
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    assert!(!out.probe.supports_ranges);
    assert_eq!(out.probe.total_bytes, PAYLOAD_LEN as u64);
    assert_eq!(out.probe.suggested_filename, "data.bin");
    assert_file_matches(&out, &data);
    assert_progress_sane(&out.progress, PAYLOAD_LEN as u64);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn probe_206_but_chunks_200_fails_cleanly_and_bounded() {
    let data = payload(PAYLOAD_LEN, 3);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.range_mode = RangeMode::ProbeOnly;
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    assert!(out.probe.supports_ranges);
    let err = out
        .result
        .expect_err("must fail: server never returns 206 for chunks");
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("206"),
        "error should explain the 200-vs-206 problem: {}",
        msg
    );
    assert!(
        out.elapsed < Duration::from_secs(10),
        "took {:?}",
        out.elapsed
    );
    // Bounded work: at most 5 attempts per chunk ever requested.
    assert!(server.stats.chunk_requests.load(Ordering::SeqCst) <= 17 * 5);
    for p in &out.progress {
        assert!(p.downloaded_bytes <= PAYLOAD_LEN as u64);
    }
    assert_eq!(out.progress.last().unwrap().downloaded_bytes, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dropped_connections_are_requeued_and_recovered() {
    let data = payload(PAYLOAD_LEN, 4);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.drop_first_n_chunks = 2;
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    assert_file_matches(&out, &data);
    assert_progress_sane(&out.progress, PAYLOAD_LEN as u64);
    assert_eq!(server.stats.chunk_requests.load(Ordering::SeqCst), 17 + 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn permanent_404_on_chunks_terminates_with_error() {
    let data = payload(PAYLOAD_LEN, 5);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.chunk_status = Some(404);
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    let msg = format!("{:#}", out.result.expect_err("404 must fail"));
    assert!(msg.contains("404"), "{}", msg);
    assert!(
        out.elapsed < Duration::from_secs(10),
        "took {:?}",
        out.elapsed
    );
    assert!(server.stats.chunk_requests.load(Ordering::SeqCst) <= 17 * 5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stalled_chunks_time_out_and_recover() {
    let data = payload(PAYLOAD_LEN, 6);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.stall_first_n_chunks = 2;
    let server = TestServer::start(cfg).await;

    let engine = test_engine().with_stall_timeout(Duration::from_millis(300));
    let out = run_download(&engine, &server, &[]).await;
    assert_file_matches(&out, &data);
    assert_progress_sane(&out.progress, PAYLOAD_LEN as u64);
    assert_eq!(server.stats.chunk_requests.load(Ordering::SeqCst), 17 + 2);
    assert!(
        out.elapsed < Duration::from_secs(10),
        "took {:?}",
        out.elapsed
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn permanent_stall_terminates_with_error() {
    let data = payload(PAYLOAD_LEN, 7);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.stall_first_n_chunks = usize::MAX;
    let server = TestServer::start(cfg).await;

    let engine = test_engine().with_stall_timeout(Duration::from_millis(200));
    let out = run_download(&engine, &server, &[]).await;
    assert!(out.result.is_err());
    assert!(
        out.elapsed < Duration::from_secs(10),
        "took {:?}",
        out.elapsed
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_content_range_is_rejected() {
    let data = payload(PAYLOAD_LEN, 8);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.range_mode = RangeMode::WrongContentRange;
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    let msg = format!(
        "{:#}",
        out.result
            .expect_err("must reject mismatched Content-Range")
    );
    assert!(msg.contains("does not match"), "{}", msg);
    assert!(out.elapsed < Duration::from_secs(10));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn overlong_chunk_body_is_rejected() {
    let data = payload(PAYLOAD_LEN, 9);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.range_mode = RangeMode::Overlong;
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    let msg = format!("{:#}", out.result.expect_err("must reject overlong body"));
    assert!(msg.contains("more than"), "{}", msg);
    // The writer never grows the file past the planned size.
    assert_eq!(
        std::fs::metadata(&out.path).unwrap().len(),
        PAYLOAD_LEN as u64
    );
    for p in &out.progress {
        assert!(p.downloaded_bytes <= PAYLOAD_LEN as u64);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failing_adapter_is_dropped_and_others_finish() {
    let data = payload(PAYLOAD_LEN, 10);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.fail_peer_ip = Some("127.0.0.2".parse().unwrap());
    let server = TestServer::start(cfg).await;

    let adapters = [fake_adapter("127.0.0.2"), fake_adapter("127.0.0.1")];
    let out = run_download(&test_engine(), &server, &adapters).await;
    assert_file_matches(&out, &data);
    assert_progress_sane(&out.progress, PAYLOAD_LEN as u64);
    // Socket binding really used 127.0.0.2 (server saw that peer), and the bad adapter
    // stopped after ~3 consecutive failures (plus at most its other in-flight workers).
    let bad = server
        .stats
        .requests_by_peer_127_0_0_2
        .load(Ordering::SeqCst);
    assert!((3..=6).contains(&bad), "bad adapter requests: {}", bad);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_mid_download_stops_promptly_and_writes_nothing_after() {
    let data = payload(PAYLOAD_LEN, 11);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.write_piece = 4096;
    cfg.throttle = Some(Duration::from_millis(10)); // ~400 KB/s per connection
    let server = TestServer::start(cfg).await;

    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let probe = engine.probe(&server.url("/data.bin")).await.unwrap();
    let path = dir.path().join("cancel.bin");

    let (tx, mut rx) = mpsc::channel(1024);
    let collector = tokio::spawn(async move {
        let mut all: Vec<ProgressUpdate> = Vec::new();
        while let Some(p) = rx.recv().await {
            all.push(p);
        }
        all
    });
    let (cancel_tx, cancel_rx) = watch::channel(false);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        cancel_tx.send(true).unwrap();
        // Keep the sender alive long enough.
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let start = Instant::now();
    let result = engine
        .download(&probe, &path, &[], Some(tx), cancel_rx)
        .await;
    let elapsed = start.elapsed();

    let err = result.expect_err("must be cancelled");
    assert!(
        err.downcast_ref::<DownloadCancelled>().is_some(),
        "expected DownloadCancelled, got {:#}",
        err
    );
    assert_eq!(err.to_string(), "download cancelled");
    assert!(
        elapsed < Duration::from_millis(1500),
        "cancel took too long: {:?}",
        elapsed
    );
    // Bytes were really flowing before the cancel, and the download did not finish.
    // (In-flight partial chunks are subtracted on cancel, so the final count may be 0.)
    let progress = collector.await.unwrap();
    let peak = progress
        .iter()
        .map(|p| p.downloaded_bytes)
        .max()
        .unwrap_or(0);
    assert!(peak > 0, "no bytes downloaded before cancel");
    assert!(progress.last().unwrap().downloaded_bytes < PAYLOAD_LEN as u64);

    let snapshot = std::fs::read(&path).unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    let later = std::fs::read(&path).unwrap();
    assert!(snapshot == later, "file changed after download() returned");
}

#[tokio::test]
async fn cancel_before_start_returns_immediately() {
    let data = payload(1024, 12);
    let server = TestServer::start(ServerConfig::new(data)).await;
    let engine = test_engine();
    let probe = engine.probe(&server.url("/x.bin")).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("never.bin");
    let (_tx, rx) = watch::channel(true);
    let err = engine
        .download(&probe, &path, &[], None, rx)
        .await
        .expect_err("cancelled");
    assert!(err.downcast_ref::<DownloadCancelled>().is_some());
    assert!(!path.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zero_length_file_with_range_server_yields_empty_file() {
    let data = payload(0, 13);
    let server = TestServer::start(ServerConfig::new(data)).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    assert_eq!(out.probe.total_bytes, 0);
    assert!(!out.probe.supports_ranges);
    assert_eq!(out.result.as_deref().unwrap(), EMPTY_SHA);
    assert_eq!(std::fs::metadata(&out.path).unwrap().len(), 0);
    assert_eq!(out.progress.last().unwrap().downloaded_bytes, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zero_length_file_with_plain_server_yields_empty_file() {
    let data = payload(0, 14);
    let mut cfg = ServerConfig::new(data);
    cfg.range_mode = RangeMode::Ignore;
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    assert_eq!(out.probe.total_bytes, 0);
    assert_eq!(out.result.as_deref().unwrap(), EMPTY_SHA);
    assert_eq!(std::fs::metadata(&out.path).unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_length_single_stream() {
    let data = payload(PAYLOAD_LEN, 15);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.range_mode = RangeMode::Ignore;
    cfg.send_content_length = false;
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    assert_eq!(out.probe.total_bytes, 0);
    assert!(!out.probe.supports_ranges);
    assert_file_matches(&out, &data);
    let last = out.progress.last().unwrap();
    assert_eq!(last.downloaded_bytes, PAYLOAD_LEN as u64);
    assert_eq!(last.total_bytes, 0);
}

#[tokio::test]
async fn probe_non_success_status_bails() {
    let mut cfg = ServerConfig::new(payload(1024, 16));
    cfg.all_status = Some(404);
    let server = TestServer::start(cfg).await;
    let err = test_engine()
        .probe(&server.url("/missing.bin"))
        .await
        .expect_err("404 probe must fail");
    assert!(format!("{:#}", err).contains("404"), "{:#}", err);
    // 404 is not a reason to fall back to HEAD.
    assert_eq!(server.stats.head_requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn probe_405_falls_back_to_head() {
    let data = payload(5000, 17);
    let mut cfg = ServerConfig::new(data);
    cfg.get_status_405 = true;
    let server = TestServer::start(cfg).await;
    let probe = test_engine().probe(&server.url("/f.iso")).await.unwrap();
    assert_eq!(server.stats.head_requests.load(Ordering::SeqCst), 1);
    assert_eq!(probe.total_bytes, 5000);
    assert!(!probe.supports_ranges);
    assert_eq!(probe.suggested_filename, "f.iso");
}

#[tokio::test]
async fn probe_connection_refused_fails() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let err = test_engine()
        .probe(&format!("http://127.0.0.1:{}/x", port))
        .await
        .expect_err("connection refused must fail");
    assert!(format!("{:#}", err).contains("HEAD"), "{:#}", err);
}

/// Two adapters (3 consecutive failures each = 6 attempts budget) with one always-failing
/// chunk: the chunk must exhaust exactly 5 attempts, with exponential backoff between them,
/// and the download must fail naming that chunk.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_marked_failed_after_five_attempts_with_backoff() {
    let data = payload(1000, 18);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.chunk_status = Some(503);
    let server = TestServer::start(cfg).await;

    let engine = DownloadEngine::new(CHUNK, 1)
        .unwrap()
        .with_retry_backoff(Duration::from_millis(40));
    let adapters = [fake_adapter("127.0.0.1"), fake_adapter("127.0.0.2")];
    let out = run_download(&engine, &server, &adapters).await;

    let msg = format!("{:#}", out.result.expect_err("must fail"));
    assert!(
        msg.contains("chunk(s) [0] failed after 5 attempts"),
        "{}",
        msg
    );
    assert!(msg.contains("503"), "{}", msg);
    assert_eq!(server.stats.chunk_requests.load(Ordering::SeqCst), 5);
    // Backoff before attempts 2..5: 40 + 80 + 160 + 320 ms.
    assert!(
        out.elapsed >= Duration::from_millis(600),
        "backoff not applied: {:?}",
        out.elapsed
    );
    assert!(
        out.elapsed < Duration::from_secs(5),
        "took {:?}",
        out.elapsed
    );
}
