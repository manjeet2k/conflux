//! End-to-end engine tests against a local fault-injecting HTTP server (see `support`).

mod support;

use anyhow::Result;
use conflux_core::{
    resume_sidecar_path, AdapterUpdate, DownloadCancelled, DownloadEngine, DownloadProbe,
    NetworkAdapter, ProgressUpdate, RequestHeaders,
};
use std::path::{Path, PathBuf};
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
        // No interface name: these are loopback aliases, not real devices to pin to.
        name: String::new(),
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

    // Resume data is removed once the download is complete.
    assert!(!resume_sidecar_path(&out.path).exists());
    // The final update always carries the chunk map; the first one does too.
    assert_eq!(last.chunk_map.as_deref(), Some("#".repeat(17).as_str()));
    assert!(out.progress[0].chunk_map.is_some());
    // One unbound adapter that received exactly the payload (no retries happened).
    assert_eq!(last.adapters.len(), 1);
    assert_eq!(last.adapters[0].label, "default-route");
    assert_eq!(last.adapters[0].ip, None);
    assert_eq!(last.adapters[0].downloaded_bytes, PAYLOAD_LEN as u64);
    assert_eq!(last.adapters[0].active_connections, 0);
    assert!(!last.adapters[0].dropped);
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

/// A blip that fails every in-flight connection at once (4 > the 3-failure drop threshold)
/// must not drop the only adapter: chunks still have retries left, so the download recovers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_failures_on_only_adapter_are_retried() {
    let data = payload(PAYLOAD_LEN, 30);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.drop_first_n_chunks = 4;
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    assert_file_matches(&out, &data);
    assert_progress_sane(&out.progress, PAYLOAD_LEN as u64);
    assert_eq!(server.stats.chunk_requests.load(Ordering::SeqCst), 17 + 4);
    assert!(!out.progress.last().unwrap().adapters[0].dropped);
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

    let last = out.progress.last().unwrap();
    assert_eq!(last.adapters.len(), 2);
    let bad_stats = &last.adapters[0];
    let good_stats = &last.adapters[1];
    assert_eq!(bad_stats.label, "127.0.0.2");
    assert!(
        bad_stats.dropped,
        "failing adapter must be reported as dropped"
    );
    assert_eq!(bad_stats.downloaded_bytes, 0);
    assert!(!good_stats.dropped);
    assert_eq!(good_stats.downloaded_bytes, PAYLOAD_LEN as u64);

    // V-3: the failing adapter explains itself; the healthy one stays silent.
    let err = bad_stats.last_error.as_deref().unwrap_or("");
    assert!(!err.is_empty(), "failing adapter must report a last_error");
    assert!(
        !err.contains("http://") && !err.contains(&server.url("")),
        "last_error must not leak the URL: {err}"
    );
    let reason = bad_stats.drop_reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("consecutive failures"),
        "unexpected drop_reason: {reason:?}"
    );
    assert_eq!(good_stats.last_error, None);
    assert_eq!(good_stats.drop_reason, None);
}

/// A removed adapter reports why it stopped; a fallback retired by a proven adapter too.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn removed_adapter_reports_drop_reason() {
    let data = payload(PAYLOAD_LEN, 77);
    let server = TestServer::start(throttled_config(&data)).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let probe = engine.probe(&server.url("/removed.bin")).await.unwrap();
    let path = dir.path().join("removed.bin");

    let (progress_tx, mut progress_rx) = mpsc::channel(1024);
    let collector = tokio::spawn(async move {
        let mut all = Vec::new();
        while let Some(p) = progress_rx.recv().await {
            all.push(p);
        }
        all
    });
    let (adapter_tx, adapter_rx) = mpsc::channel(4);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = adapter_tx
            .send(AdapterUpdate::Remove("127.0.0.2".parse().unwrap()))
            .await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let result = engine
        .download_with_updates(
            &probe,
            &path,
            &[fake_adapter("127.0.0.2"), fake_adapter("127.0.0.1")],
            Some(adapter_rx),
            Some(progress_tx),
            cancel_rx,
        )
        .await;
    assert_eq!(result.unwrap(), sha256_hex(&data));
    let progress = collector.await.unwrap();
    let last = progress.last().unwrap();
    let removed = last
        .adapters
        .iter()
        .find(|a| a.label == "127.0.0.2")
        .unwrap();
    assert!(removed.dropped);
    assert_eq!(
        removed.drop_reason.as_deref(),
        Some("adapter disconnected or disabled")
    );
    let kept = last
        .adapters
        .iter()
        .find(|a| a.label == "127.0.0.1")
        .unwrap();
    assert!(!kept.dropped && kept.drop_reason.is_none());
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

// ─── Resume ──────────────────────────────────────────────────────────────────

fn throttled_config(data: &Arc<Vec<u8>>) -> ServerConfig {
    let mut cfg = ServerConfig::new(Arc::clone(data));
    cfg.write_piece = 4096;
    cfg.throttle = Some(Duration::from_millis(10)); // ~400 KB/s per connection
    cfg
}

/// Starts a download of `url` into `path` and cancels it after `after`.
/// Returns the peak `completed_chunks` seen before the cancel.
async fn cancelled_partial_download(
    engine: &DownloadEngine,
    url: &str,
    path: &Path,
    after: Duration,
) -> usize {
    let probe = engine.probe(url).await.unwrap();
    let (tx, mut rx) = mpsc::channel(1024);
    let collector = tokio::spawn(async move {
        let mut all: Vec<ProgressUpdate> = Vec::new();
        while let Some(p) = rx.recv().await {
            all.push(p);
        }
        all
    });
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(after).await;
        cancel_tx.send(true).unwrap();
        cancel_tx
    });
    let err = engine
        .download(&probe, path, &[], Some(tx), cancel_rx)
        .await
        .expect_err("must be cancelled");
    assert!(
        err.downcast_ref::<DownloadCancelled>().is_some(),
        "{:#}",
        err
    );
    drop(canceller.await.unwrap());
    let progress = collector.await.unwrap();
    progress.last().map(|p| p.completed_chunks).unwrap_or(0)
}

async fn resume_to_end(
    engine: &DownloadEngine,
    url: &str,
    path: &Path,
) -> (Result<String>, Vec<ProgressUpdate>) {
    let probe = engine.probe(url).await.unwrap();
    let (tx, mut rx) = mpsc::channel(1024);
    let collector = tokio::spawn(async move {
        let mut all = Vec::new();
        while let Some(p) = rx.recv().await {
            all.push(p);
        }
        all
    });
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let result = engine.resume(&probe, path, &[], Some(tx), cancel_rx).await;
    (result, collector.await.unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_then_resume_continues_from_completed_chunks() {
    let data = payload(PAYLOAD_LEN, 20);
    let slow = TestServer::start(throttled_config(&data)).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resume.bin");

    let done_before = cancelled_partial_download(
        &engine,
        &slow.url("/resume.bin"),
        &path,
        Duration::from_millis(1500),
    )
    .await;
    assert!(done_before > 0, "no chunk finished before the pause");
    assert!(done_before < 17, "download finished before the pause");
    let sidecar = resume_sidecar_path(&path);
    assert!(sidecar.exists(), "cancel must leave a resume sidecar");

    // Resume against a fast server with the same content; count what it has to send.
    let fast = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let (result, progress) = resume_to_end(&engine, &fast.url("/resume.bin"), &path).await;
    assert_eq!(result.expect("resume should succeed"), sha256_hex(&data));
    assert!(
        std::fs::read(&path).unwrap() == *data,
        "file content differs"
    );
    assert!(!sidecar.exists(), "sidecar must be removed on success");

    let fetched = fast.stats.chunk_requests.load(Ordering::SeqCst);
    assert_eq!(fetched, 17 - done_before, "only missing chunks are fetched");
    let sent = fast.stats.body_bytes_sent.load(Ordering::SeqCst);
    assert!(
        sent < PAYLOAD_LEN as u64,
        "resume re-downloaded everything ({} bytes)",
        sent
    );
    // Resumed bytes are counted from the first update, and accounting still ends exact.
    assert!(progress[0].downloaded_bytes >= done_before as u64 * CHUNK);
    assert!(progress[0].completed_chunks >= done_before);
    assert_progress_sane(&progress, PAYLOAD_LEN as u64);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_with_changed_etag_restarts_from_zero() {
    let data = payload(PAYLOAD_LEN, 21);
    let mut cfg = throttled_config(&data);
    cfg.etag = Some("\"v1\"".into());
    let old = TestServer::start(cfg).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("etag.bin");
    let done = cancelled_partial_download(
        &engine,
        &old.url("/etag.bin"),
        &path,
        Duration::from_millis(1500),
    )
    .await;
    assert!(done > 0);

    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.etag = Some("\"v2\"".into());
    let new = TestServer::start(cfg).await;
    let (result, _) = resume_to_end(&engine, &new.url("/etag.bin"), &path).await;
    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert_eq!(new.stats.chunk_requests.load(Ordering::SeqCst), 17);
    assert!(!resume_sidecar_path(&path).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_with_changed_size_restarts_from_zero() {
    let data = payload(PAYLOAD_LEN, 22);
    let old = TestServer::start(throttled_config(&data)).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("size.bin");
    let done = cancelled_partial_download(
        &engine,
        &old.url("/size.bin"),
        &path,
        Duration::from_millis(1500),
    )
    .await;
    assert!(done > 0);

    let bigger = payload(PAYLOAD_LEN + CHUNK as usize, 23);
    let new = TestServer::start(ServerConfig::new(Arc::clone(&bigger))).await;
    let (result, _) = resume_to_end(&engine, &new.url("/size.bin"), &path).await;
    assert_eq!(result.unwrap(), sha256_hex(&bigger));
    assert!(std::fs::read(&path).unwrap() == *bigger);
    assert_eq!(new.stats.chunk_requests.load(Ordering::SeqCst), 18);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_with_corrupt_sidecar_or_missing_file_starts_fresh() {
    let data = payload(PAYLOAD_LEN, 24);
    let server = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();

    // No sidecar and no file at all.
    let path = dir.path().join("fresh.bin");
    let (result, _) = resume_to_end(&engine, &server.url("/fresh.bin"), &path).await;
    assert_eq!(result.unwrap(), sha256_hex(&data));

    // Garbage sidecar next to a wrong-length file: both ignored, full download.
    let path = dir.path().join("corrupt.bin");
    std::fs::write(&path, b"short").unwrap();
    std::fs::write(resume_sidecar_path(&path), b"{not json").unwrap();
    let (result, _) = resume_to_end(&engine, &server.url("/corrupt.bin"), &path).await;
    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert!(std::fs::read(&path).unwrap() == *data);
    assert!(!resume_sidecar_path(&path).exists());
    assert_eq!(server.stats.chunk_requests.load(Ordering::SeqCst), 34);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn single_stream_resume_preserves_partial_file_and_aborts_on_range_loss() {
    let data = payload(PAYLOAD_LEN, 25);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.range_mode = RangeMode::Ignore;
    let server = TestServer::start(cfg).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("single.bin");
    // Pre-create partial file and sidecar
    std::fs::write(&path, b"partial data").unwrap();
    std::fs::write(resume_sidecar_path(&path), b"{\"version\":1}").unwrap();

    let (result, _) = resume_to_end(&engine, &server.url("/single.bin"), &path).await;
    let err = result.expect_err("resuming when server reports no ranges must abort with error");
    assert!(
        err.to_string().contains("partial download preserved"),
        "error must explicitly report that partial data was preserved: {}",
        err
    );
    // Partial file and sidecar must NOT be truncated or removed
    assert_eq!(std::fs::read(&path).unwrap(), b"partial data");
    assert!(resume_sidecar_path(&path).exists());
}

#[tokio::test]
async fn probe_captures_etag() {
    let mut cfg = ServerConfig::new(payload(1024, 26));
    cfg.etag = Some("\"abc\"".into());
    let server = TestServer::start(cfg).await;
    let probe = test_engine().probe(&server.url("/e.bin")).await.unwrap();
    assert_eq!(probe.etag.as_deref(), Some("\"abc\""));
    assert_eq!(probe.last_modified, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dynamic_adapter_joins_mid_download_and_aggregates_bandwidth() {
    let data = payload(PAYLOAD_LEN, 27);
    let mut config = ServerConfig::new(Arc::clone(&data));
    config.throttle = Some(Duration::from_millis(10));
    let server = TestServer::start(config).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let probe = engine.probe(&server.url("/dynamic.bin")).await.unwrap();
    let path = dir.path().join(&probe.suggested_filename);

    let (progress_tx, mut progress_rx) = mpsc::channel(1024);
    let collector = tokio::spawn(async move {
        let mut all = Vec::new();
        while let Some(p) = progress_rx.recv().await {
            all.push(p);
        }
        all
    });
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let (adapter_tx, adapter_rx) = mpsc::channel(16);

    let initial_adapters: Vec<NetworkAdapter> = Vec::new();

    let adapter_tx_clone = adapter_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let new_adapter = NetworkAdapter {
            id: "eth_hotplug:127.0.0.1".into(),
            name: String::new(),
            ip: "127.0.0.1".parse().unwrap(),
            is_ipv4: true,
            is_loopback: false,
            enabled: true,
        };
        let _ = adapter_tx_clone.send(AdapterUpdate::Add(new_adapter)).await;
    });

    let result = engine
        .download_with_updates(
            &probe,
            &path,
            &initial_adapters,
            Some(adapter_rx),
            Some(progress_tx),
            cancel_rx,
        )
        .await;

    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert!(std::fs::read(&path).unwrap() == *data);

    let progress = collector.await.unwrap();
    assert!(!progress.is_empty());
    let last = progress.last().unwrap();
    assert!(last.adapters.iter().any(|a| a.label == "127.0.0.1"));
    // The default route is retired once the joined adapter has proven itself.
    let default_route = last
        .adapters
        .iter()
        .find(|a| a.label == "default-route")
        .unwrap();
    assert!(default_route.dropped);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closed_adapter_updates_do_not_block_completion() {
    let data = payload(PAYLOAD_LEN, 28);
    let server = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let probe = engine
        .probe(&server.url("/closed-updates.bin"))
        .await
        .unwrap();
    let path = dir.path().join(&probe.suggested_filename);
    let (adapter_tx, adapter_rx) = mpsc::channel(1);
    drop(adapter_tx);
    let (_cancel_tx, cancel_rx) = watch::channel(false);

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        engine.download_with_updates(&probe, &path, &[], Some(adapter_rx), None, cancel_rx),
    )
    .await
    .expect("closed adapter channel must not spin or hang")
    .unwrap();

    assert_eq!(result, sha256_hex(&data));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn all_adapters_removed_falls_back_to_default_route_and_completes() {
    let data = payload(PAYLOAD_LEN, 29);
    let mut config = ServerConfig::new(Arc::clone(&data));
    config.throttle = Some(Duration::from_millis(10));
    let server = TestServer::start(config).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let probe = engine.probe(&server.url("/fallback.bin")).await.unwrap();
    let path = dir.path().join(&probe.suggested_filename);

    let (progress_tx, mut progress_rx) = mpsc::channel(1024);
    let collector = tokio::spawn(async move {
        let mut all = Vec::new();
        while let Some(p) = progress_rx.recv().await {
            all.push(p);
        }
        all
    });
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let (adapter_tx, adapter_rx) = mpsc::channel(16);

    let loopback_adapter = NetworkAdapter {
        id: "eth0:127.0.0.1".into(),
        name: String::new(),
        ip: "127.0.0.1".parse().unwrap(),
        is_ipv4: true,
        is_loopback: false,
        enabled: true,
    };
    let initial_adapters = vec![loopback_adapter];

    let adapter_tx_clone = adapter_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Drop the only active adapter mid-download
        let _ = adapter_tx_clone
            .send(AdapterUpdate::Remove("127.0.0.1".parse().unwrap()))
            .await;
    });

    let result = engine
        .download_with_updates(
            &probe,
            &path,
            &initial_adapters,
            Some(adapter_rx),
            Some(progress_tx),
            cancel_rx,
        )
        .await;

    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert!(std::fs::read(&path).unwrap() == *data);

    let progress = collector.await.unwrap();
    assert!(!progress.is_empty());
    let last = progress.last().unwrap();
    // Default-route fallback was spun up and completed remaining chunks
    assert!(last.adapters.iter().any(|a| a.label == "default-route"));
}

// ─── Interface pinning ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn named_adapter(ip: &str, name: &str) -> NetworkAdapter {
    NetworkAdapter {
        name: name.to_string(),
        ..fake_adapter(ip)
    }
}

/// The adapter's interface name must reach the socket (SO_BINDTODEVICE on Linux): an
/// adapter pinned to a nonexistent device can never connect, so the server never sees its
/// IP, both for initial adapters and for hot-added ones. Unnamed adapters keep working.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adapter_interface_name_is_used_for_binding() {
    let data = payload(PAYLOAD_LEN, 40);
    let server = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;

    let adapters = [
        named_adapter("127.0.0.2", "conflux-nodev0"),
        fake_adapter("127.0.0.1"),
    ];
    let out = run_download(&test_engine(), &server, &adapters).await;
    assert_file_matches(&out, &data);
    assert_eq!(
        server
            .stats
            .requests_by_peer_127_0_0_2
            .load(Ordering::SeqCst),
        0,
        "adapter pinned to a missing device must not reach the server"
    );
    assert!(out.progress.last().unwrap().adapters[0].dropped);

    // Hot-added adapter: same expectation.
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let probe = engine.probe(&server.url("/hot.bin")).await.unwrap();
    let path = dir.path().join("hot.bin");
    let (adapter_tx, adapter_rx) = mpsc::channel(4);
    adapter_tx
        .send(AdapterUpdate::Add(named_adapter(
            "127.0.0.2",
            "conflux-nodev0",
        )))
        .await
        .unwrap();
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let result = engine
        .download_with_updates(
            &probe,
            &path,
            &[fake_adapter("127.0.0.1")],
            Some(adapter_rx),
            None,
            cancel_rx,
        )
        .await;
    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert_eq!(
        server
            .stats
            .requests_by_peer_127_0_0_2
            .load(Ordering::SeqCst),
        0
    );
}

// ─── Shutdown hygiene ────────────────────────────────────────────────────────

/// Paths of every file this process currently has open.
#[cfg(target_os = "linux")]
fn open_file_paths() -> Vec<PathBuf> {
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
        .collect()
}

/// Dropping the `download()` future (what `JoinHandle::abort` does) must stop every
/// background task: none may keep the output file open or keep running.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn aborted_download_releases_output_file() {
    let data = payload(PAYLOAD_LEN, 41);
    let server = TestServer::start(throttled_config(&data)).await;
    let engine = Arc::new(test_engine());
    let dir = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(dir.path()).unwrap().join("abort.bin");
    let probe = engine.probe(&server.url("/abort.bin")).await.unwrap();

    // The progress receiver stays alive, so the reporter cannot exit via a closed channel.
    let (tx, _rx) = mpsc::channel(1024);
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let task = {
        let (engine, path) = (Arc::clone(&engine), path.clone());
        tokio::spawn(async move {
            engine
                .download(&probe, &path, &[], Some(tx), cancel_rx)
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        open_file_paths().contains(&path),
        "download should hold the file"
    );
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());

    let deadline = Instant::now() + Duration::from_secs(3);
    while open_file_paths().contains(&path) {
        assert!(
            Instant::now() < deadline,
            "output file still open after the download future was dropped"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Cancel guarantees no write is in flight when `download()` returns, so a fresh download
/// started immediately into the same path must end byte-exact.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_then_immediate_restart_is_byte_exact() {
    let data = payload(PAYLOAD_LEN, 42);
    let slow = TestServer::start(throttled_config(&data)).await;
    let fast = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let engine = DownloadEngine::new(CHUNK, 8)
        .unwrap()
        .with_retry_backoff(Duration::from_millis(10));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("restart.bin");

    for _ in 0..3 {
        cancelled_partial_download(
            &engine,
            &slow.url("/restart.bin"),
            &path,
            Duration::from_millis(300),
        )
        .await;
        let probe = engine.probe(&fast.url("/restart.bin")).await.unwrap();
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let sha = engine
            .download(&probe, &path, &[], None, cancel_rx)
            .await
            .unwrap();
        assert_eq!(sha, sha256_hex(&data));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            std::fs::read(&path).unwrap() == *data,
            "a write of the cancelled download landed in the new file"
        );
    }
}

// ─── Resource change detection ───────────────────────────────────────────────

fn if_range_values(server: &TestServer) -> Vec<String> {
    server.stats.if_range_values.lock().unwrap().clone()
}

const LAST_MODIFIED: &str = "Wed, 21 Oct 2015 07:28:00 GMT";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_requests_carry_if_range_validator() {
    let data = payload(PAYLOAD_LEN, 43);

    // Strong ETag wins over Last-Modified.
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.etag = Some("\"v1\"".into());
    cfg.last_modified = Some(LAST_MODIFIED.into());
    let server = TestServer::start(cfg).await;
    let out = run_download(&test_engine(), &server, &[]).await;
    assert_file_matches(&out, &data);
    assert_eq!(out.probe.last_modified.as_deref(), Some(LAST_MODIFIED));
    assert_eq!(if_range_values(&server), vec!["\"v1\"".to_string(); 17]);

    // Weak ETags are not allowed in If-Range: use Last-Modified instead.
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.etag = Some("W/\"v1\"".into());
    cfg.last_modified = Some(LAST_MODIFIED.into());
    let server = TestServer::start(cfg).await;
    let out = run_download(&test_engine(), &server, &[]).await;
    assert_file_matches(&out, &data);
    assert_eq!(
        if_range_values(&server),
        vec![LAST_MODIFIED.to_string(); 17]
    );

    // Weak ETag only: no usable validator, no If-Range.
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.etag = Some("W/\"v1\"".into());
    let server = TestServer::start(cfg).await;
    let out = run_download(&test_engine(), &server, &[]).await;
    assert_file_matches(&out, &data);
    assert!(if_range_values(&server).is_empty());
}

/// Serves `data` with ETag "v1", then (from the 5th chunk request on) a different payload
/// of the same size with validator `new_etag`.
fn changing_config(data: &Arc<Vec<u8>>, new_etag: Option<&str>, honor: bool) -> ServerConfig {
    let mut cfg = ServerConfig::new(Arc::clone(data));
    cfg.etag = Some("\"v1\"".into());
    cfg.honor_if_range = honor;
    cfg.content_change = Some(support::ContentChange {
        after_chunks: 4,
        payload: payload(data.len(), 999),
        etag: new_etag.map(str::to_string),
        last_modified: None,
    });
    cfg
}

fn assert_failed_as_changed(out: &Outcome, server: &TestServer) {
    let msg = format!(
        "{:#}",
        out.result
            .as_ref()
            .expect_err("a changed resource must fail the download")
    );
    assert!(msg.contains("changed on the server"), "{}", msg);
    // Fails on the first evidence instead of retrying every chunk 5 times.
    let requests = server.stats.chunk_requests.load(Ordering::SeqCst);
    assert!(requests < 17, "kept requesting chunks: {}", requests);
    assert!(
        out.elapsed < Duration::from_secs(5),
        "took {:?}",
        out.elapsed
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_size_change_mid_download_fails_via_if_range() {
    let data = payload(PAYLOAD_LEN, 44);
    let server = TestServer::start(changing_config(&data, Some("\"v2\""), true)).await;
    let out = run_download(&test_engine(), &server, &[]).await;
    assert_failed_as_changed(&out, &server);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_size_change_mid_download_fails_via_etag_compare() {
    let data = payload(PAYLOAD_LEN, 45);
    // The server ignores If-Range and answers 206 with the new ETag.
    let server = TestServer::start(changing_config(&data, Some("\"v2\""), false)).await;
    let out = run_download(&test_engine(), &server, &[]).await;
    assert_failed_as_changed(&out, &server);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn etag_disappearing_mid_download_fails() {
    let data = payload(PAYLOAD_LEN, 46);
    for honor in [true, false] {
        let server = TestServer::start(changing_config(&data, None, honor)).await;
        let out = run_download(&test_engine(), &server, &[]).await;
        assert_failed_as_changed(&out, &server);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_with_disappeared_etag_restarts_from_zero() {
    let data = payload(PAYLOAD_LEN, 47);
    let mut cfg = throttled_config(&data);
    cfg.etag = Some("\"v1\"".into());
    let old = TestServer::start(cfg).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gone.bin");
    let done = cancelled_partial_download(
        &engine,
        &old.url("/gone.bin"),
        &path,
        Duration::from_millis(1500),
    )
    .await;
    assert!(done > 0);

    let new = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let (result, _) = resume_to_end(&engine, &new.url("/gone.bin"), &path).await;
    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert_eq!(new.stats.chunk_requests.load(Ordering::SeqCst), 17);
}

// ─── Dynamic adapters ────────────────────────────────────────────────────────

/// A hot-added adapter that never works must not retire the working default route.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failing_hot_added_adapter_keeps_default_route() {
    let data = payload(PAYLOAD_LEN, 48);
    let mut cfg = throttled_config(&data);
    cfg.fail_peer_ip = Some("127.0.0.2".parse().unwrap());
    let server = TestServer::start(cfg).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let probe = engine.probe(&server.url("/bad-add.bin")).await.unwrap();
    let path = dir.path().join("bad-add.bin");

    let (progress_tx, mut progress_rx) = mpsc::channel(1024);
    let collector = tokio::spawn(async move {
        let mut all = Vec::new();
        while let Some(p) = progress_rx.recv().await {
            all.push(p);
        }
        all
    });
    let (adapter_tx, adapter_rx) = mpsc::channel(4);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = adapter_tx
            .send(AdapterUpdate::Add(fake_adapter("127.0.0.2")))
            .await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let result = engine
        .download_with_updates(
            &probe,
            &path,
            &[],
            Some(adapter_rx),
            Some(progress_tx),
            cancel_rx,
        )
        .await;
    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert!(
        server
            .stats
            .requests_by_peer_127_0_0_2
            .load(Ordering::SeqCst)
            > 0
    );

    let progress = collector.await.unwrap();
    let last = progress.last().unwrap();
    let default_route = last
        .adapters
        .iter()
        .find(|a| a.label == "default-route")
        .unwrap();
    assert!(!default_route.dropped, "working default route was retired");
    let bad = last
        .adapters
        .iter()
        .find(|a| a.label == "127.0.0.2")
        .unwrap();
    assert!(bad.dropped);
}

/// Removing and re-adding the same adapter creates a second stats entry with the same
/// label; the two must not share speed baselines. The old (dropped, idle) entry receives
/// no bytes, so its displayed speed may only decay.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn readded_adapter_keeps_separate_speed_stats() {
    let data = payload(PAYLOAD_LEN, 49);
    let server = TestServer::start(throttled_config(&data)).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let probe = engine.probe(&server.url("/readd.bin")).await.unwrap();
    let path = dir.path().join("readd.bin");

    let (progress_tx, mut progress_rx) = mpsc::channel(4096);
    let collector = tokio::spawn(async move {
        let mut all = Vec::new();
        while let Some(p) = progress_rx.recv().await {
            all.push(p);
        }
        all
    });
    let (adapter_tx, adapter_rx) = mpsc::channel(4);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let ip = "127.0.0.1".parse().unwrap();
        let _ = adapter_tx.send(AdapterUpdate::Remove(ip)).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = adapter_tx
            .send(AdapterUpdate::Add(fake_adapter("127.0.0.1")))
            .await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let result = engine
        .download_with_updates(
            &probe,
            &path,
            &[fake_adapter("127.0.0.1")],
            Some(adapter_rx),
            Some(progress_tx),
            cancel_rx,
        )
        .await;
    assert_eq!(result.unwrap(), sha256_hex(&data));

    let progress = collector.await.unwrap();
    let readded = progress
        .iter()
        .filter(|p| p.adapters.iter().filter(|a| a.label == "127.0.0.1").count() == 2)
        .count();
    assert!(readded > 2, "re-added adapter was not observed running");
    for pair in progress.windows(2) {
        let (a, b) = (&pair[0].adapters[0], &pair[1].adapters[0]);
        if a.dropped && a.active_connections == 0 && a.downloaded_bytes == b.downloaded_bytes {
            assert!(
                b.speed_bytes_sec <= a.speed_bytes_sec + 1e-6,
                "idle dropped adapter speed rose from {} to {}",
                a.speed_bytes_sec,
                b.speed_bytes_sec
            );
        }
    }
}

// ─── Filenames ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn non_ascii_content_disposition_keeps_filename_star() {
    let mut cfg = ServerConfig::new(payload(1024, 50));
    cfg.content_disposition =
        Some("attachment; filename=\"résumé.pdf\"; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf".into());
    let server = TestServer::start(cfg).await;
    let probe = test_engine().probe(&server.url("/dl?id=7")).await.unwrap();
    assert_eq!(probe.suggested_filename, "résumé.pdf");
}

// ─── Finished-download safety (hash phase) ───────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_during_hashing_keeps_sidecar_and_resume_does_not_redownload() {
    let data = payload(PAYLOAD_LEN, 60);
    let server = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    // Slow hashing so the cancel reliably lands inside the hash phase.
    let engine = test_engine().with_hash_throttle(Duration::from_millis(20));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hashing.bin");
    let probe = engine.probe(&server.url("/hashing.bin")).await.unwrap();

    let (tx, mut rx) = mpsc::channel::<ProgressUpdate>(1024);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let watcher = tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            if p.total_chunks > 0 && p.completed_chunks == p.total_chunks {
                tokio::time::sleep(Duration::from_millis(150)).await;
                cancel_tx.send(true).unwrap();
                break;
            }
        }
        // Keep draining so the engine never blocks on the channel.
        while rx.recv().await.is_some() {}
    });
    let started = Instant::now();
    let err = engine
        .download(&probe, &path, &[], Some(tx), cancel_rx)
        .await
        .expect_err("cancel during hashing must abort");
    assert!(err.downcast_ref::<DownloadCancelled>().is_some(), "{err:#}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancel did not interrupt hashing"
    );
    watcher.await.unwrap();
    assert_eq!(server.stats.chunk_requests.load(Ordering::SeqCst), 17);

    let sidecar = resume_sidecar_path(&path);
    let json = std::fs::read_to_string(&sidecar).expect("sidecar must survive a hashing cancel");
    assert!(json.contains("\"completed\":\"0-16\""), "{json}");
    assert!(
        std::fs::read(&path).unwrap() == *data,
        "file must be intact"
    );

    let fast = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let (result, _) = resume_to_end(&test_engine(), &fast.url("/hashing.bin"), &path).await;
    assert_eq!(result.expect("resume must succeed"), sha256_hex(&data));
    assert_eq!(
        fast.stats.chunk_requests.load(Ordering::SeqCst),
        0,
        "a fully downloaded file must not be fetched again"
    );
    assert!(std::fs::read(&path).unwrap() == *data, "file byte-exact");
    assert!(!sidecar.exists(), "sidecar removed after the hash");
}

// ─── Hostile sizes ───────────────────────────────────────────────────────────

#[tokio::test]
async fn hostile_probe_total_is_rejected_without_allocating() {
    let mut cfg = ServerConfig::new(payload(1024, 61));
    cfg.probe_total_override = Some(1 << 60);
    let server = TestServer::start(cfg).await;
    let err = test_engine()
        .probe(&server.url("/huge.bin"))
        .await
        .expect_err("absurd size must be refused");
    assert!(format!("{err:#}").contains("too many chunks"), "{err:#}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hostile_download_probe_is_refused_by_download() {
    let server = TestServer::start(ServerConfig::new(payload(1024, 62))).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.bin");
    let probe = DownloadProbe {
        url: server.url("/huge.bin"),
        total_bytes: 1 << 60,
        supports_ranges: true,
        suggested_filename: "huge.bin".into(),
        etag: None,
        last_modified: None,
        headers: None,
    };
    let (_tx, cancel_rx) = watch::channel(false);
    let err = test_engine()
        .download(&probe, &path, &[], None, cancel_rx)
        .await
        .expect_err("must refuse");
    assert!(format!("{err:#}").contains("too many chunks"), "{err:#}");
    assert!(!path.exists(), "nothing may be created");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hostile_sidecar_chunk_size_falls_back_to_fresh_start() {
    let data = payload(PAYLOAD_LEN, 63);
    let server = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hostile.bin");
    std::fs::write(&path, vec![0u8; PAYLOAD_LEN]).unwrap();
    // chunk_size 1 on a 4 MiB file => ~4.2M chunks, above the cap.
    std::fs::write(
        resume_sidecar_path(&path),
        format!(
            "{{\"version\":1,\"total_bytes\":{PAYLOAD_LEN},\"chunk_size\":1,\"etag\":null,\"last_modified\":null,\"completed\":\"\"}}"
        ),
    )
    .unwrap();
    let (result, _) = resume_to_end(&test_engine(), &server.url("/hostile.bin"), &path).await;
    assert_eq!(result.unwrap(), sha256_hex(&data));
    assert_eq!(server.stats.chunk_requests.load(Ordering::SeqCst), 17);
    assert!(std::fs::read(&path).unwrap() == *data);
}

// ─── Chunk status handling ───────────────────────────────────────────────────

async fn assert_chunk_status_fails(code: u16, seed: u64) {
    let data = payload(PAYLOAD_LEN, seed);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.chunk_status = Some(code);
    let server = TestServer::start(cfg).await;
    let out = run_download(&test_engine(), &server, &[]).await;
    let msg = format!("{:#}", out.result.expect_err("must fail, not complete"));
    assert!(msg.contains(&code.to_string()), "{msg}");
    // Bounded retries, and the sidecar must not claim any chunk finished.
    assert!(server.stats.chunk_requests.load(Ordering::SeqCst) <= 17 * 5);
    let json = std::fs::read_to_string(resume_sidecar_path(&out.path)).unwrap_or_default();
    assert!(
        json.is_empty() || json.contains("\"completed\":\"\""),
        "{json}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_416_is_a_failure_not_completion() {
    assert_chunk_status_fails(416, 64).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_redirect_without_location_is_a_failure() {
    assert_chunk_status_fails(302, 65).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn download_carries_request_headers_on_probe_and_chunks() {
    let data = payload(PAYLOAD_LEN, 66);
    let server = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let engine = test_engine();

    let custom_headers = RequestHeaders {
        cookie: Some("auth_session=secret_token_123".into()),
        referer: Some("https://portal.example.com/downloads".into()),
        user_agent: Some("ConfluxBrowserBridge/1.0".into()),
    };

    let dir = tempfile::tempdir().unwrap();
    let probe = engine
        .probe_with_headers(
            &server.url("/secure/file.bin"),
            Some(custom_headers.clone()),
        )
        .await
        .expect("probe must succeed");

    assert_eq!(probe.headers, Some(custom_headers));
    let path = dir.path().join(&probe.suggested_filename);
    let (_tx, cancel_rx) = watch::channel(false);

    let sha = engine
        .download(&probe, &path, &[], None, cancel_rx)
        .await
        .expect("download must succeed");

    assert_eq!(sha, sha256_hex(&data));

    // Verify all requests received by the server carried the expected headers
    let cookies = server.stats.cookies.lock().unwrap().clone();
    let referers = server.stats.referers.lock().unwrap().clone();
    let user_agents = server.stats.user_agents.lock().unwrap().clone();

    // 1 probe + 17 chunk requests = 18 total requests
    assert_eq!(cookies.len(), 18);
    assert!(cookies.iter().all(|c| c == "auth_session=secret_token_123"));
    assert_eq!(referers.len(), 18);
    assert!(referers
        .iter()
        .all(|r| r == "https://portal.example.com/downloads"));
    assert_eq!(user_agents.len(), 18);
    assert!(user_agents
        .iter()
        .all(|ua| ua == "ConfluxBrowserBridge/1.0"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sidecar_never_persists_cookies_to_disk() {
    let data = payload(PAYLOAD_LEN, 67);
    let slow = TestServer::start(throttled_config(&data)).await;
    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secure_resume.bin");

    let custom_headers = RequestHeaders {
        cookie: Some("session=ultra_secret_cookie_token".into()),
        referer: Some("https://auth.example.com/vault".into()),
        user_agent: Some("ConfluxTest/1.0".into()),
    };

    let probe = engine
        .probe_with_headers(&slow.url("/secure_resume.bin"), Some(custom_headers))
        .await
        .unwrap();

    let (cancel_tx, cancel_rx) = watch::channel(false);
    let path_clone = path.clone();
    let engine_clone = test_engine();
    let probe_clone = probe.clone();
    let download_handle = tokio::spawn(async move {
        engine_clone
            .download(&probe_clone, &path_clone, &[], None, cancel_rx)
            .await
    });

    // Let at least one chunk complete and trigger sidecar persister
    tokio::time::sleep(Duration::from_millis(500)).await;
    cancel_tx.send_replace(true);
    let _ = download_handle.await;

    let sidecar_path = resume_sidecar_path(&path);
    assert!(
        sidecar_path.exists(),
        "paused download must leave a sidecar"
    );
    let content = std::fs::read_to_string(&sidecar_path).unwrap();
    assert!(
        !content.contains("ultra_secret_cookie_token"),
        "cookies must NEVER be written to the sidecar JSON"
    );
    assert!(
        content.contains("https://auth.example.com/vault"),
        "non-sensitive headers (referer) may be retained"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn probe_records_final_url_and_chunks_target_final_endpoint() {
    let data = payload(PAYLOAD_LEN, 68);
    let target_server = TestServer::start(ServerConfig::new(Arc::clone(&data))).await;
    let target_url = target_server.url("/data.bin");

    let mut redirect_cfg = ServerConfig::new(Arc::clone(&data));
    redirect_cfg.redirect_to = Some(target_url.clone());
    let redirect_server = TestServer::start(redirect_cfg).await;

    let engine = test_engine();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("final_url_test.bin");

    let custom_headers = RequestHeaders {
        cookie: Some("origin_secret_cookie".into()),
        referer: Some("https://example.com/landing".into()),
        user_agent: Some("ConfluxTest/1.0".into()),
    };

    let probe = engine
        .probe_with_headers(
            &redirect_server.url("/start_download"),
            Some(custom_headers),
        )
        .await
        .expect("probe following redirect must succeed");

    // Probe URL must be the final redirected target, not the initial redirector URL
    assert_eq!(probe.url, target_url);

    // Cross-origin redirect must strip cookies to prevent leaking credentials to foreign targets
    assert_eq!(
        probe.headers.as_ref().and_then(|h| h.cookie.as_ref()),
        None,
        "cookies must be stripped on cross-origin redirects"
    );

    let (_tx, cancel_rx) = watch::channel(false);
    let sha = engine
        .download(&probe, &path, &[], None, cancel_rx)
        .await
        .expect("download directly from target must succeed");
    assert_eq!(sha, sha256_hex(&data));

    // Target server must have served all chunks
    assert_eq!(
        target_server.stats.chunk_requests.load(Ordering::SeqCst),
        17
    );
    // Redirector server must NOT have been re-queried for chunks
    assert_eq!(
        redirect_server.stats.chunk_requests.load(Ordering::SeqCst),
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn content_range_total_size_mismatch_fails_via_resource_changed() {
    let data = payload(PAYLOAD_LEN, 88);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    // Chunk responses report a different total than the probe saw
    cfg.chunk_total_override = Some(PAYLOAD_LEN as u64 + 5000);
    let server = TestServer::start(cfg).await;

    let out = run_download(&test_engine(), &server, &[]).await;
    let msg = format!(
        "{:#}",
        out.result
            .expect_err("must fail immediately when Content-Range total size changes")
    );
    assert!(
        msg.contains("the file changed on the server during the download"),
        "error message should indicate resource changed: {}",
        msg
    );
    assert!(
        msg.contains("remote size changed"),
        "error message should explain size mismatch: {}",
        msg
    );
    // Must fail promptly on first detection without burning 5 retries per worker
    let requests = server.stats.chunk_requests.load(Ordering::SeqCst);
    assert!(
        requests <= 4,
        "must abort immediately rather than retrying mismatched chunks: {}",
        requests
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dropped_adapter_retire_cancels_sibling_workers_promptly() {
    let data = payload(PAYLOAD_LEN, 89);
    let mut cfg = ServerConfig::new(Arc::clone(&data));
    cfg.fail_peer_ip = Some("127.0.0.2".parse().unwrap());
    // 127.0.0.2 stalls on its first chunk request, while subsequent requests fail with 503
    cfg.stall_peer_first_n_chunks = 1;
    let server = TestServer::start(cfg).await;

    // Use a long 15-second stall timeout: if sibling workers were NOT retired promptly,
    // the stalled chunk would take >= 15 seconds to time out before another adapter could finish it.
    let engine = DownloadEngine::new(CHUNK, 4)
        .unwrap()
        .with_retry_backoff(Duration::from_millis(10))
        .with_stall_timeout(Duration::from_secs(15));
    let adapters = [fake_adapter("127.0.0.2"), fake_adapter("127.0.0.1")];

    let start = std::time::Instant::now();
    let out = run_download(&engine, &server, &adapters).await;
    let elapsed = start.elapsed();

    assert_file_matches(&out, &data);
    // Sibling worker on 127.0.0.2 must be cancelled promptly via retire_tx,
    // so the stalled chunk is claimed and finished by 127.0.0.1 well under 15s.
    assert!(
        elapsed < Duration::from_secs(5),
        "download should complete quickly via retire_tx without waiting for 15s stall timeout (took {:?})",
        elapsed
    );
}
