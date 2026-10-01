use crate::adapter::{build_bound_http_client, NetworkAdapter, DEFAULT_STALL_TIMEOUT};
use crate::checksum::compute_sha256_cancellable;
use crate::chunk::{
    chunk_count, try_plan_chunks, Chunk, ChunkScheduler, ChunkStatus, Claim, FailOutcome,
};
use crate::filename::derive_filename;
use crate::resume::{
    decode_ranges, encode_ranges, read_sidecar, remove_resume_sidecar, resume_sidecar_path,
    validate, write_sidecar, ResumeState, SIDECAR_VERSION,
};
use crate::writer::SparseFileWriter;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use reqwest::{header, StatusCode};
use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch, Notify};
use tokio::task::{JoinHandle, JoinSet};
use tracing::{debug, error, info, warn};

pub use crate::chunk::MAX_PLANNED_CHUNKS;

/// Maximum attempts per chunk before it is marked `Failed`.
pub const MAX_CHUNK_ATTEMPTS: u32 = 5;

/// An adapter is dropped for the rest of a download after this many consecutive failed
/// attempts (with no successful chunk in between), unless it is the last usable adapter.
pub const MAX_CONSECUTIVE_ADAPTER_FAILURES: u32 = 3;

/// Default base for the exponential retry backoff (0.5s, 1s, 2s, 4s, 8s).
pub const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_millis(500);

/// Upper bound on how long the probe may take to receive response headers.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// Interval between progress updates.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// The chunk map is included in every Nth progress update (~1/s) and in the final one.
const CHUNK_MAP_EVERY_N_UPDATES: u32 = 5;

/// How often the resume sidecar is refreshed (only when the completed set changed).
const SIDECAR_INTERVAL: Duration = Duration::from_secs(2);

/// Returned (wrapped in `anyhow::Error`) when a download was cancelled via the cancel channel.
/// Detect with `err.downcast_ref::<DownloadCancelled>().is_some()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DownloadCancelled;

impl fmt::Display for DownloadCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("download cancelled")
    }
}

impl std::error::Error for DownloadCancelled {}

/// Optional HTTP headers attached to download requests (e.g. cookies or referer from a browser).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestHeaders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DownloadProbe {
    pub url: String,
    /// Total size in bytes; `0` means empty *or unknown* (single-stream will find out).
    pub total_bytes: u64,
    /// `true` only if the server answered a real ranged GET with a valid 206.
    pub supports_ranges: bool,
    /// Already sanitized, bare file name.
    pub suggested_filename: String,
    /// `ETag` validator, used to detect a changed resource before resuming.
    pub etag: Option<String>,
    /// `Last-Modified` validator, used to detect a changed resource before resuming.
    pub last_modified: Option<String>,
    /// Optional HTTP request headers (Cookie, Referer, User-Agent) attached to probe and chunk requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<RequestHeaders>,
}

/// Live statistics of one bound adapter (or the unbound default route) in one download run.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterProgress {
    /// Bound local IP as a string, or `"default-route"`.
    pub label: String,
    pub ip: Option<IpAddr>,
    /// Gross body bytes received through this adapter in this run, including bytes of
    /// attempts that later failed and were retried (so it measures real traffic).
    pub downloaded_bytes: u64,
    pub speed_bytes_sec: f64,
    pub active_connections: usize,
    /// The engine stopped using this adapter after repeated consecutive failures.
    pub dropped: bool,
    /// Human-readable cause of this adapter's most recent failed attempt, cleared when it
    /// completes a chunk. Never contains a URL. `None` when the adapter is healthy.
    pub last_error: Option<String>,
    /// Why the engine stopped using this adapter (set together with `dropped`).
    pub drop_reason: Option<String>,
}

/// Dynamic adapter events that can be sent to an in-flight chunked download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterUpdate {
    /// A new usable network adapter appeared and should be hot-plugged into the worker pool.
    Add(NetworkAdapter),
    /// A network adapter disconnected and its workers should be retired.
    Remove(IpAddr),
}

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_sec: f64,
    pub active_chunks: usize,
    pub completed_chunks: usize,
    pub total_chunks: usize,
    pub eta_seconds: u64,
    pub adapters: Vec<AdapterProgress>,
    /// One char per chunk: `.` pending, `>` downloading, `#` completed, `!` failed.
    /// Only present in about one update per second and in the final update (`None` means
    /// "unchanged since the last map"); always `None` for single-stream downloads.
    pub chunk_map: Option<String>,
}

pub struct DownloadEngine {
    chunk_size: u64,
    connections_per_adapter: usize,
    retry_backoff: Duration,
    stall_timeout: Duration,
    /// Test hook: sleep after every 64 KiB hashed (see `with_hash_throttle`).
    hash_throttle: Duration,
}

impl Default for DownloadEngine {
    fn default() -> Self {
        Self {
            chunk_size: 4 * 1024 * 1024, // 4 MB default chunk size
            connections_per_adapter: 4,
            retry_backoff: DEFAULT_RETRY_BACKOFF,
            stall_timeout: DEFAULT_STALL_TIMEOUT,
            hash_throttle: Duration::ZERO,
        }
    }
}

/// Parsed `Content-Range: bytes start-end/total` (total `None` for `*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContentRange {
    pub start: u64,
    pub end: u64,
    pub total: Option<u64>,
}

/// Parses a satisfied `Content-Range` value (`bytes 0-99/1000` or `bytes 0-99/*`).
pub(crate) fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.trim().split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    if end < start {
        return None;
    }
    let total = match total.trim() {
        "*" => None,
        t => {
            let t: u64 = t.parse().ok()?;
            if end >= t {
                return None;
            }
            Some(t)
        }
    };
    Some(ContentRange { start, end, total })
}

fn header_str(headers: &header::HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// `Content-Disposition` decoded as (lossy) UTF-8. `HeaderValue::to_str` rejects any
/// non-ASCII byte, which would also discard a valid RFC 5987 `filename*` next to a raw
/// UTF-8 `filename="résumé.pdf"`.
fn content_disposition_header(headers: &header::HeaderMap) -> Option<String> {
    headers
        .get(header::CONTENT_DISPOSITION)
        .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
}

fn content_length_header(headers: &header::HeaderMap) -> Option<u64> {
    header_str(headers, header::CONTENT_LENGTH).and_then(|v| v.trim().parse().ok())
}

/// Resolves once `rx` holds `true`. If the sender is dropped, never resolves.
async fn wait_for_true(rx: &mut watch::Receiver<bool>) {
    if rx.wait_for(|v| *v).await.is_err() {
        std::future::pending::<()>().await;
    }
}

fn cancelled_error() -> anyhow::Error {
    anyhow::Error::new(DownloadCancelled)
}

/// Why a single chunk attempt did not complete.
enum AttemptError {
    /// The download is stopping (cancel / fatal error elsewhere); not the chunk's fault.
    Stopped,
    /// The attempt failed and counts against the chunk's and adapter's retry budget.
    Failed(anyhow::Error),
    /// The remote resource is no longer the one the probe saw; retrying cannot help and
    /// the bytes already written are from the old version, so the whole download fails.
    ResourceChanged(anyhow::Error),
}

impl From<anyhow::Error> for AttemptError {
    fn from(e: anyhow::Error) -> Self {
        AttemptError::Failed(e)
    }
}

/// Byte/chunk counters shared between workers and the progress reporter.
#[derive(Default)]
struct Counters {
    downloaded: AtomicU64,
    active: AtomicUsize,
    completed: AtomicUsize,
}

/// Where one adapter's sockets are bound. `ip: None` is the unbound default OS route.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BindTarget {
    ip: Option<IpAddr>,
    /// OS interface name to pin egress to (SO_BINDTODEVICE on Linux); `None` if unknown.
    interface: Option<String>,
}

impl BindTarget {
    fn of(adapter: &NetworkAdapter) -> Self {
        Self {
            ip: Some(adapter.ip),
            interface: Some(adapter.name.clone()).filter(|n| !n.is_empty()),
        }
    }
}

/// Source of [`AdapterWorkerState::id`].
static NEXT_ADAPTER_STATE_ID: AtomicU64 = AtomicU64::new(0);

/// Per-adapter state shared by that adapter's workers.
struct AdapterWorkerState {
    /// Unique per state. Labels are not: a removed and re-added adapter (or a second
    /// default-route fallback) gets a new state with the same label.
    id: u64,
    label: String,
    ip: Option<IpAddr>,
    client: reqwest::Client,
    consecutive_failures: AtomicU32,
    dropped: AtomicBool,
    /// See `AdapterProgress::last_error`.
    last_error: Mutex<Option<String>>,
    /// See `AdapterProgress::drop_reason`.
    drop_reason: Mutex<Option<String>>,
    /// Gross body bytes received (never decremented; see `AdapterProgress`).
    received: AtomicU64,
    active: AtomicUsize,
    retire_tx: watch::Sender<bool>,
}

impl AdapterWorkerState {
    fn new(ip: Option<IpAddr>, client: reqwest::Client) -> Self {
        let (retire_tx, _retire_rx) = watch::channel(false);
        Self {
            id: NEXT_ADAPTER_STATE_ID.fetch_add(1, Ordering::Relaxed),
            label: DownloadEngine::bind_label(ip),
            ip,
            client,
            consecutive_failures: AtomicU32::new(0),
            dropped: AtomicBool::new(false),
            last_error: Mutex::new(None),
            drop_reason: Mutex::new(None),
            received: AtomicU64::new(0),
            active: AtomicUsize::new(0),
            retire_tx,
        }
    }

    fn progress(&self, speed_bytes_sec: f64) -> AdapterProgress {
        AdapterProgress {
            label: self.label.clone(),
            ip: self.ip,
            downloaded_bytes: self.received.load(Ordering::SeqCst),
            speed_bytes_sec,
            active_connections: self.active.load(Ordering::SeqCst),
            dropped: self.dropped.load(Ordering::SeqCst),
            last_error: self
                .last_error
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
            drop_reason: self
                .drop_reason
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
        }
    }

    fn set_last_error(&self, msg: Option<String>) {
        *self.last_error.lock().unwrap_or_else(|p| p.into_inner()) = msg;
    }

    /// Marks the adapter dropped and records why (first reason wins).
    fn drop_with_reason(&self, reason: String) {
        let mut slot = self.drop_reason.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_none() {
            *slot = Some(reason);
        }
        drop(slot);
        self.dropped.store(true, Ordering::SeqCst);
    }
}

/// The server answered a chunk request with something other than 206 (typed so the status
/// can be shown to the user without parsing the message).
#[derive(Debug)]
struct UnexpectedStatus {
    range: String,
    status: StatusCode,
}

impl std::fmt::Display for UnexpectedStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "expected 206 Partial Content for {}, got {}",
            self.range, self.status
        )
    }
}

impl std::error::Error for UnexpectedStatus {}

/// Replaces every `scheme://...` token with `<url>` so no credentials or query leak.
fn strip_urls(text: &str) -> String {
    text.split_whitespace()
        .map(|w| if w.contains("://") { "<url>" } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

fn describe_io(e: &std::io::Error) -> Option<&'static str> {
    use std::io::ErrorKind as K;
    // Windows (WSA*) and Linux errno values, for kinds std does not map on every platform.
    match e.raw_os_error() {
        Some(10060 | 110) => return Some("connect timed out"),
        Some(10061 | 111) => return Some("connection refused"),
        Some(10051 | 10065 | 101 | 113) => {
            return Some("no route to host (does this adapter have its own gateway?)")
        }
        Some(10049 | 99) => return Some("adapter address is not usable (address not available)"),
        _ => {}
    }
    match e.kind() {
        K::TimedOut => Some("connect timed out"),
        K::ConnectionRefused => Some("connection refused"),
        K::HostUnreachable | K::NetworkUnreachable => {
            Some("no route to host (does this adapter have its own gateway?)")
        }
        K::AddrNotAvailable => Some("adapter address is not usable (address not available)"),
        K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe => {
            Some("connection reset by the server or network")
        }
        _ => None,
    }
}

/// Turns a chunk-attempt failure into a short, human-readable reason with no URLs.
fn describe_failure(err: &anyhow::Error) -> String {
    let mut timed_out = false;
    let mut is_connect = false;
    for cause in err.chain() {
        if let Some(s) = cause.downcast_ref::<UnexpectedStatus>() {
            return format!("server answered HTTP {}", s.status);
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if let Some(text) = describe_io(io) {
                return text.to_string();
            }
        }
        if let Some(r) = cause.downcast_ref::<reqwest::Error>() {
            timed_out |= r.is_timeout();
            is_connect |= r.is_connect();
        }
    }
    let full = format!("{err:#}").to_lowercase();
    if full.contains("certificate") || full.contains("tls") || full.contains("handshake") {
        return "TLS error (certificate or handshake failed)".to_string();
    }
    if timed_out || full.contains("timed out") {
        return if is_connect {
            "connect timed out".to_string()
        } else {
            "stalled: no data received before the timeout".to_string()
        };
    }
    if full.contains("stall timeout") {
        return "stalled: no data received before the timeout".to_string();
    }
    if is_connect {
        return "could not connect to the server".to_string();
    }
    let head = strip_urls(&format!("{err:#}"));
    let mut short: String = head.chars().take(160).collect();
    if head.chars().count() > 160 {
        short.push('…');
    }
    short
}

/// Where and what to write for the resume sidecar of one chunked download.
struct SidecarTarget {
    path: PathBuf,
    /// Everything except `completed`.
    template: ResumeState,
}

/// State shared by every worker of one chunked download.
struct ChunkedShared {
    url: String,
    total_bytes: u64,
    /// Sent as `If-Range` on every chunk GET: the probe's strong ETag, else its
    /// Last-Modified (weak ETags are not allowed in `If-Range`).
    if_range: Option<String>,
    /// The probe's validators; every 206 must carry exactly these.
    etag: Option<String>,
    last_modified: Option<String>,
    headers: Option<RequestHeaders>,
    writer: SparseFileWriter,
    scheduler: Mutex<ChunkScheduler>,
    counters: Arc<Counters>,
    stop_tx: watch::Sender<bool>,
    last_error: Mutex<Option<String>>,
    /// Set when the remote resource changed mid-download; fails the download.
    resource_changed: Mutex<Option<String>>,
}

impl ChunkedShared {
    fn scheduler(&self) -> std::sync::MutexGuard<'_, ChunkScheduler> {
        // A poisoned lock means a worker panicked mid-update; the state is still plain data.
        self.scheduler.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn record_error(&self, msg: String) {
        *self.last_error.lock().unwrap_or_else(|p| p.into_inner()) = Some(msg);
    }
}

/// Signals workers and the sidecar persister to stop if `download()` is dropped before
/// finishing (e.g. its task was aborted).
struct StopOnDrop(Arc<ChunkedShared>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop_tx.send_replace(true);
    }
}

/// Notifies on drop, so the progress reporter ends even if `download()` is dropped.
struct NotifyOnDrop(Arc<Notify>);

impl Drop for NotifyOnDrop {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

/// The `If-Range` validator for `probe`: a strong ETag, else Last-Modified.
fn if_range_validator(probe: &DownloadProbe) -> Option<String> {
    match &probe.etag {
        Some(etag) if !etag.trim_start().starts_with("W/") => Some(etag.clone()),
        _ => probe.last_modified.clone(),
    }
}

impl DownloadEngine {
    /// Creates an engine. `chunk_size` and `connections_per_adapter` must be non-zero.
    pub fn new(chunk_size: u64, connections_per_adapter: usize) -> Result<Self> {
        if chunk_size == 0 {
            bail!("chunk_size must be greater than 0");
        }
        if connections_per_adapter == 0 {
            bail!("connections_per_adapter must be greater than 0");
        }
        Ok(Self {
            chunk_size,
            connections_per_adapter,
            ..Self::default()
        })
    }

    /// Sets the base of the exponential retry backoff (default 500ms → 0.5s,1s,2s,4s,8s).
    pub fn with_retry_backoff(mut self, base: Duration) -> Self {
        self.retry_backoff = base;
        self
    }

    /// Test hook: slows the final SHA-256 pass (sleep per 64 KiB) so tests can cancel
    /// inside the hash phase. Not for production use.
    #[doc(hidden)]
    pub fn with_hash_throttle(mut self, per_64k: Duration) -> Self {
        self.hash_throttle = per_64k;
        self
    }

    /// Sets the stall timeout: an attempt fails when no bytes arrive for this long (default 20s).
    pub fn with_stall_timeout(mut self, timeout: Duration) -> Self {
        self.stall_timeout = timeout;
        self
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    pub fn connections_per_adapter(&self) -> usize {
        self.connections_per_adapter
    }

    /// Probes the target URL with `GET` + `Range: bytes=0-0`.
    ///
    /// - `206` with `Content-Range: bytes 0-0/N` ⇒ ranges supported, size `N`.
    /// - `200` ⇒ ranges not supported, size from `Content-Length` (0 if unknown).
    /// - `416` ⇒ empty resource (size 0, single-stream).
    /// - Transport error / `405` / `501` ⇒ falls back to `HEAD` (ranges assumed unsupported).
    /// - Any other non-success status is an error.
    ///
    /// The body is never read beyond what the server already sent with the headers.
    pub async fn probe(&self, url: &str) -> Result<DownloadProbe> {
        self.probe_with_headers(url, None).await
    }

    /// Like [`probe`](Self::probe), but attaches optional HTTP request headers (Cookie, Referer,
    /// User-Agent) to the probe request.
    pub async fn probe_with_headers(
        &self,
        url: &str,
        headers: Option<RequestHeaders>,
    ) -> Result<DownloadProbe> {
        let parsed_url =
            reqwest::Url::parse(url).with_context(|| format!("Invalid URL: {}", url))?;
        let client = build_bound_http_client(None, None, self.stall_timeout)?;

        let mut req = client
            .get(parsed_url.clone())
            .header(header::RANGE, "bytes=0-0");
        if let Some(h) = &headers {
            if let Some(cookie) = &h.cookie {
                req = req.header(header::COOKIE, cookie);
            }
            if let Some(referer) = &h.referer {
                req = req.header(header::REFERER, referer);
            }
            if let Some(user_agent) = &h.user_agent {
                req = req.header(header::USER_AGENT, user_agent);
            }
        }

        let get = tokio::time::timeout(PROBE_TIMEOUT, req.send()).await;

        let get_resp = match get {
            Ok(Ok(resp)) => {
                let status = resp.status();
                if status == StatusCode::METHOD_NOT_ALLOWED || status == StatusCode::NOT_IMPLEMENTED
                {
                    warn!("Probe GET returned {}; falling back to HEAD", status);
                    None
                } else {
                    Some(resp)
                }
            }
            Ok(Err(e)) => {
                warn!(
                    "Probe GET failed at transport level ({}); falling back to HEAD",
                    e
                );
                None
            }
            Err(_) => {
                warn!(
                    "Probe GET timed out after {:?}; falling back to HEAD",
                    PROBE_TIMEOUT
                );
                None
            }
        };

        let Some(resp) = get_resp else {
            return self
                .probe_head(&client, url, &parsed_url, headers.as_ref())
                .await;
        };

        let status = resp.status();
        let resp_headers = resp.headers().clone();
        let final_url = resp.url().clone();
        info!(
            "Probe GET {} -> {} (remote {:?}, Content-Range {:?}, Content-Length {:?})",
            url,
            status,
            resp.remote_addr(),
            header_str(&resp_headers, header::CONTENT_RANGE),
            header_str(&resp_headers, header::CONTENT_LENGTH)
        );
        // Dropping the response without reading the body closes/reuses the connection;
        // for a 200 full-body answer this aborts the transfer.
        drop(resp);

        let suggested_filename = derive_filename(
            content_disposition_header(&resp_headers).as_deref(),
            &final_url,
        );
        let etag = header_str(&resp_headers, header::ETAG).map(str::to_string);
        let last_modified = header_str(&resp_headers, header::LAST_MODIFIED).map(str::to_string);

        let (total_bytes, supports_ranges) = if status == StatusCode::PARTIAL_CONTENT {
            match header_str(&resp_headers, header::CONTENT_RANGE).and_then(parse_content_range) {
                Some(ContentRange {
                    start: 0,
                    end: 0,
                    total: Some(total),
                }) => (total, true),
                other => {
                    warn!(
                        "Probe got 206 with unusable Content-Range {:?}; using single-stream",
                        other
                    );
                    (0, false)
                }
            }
        } else if status == StatusCode::RANGE_NOT_SATISFIABLE {
            // `bytes=0-0` is unsatisfiable only for an empty resource.
            (0, false)
        } else if status.is_success() {
            (content_length_header(&resp_headers).unwrap_or(0), false)
        } else {
            bail!("Remote server returned non-success status: {}", status);
        };

        if supports_ranges {
            Self::check_chunk_plan(total_bytes, self.chunk_size)?;
        }

        Ok(DownloadProbe {
            url: url.to_string(),
            total_bytes,
            supports_ranges,
            suggested_filename,
            etag,
            last_modified,
            headers,
        })
    }

    /// Refuses a size that would need more than `MAX_PLANNED_CHUNKS` chunks.
    fn check_chunk_plan(total_bytes: u64, chunk_size: u64) -> Result<()> {
        let n = chunk_count(total_bytes, chunk_size);
        if n > MAX_PLANNED_CHUNKS {
            bail!(
                "Refusing download: {} bytes at {} bytes per chunk needs too many chunks ({} > {})",
                total_bytes,
                chunk_size,
                n,
                MAX_PLANNED_CHUNKS
            );
        }
        Ok(())
    }

    /// Hashes the finished file; `Err(DownloadCancelled)` if cancelled meanwhile.
    async fn hash_output(
        &self,
        output_path: &Path,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<String> {
        match compute_sha256_cancellable(output_path, cancel, self.hash_throttle).await? {
            Some(sha) => Ok(sha),
            None => Err(cancelled_error()),
        }
    }

    async fn probe_head(
        &self,
        client: &reqwest::Client,
        url: &str,
        parsed_url: &reqwest::Url,
        custom_headers: Option<&RequestHeaders>,
    ) -> Result<DownloadProbe> {
        let mut req = client.head(parsed_url.clone());
        if let Some(h) = custom_headers {
            if let Some(cookie) = &h.cookie {
                req = req.header(header::COOKIE, cookie);
            }
            if let Some(referer) = &h.referer {
                req = req.header(header::REFERER, referer);
            }
            if let Some(user_agent) = &h.user_agent {
                req = req.header(header::USER_AGENT, user_agent);
            }
        }
        let resp = tokio::time::timeout(PROBE_TIMEOUT, req.send())
            .await
            .map_err(|_| anyhow!("HEAD probe to {} timed out", url))?
            .with_context(|| format!("Failed to send HEAD request to {}", url))?;
        let status = resp.status();
        info!("Probe HEAD {} -> {}", url, status);
        if !status.is_success() {
            bail!("Remote server returned non-success status: {}", status);
        }
        let headers = resp.headers();
        Ok(DownloadProbe {
            url: url.to_string(),
            total_bytes: content_length_header(headers).unwrap_or(0),
            // Range support is unverified without a ranged GET; stay on the safe path.
            supports_ranges: false,
            suggested_filename: derive_filename(
                content_disposition_header(headers).as_deref(),
                resp.url(),
            ),
            etag: header_str(headers, header::ETAG).map(str::to_string),
            last_modified: header_str(headers, header::LAST_MODIFIED).map(str::to_string),
            headers: custom_headers.cloned(),
        })
    }

    /// Whether the engine can bind to `adapter`: enabled, non-loopback IPv4.
    fn is_usable(adapter: &NetworkAdapter) -> bool {
        adapter.enabled && adapter.is_ipv4 && !adapter.is_loopback && adapter.ip.is_ipv4()
    }

    /// Selects the adapters to bind to (see [`Self::is_usable`]). None usable ⇒ one
    /// unbound target (default OS routing).
    fn select_bind_targets(adapters: &[NetworkAdapter]) -> Vec<BindTarget> {
        let targets: Vec<BindTarget> = adapters
            .iter()
            .filter(|a| Self::is_usable(a))
            .map(BindTarget::of)
            .collect();
        if targets.is_empty() {
            vec![BindTarget::default()]
        } else {
            targets
        }
    }

    /// Builds the HTTP client and worker state for one bind target.
    fn new_adapter_state(&self, target: &BindTarget) -> Result<Arc<AdapterWorkerState>> {
        let client =
            build_bound_http_client(target.ip, target.interface.as_deref(), self.stall_timeout)
                .with_context(|| {
                    format!(
                        "Failed to build HTTP client bound to {} (interface {:?})",
                        Self::bind_label(target.ip),
                        target.interface
                    )
                })?;
        Ok(Arc::new(AdapterWorkerState::new(target.ip, client)))
    }

    fn bind_label(ip: Option<IpAddr>) -> String {
        ip.map(|ip| ip.to_string())
            .unwrap_or_else(|| "default-route".to_string())
    }

    /// Downloads `probe.url` into `output_path` (created/truncated), returning the SHA-256 hex.
    ///
    /// Uses parallel ranged GETs across `adapters` when the probe verified range support and
    /// a known size; otherwise a single stream. When `cancel` becomes `true`, all workers stop
    /// (no write is in flight when this returns) and the result is `Err(DownloadCancelled)`.
    ///
    /// Chunked downloads keep a resume sidecar (see [`resume_sidecar_path`]) while running
    /// and after a cancel or failure; it is deleted on success.
    ///
    /// Chunk requests carry `If-Range` (strong ETag, else Last-Modified). If the server shows
    /// the resource changed mid-download (200 instead of 206, or different validators on a
    /// 206), the download fails at once instead of mixing two versions.
    ///
    /// Dropping the returned future stops all background tasks and releases the file, but
    /// unlike a cancel it cannot wait for an in-flight disk write to finish.
    pub async fn download(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        adapters: &[NetworkAdapter],
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        cancel: watch::Receiver<bool>,
    ) -> Result<String> {
        self.download_with_updates(probe, output_path, adapters, None, progress_tx, cancel)
            .await
    }

    /// Like [`download`](Self::download), but accepts an optional receiver of [`AdapterUpdate`]s
    /// allowing newly connected adapters to be hot-plugged mid-download into active chunk workers.
    pub async fn download_with_updates(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        adapters: &[NetworkAdapter],
        adapter_rx: Option<mpsc::Receiver<AdapterUpdate>>,
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        cancel: watch::Receiver<bool>,
    ) -> Result<String> {
        self.run(
            probe,
            output_path,
            adapters,
            adapter_rx,
            progress_tx,
            cancel,
            false,
        )
        .await
    }

    /// Like [`download`](Self::download), but continues from the chunks recorded in the
    /// resume sidecar of `output_path`. Falls back to a fresh download (logging why) when
    /// there is no valid sidecar, the remote resource changed (size / ETag / Last-Modified),
    /// the file on disk has the wrong length, or the server no longer supports ranges.
    pub async fn resume(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        adapters: &[NetworkAdapter],
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        cancel: watch::Receiver<bool>,
    ) -> Result<String> {
        self.resume_with_updates(probe, output_path, adapters, None, progress_tx, cancel)
            .await
    }

    /// Like [`resume`](Self::resume), but accepts an optional receiver of [`AdapterUpdate`]s
    /// allowing newly connected adapters to be hot-plugged mid-download into active chunk workers.
    pub async fn resume_with_updates(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        adapters: &[NetworkAdapter],
        adapter_rx: Option<mpsc::Receiver<AdapterUpdate>>,
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        cancel: watch::Receiver<bool>,
    ) -> Result<String> {
        self.run(
            probe,
            output_path,
            adapters,
            adapter_rx,
            progress_tx,
            cancel,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        adapters: &[NetworkAdapter],
        adapter_rx: Option<mpsc::Receiver<AdapterUpdate>>,
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        mut cancel: watch::Receiver<bool>,
        resume: bool,
    ) -> Result<String> {
        if *cancel.borrow_and_update() {
            return Err(cancelled_error());
        }

        let targets = Self::select_bind_targets(adapters);
        info!(
            "Download {} -> {:?} (size {} bytes, ranges {}, adapters {:?})",
            probe.url, output_path, probe.total_bytes, probe.supports_ranges, targets
        );

        if !probe.supports_ranges || probe.total_bytes == 0 {
            info!("Using single-stream download (no verified range support or unknown/zero size)");
            if resume {
                info!("Single-stream downloads cannot resume; starting from zero");
            }
            // A stale sidecar must never outlive the file it described.
            remove_resume_sidecar(output_path).with_context(|| {
                format!("Failed to remove stale resume data for {:?}", output_path)
            })?;
            return self
                .download_single_stream(probe, output_path, &targets[0], progress_tx, cancel)
                .await;
        }

        self.download_chunked(
            probe,
            output_path,
            &targets,
            adapter_rx,
            progress_tx,
            cancel,
            resume,
        )
        .await
    }

    /// Opens `output_path` for resuming from its sidecar. Returns the chunk plan (with
    /// recorded chunks marked `Completed`), the chunk size used, and the writer.
    async fn open_for_resume(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
    ) -> Result<(Vec<Chunk>, u64, SparseFileWriter, Option<RequestHeaders>)> {
        let state = read_sidecar(&resume_sidecar_path(output_path))?;
        validate(&state, probe)?;
        // A tampered sidecar must not make us allocate an absurd plan.
        let mut chunks = try_plan_chunks(probe.total_bytes, state.chunk_size)?;
        let done = decode_ranges(&state.completed, chunks.len())?;
        let writer = SparseFileWriter::open_existing(output_path, probe.total_bytes).await?;
        for id in done {
            // `plan_chunks` ids are their positions.
            chunks[id].status = ChunkStatus::Completed;
            chunks[id].downloaded = chunks[id].size;
        }
        Ok((chunks, state.chunk_size, writer, state.headers))
    }

    #[allow(clippy::too_many_arguments)]
    async fn download_chunked(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        targets: &[BindTarget],
        mut adapter_rx: Option<mpsc::Receiver<AdapterUpdate>>,
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        mut cancel: watch::Receiver<bool>,
        resume: bool,
    ) -> Result<String> {
        let mut initial_states = Vec::new();
        for target in targets {
            match self.new_adapter_state(target) {
                Ok(state) => initial_states.push(state),
                Err(e) => error!("{:#}", e),
            }
        }
        if initial_states.is_empty() {
            bail!("Could not build an HTTP client for any selected network adapter");
        }
        let adapter_states = Arc::new(RwLock::new(initial_states));

        let resumed = if resume {
            match self.open_for_resume(probe, output_path).await {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!(
                        "Cannot resume {:?} ({:#}); starting from zero",
                        output_path, e
                    );
                    None
                }
            }
        } else {
            None
        };
        let (chunks, chunk_size, writer, resumed_headers) = match resumed {
            Some(r) => r,
            None => {
                // Remove any stale sidecar *before* truncating, so it can never describe
                // the new, empty file.
                remove_resume_sidecar(output_path).with_context(|| {
                    format!("Failed to remove stale resume data for {:?}", output_path)
                })?;
                let chunks = try_plan_chunks(probe.total_bytes, self.chunk_size)?;
                if chunks.is_empty() {
                    bail!(
                        "Refusing to download: planned zero chunks for {} bytes (chunk size {})",
                        probe.total_bytes,
                        self.chunk_size
                    );
                }
                let writer = SparseFileWriter::create(output_path, probe.total_bytes).await?;
                (chunks, self.chunk_size, writer, None)
            }
        };
        let effective_headers = probe.headers.clone().or(resumed_headers);
        let total_chunks = chunks.len();
        let resumed_chunks: Vec<&Chunk> = chunks
            .iter()
            .filter(|c| c.status == ChunkStatus::Completed)
            .collect();
        let resumed_bytes: u64 = resumed_chunks.iter().map(|c| c.size).sum();
        info!(
            "Planned {} chunks of up to {} bytes each; {} chunks ({} bytes) already on disk",
            total_chunks,
            chunk_size,
            resumed_chunks.len(),
            resumed_bytes
        );

        let counters = Arc::new(Counters {
            downloaded: AtomicU64::new(resumed_bytes),
            active: AtomicUsize::new(0),
            completed: AtomicUsize::new(resumed_chunks.len()),
        });
        let (stop_tx, stop_rx) = watch::channel(false);
        let shared = Arc::new(ChunkedShared {
            url: probe.url.clone(),
            total_bytes: probe.total_bytes,
            if_range: if_range_validator(probe),
            etag: probe.etag.clone(),
            last_modified: probe.last_modified.clone(),
            headers: effective_headers.clone(),
            writer: writer.clone(),
            scheduler: Mutex::new(ChunkScheduler::new(
                chunks,
                MAX_CHUNK_ATTEMPTS,
                self.retry_backoff,
            )),
            counters: Arc::clone(&counters),
            stop_tx,
            last_error: Mutex::new(None),
            resource_changed: Mutex::new(None),
        });
        let _stop_guard = StopOnDrop(Arc::clone(&shared));

        let sidecar = Arc::new(SidecarTarget {
            path: resume_sidecar_path(output_path),
            template: ResumeState {
                version: SIDECAR_VERSION,
                total_bytes: probe.total_bytes,
                chunk_size,
                etag: probe.etag.clone(),
                last_modified: probe.last_modified.clone(),
                headers: effective_headers,
                completed: String::new(),
            },
        });
        let persister_done = Arc::new(Notify::new());
        let persister = tokio::spawn(run_sidecar_persister(
            Arc::clone(&shared),
            Arc::clone(&sidecar),
            Arc::clone(&persister_done),
        ));

        let start_time = Instant::now();
        let (progress_handle, progress_done) = spawn_progress_reporter(
            progress_tx,
            Arc::clone(&counters),
            Arc::clone(&adapter_states),
            Some(Arc::clone(&shared)),
            probe.total_bytes,
            total_chunks,
        );
        let _progress_guard = NotifyOnDrop(Arc::clone(&progress_done));

        let mut worker_set = JoinSet::new();
        {
            let states = adapter_states.read().unwrap();
            for adapter in states.iter() {
                for worker_idx in 0..self.connections_per_adapter {
                    worker_set.spawn(run_chunk_worker(
                        Arc::clone(&shared),
                        Arc::clone(&adapter_states),
                        Arc::clone(adapter),
                        worker_idx,
                        stop_rx.clone(),
                    ));
                }
            }
        }

        let mut user_cancelled = false;
        loop {
            if worker_set.is_empty() {
                break;
            }
            tokio::select! {
                biased;
                _ = wait_for_true(&mut cancel) => {
                    info!("Cancel requested; stopping all workers");
                    user_cancelled = true;
                    shared.stop_tx.send_replace(true);
                    break;
                }
                maybe_update = async {
                    match &mut adapter_rx {
                        Some(rx) => rx.recv().await,
                        None => futures_util::future::pending().await,
                    }
                } => {
                    match maybe_update {
                        Some(AdapterUpdate::Add(adapter)) => {
                            if Self::is_usable(&adapter) {
                                let already_exists = {
                                    let states = adapter_states.read().unwrap();
                                    states.iter().any(|s| {
                                        s.ip == Some(adapter.ip)
                                            && !s.dropped.load(Ordering::SeqCst)
                                    })
                                };
                                if !already_exists {
                                    info!(
                                        "Dynamically aggregating new network adapter {} ({}, interface {:?}) into download",
                                        adapter.id, adapter.ip, adapter.name
                                    );
                                    // A default-route fallback is retired only once this adapter
                                    // completes a chunk (see `retire_default_route`).
                                    match self.new_adapter_state(&BindTarget::of(&adapter)) {
                                        Ok(new_state) => {
                                            adapter_states.write().unwrap().push(Arc::clone(&new_state));
                                            for worker_idx in 0..self.connections_per_adapter {
                                                worker_set.spawn(run_chunk_worker(
                                                    Arc::clone(&shared),
                                                    Arc::clone(&adapter_states),
                                                    Arc::clone(&new_state),
                                                    worker_idx,
                                                    stop_rx.clone(),
                                                ));
                                            }
                                        }
                                        Err(e) => {
                                            error!("Newly added adapter {}: {:#}", adapter.id, e);
                                        }
                                    }
                                }
                            }
                        }
                        Some(AdapterUpdate::Remove(ip)) => {
                            info!("Network adapter {} disconnected; dropping workers", ip);
                            let states = adapter_states.read().unwrap();
                            for target in states.iter().filter(|s| s.ip == Some(ip)) {
                                target.drop_with_reason("adapter disconnected or disabled".to_string());
                                target.retire_tx.send_replace(true);
                            }
                            let has_active = states.iter().any(|s| !s.dropped.load(Ordering::SeqCst));
                            let has_fallback = states.iter().any(|s| s.ip.is_none() && !s.dropped.load(Ordering::SeqCst));
                            if !has_active && !has_fallback {
                                info!("All specific network adapters were removed/disabled; falling back to default OS routing");
                                match self.new_adapter_state(&BindTarget::default()) {
                                    Ok(fallback_state) => {
                                        drop(states);
                                        adapter_states.write().unwrap().push(Arc::clone(&fallback_state));
                                        for worker_idx in 0..self.connections_per_adapter {
                                            worker_set.spawn(run_chunk_worker(
                                                Arc::clone(&shared),
                                                Arc::clone(&adapter_states),
                                                Arc::clone(&fallback_state),
                                                worker_idx,
                                                stop_rx.clone(),
                                            ));
                                        }
                                    }
                                    Err(e) => {
                                        error!("Default-route fallback: {:#}", e);
                                    }
                                }
                            }
                        }
                        None => {
                            adapter_rx = None;
                        }
                    }
                }
                Some(join_res) = worker_set.join_next() => {
                    if let Err(e) = join_res {
                        error!("Worker task panicked: {:?}", e);
                    }
                }
            }
        }

        // Never abort workers: one may be inside `writer.write_at`, and aborting would leave
        // that write running on the blocking pool after we return. Stop was already sent and
        // workers observe it at every network await, so joining them is prompt.
        while let Some(res) = worker_set.join_next().await {
            if let Err(e) = res {
                error!("Worker task panicked during shutdown: {:?}", e);
            }
        }
        // A cancel that raced with the last chunk finishing still counts as cancelled.
        let user_cancelled = user_cancelled || *cancel.borrow();

        finish_progress(progress_handle, progress_done).await;

        persister_done.notify_one();
        let mut last_persisted = persister.await.unwrap_or_else(|e| {
            error!("Resume sidecar task panicked: {:?}", e);
            None
        });
        {
            // Record the final state so a later `resume` loses at most in-flight chunks.
            // Also when everything completed: the sidecar must list every chunk until the
            // SHA-256 is done, so a pause during hashing resumes into a verify-only run.
            persist_sidecar(&shared, &sidecar, &mut last_persisted).await;
        }

        if user_cancelled {
            return Err(cancelled_error());
        }

        let resource_changed = shared
            .resource_changed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(msg) = resource_changed {
            bail!(
                "Download failed: the file changed on the server during the download; start it again. {}",
                msg
            );
        }

        {
            let sched = shared.scheduler();
            if !sched.all_completed() {
                let failed = sched.ids_with_status(ChunkStatus::Failed);
                let incomplete = sched.incomplete_ids();
                let last_error = shared
                    .last_error
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone()
                    .unwrap_or_else(|| "none recorded".to_string());
                let states = adapter_states.read().unwrap();
                let dropped: Vec<&str> = states
                    .iter()
                    .filter(|a| a.dropped.load(Ordering::SeqCst))
                    .map(|a| a.label.as_str())
                    .collect();
                if !failed.is_empty() {
                    bail!(
                        "Download failed: chunk(s) {:?} failed after {} attempts ({} of {} chunks incomplete). Last error: {}",
                        failed,
                        MAX_CHUNK_ATTEMPTS,
                        incomplete.len(),
                        total_chunks,
                        last_error
                    );
                }
                if dropped.len() == states.len() {
                    bail!(
                        "Download failed: all network adapters {:?} were dropped after {} consecutive failures; {} of {} chunks incomplete (ids {:?}). Last error: {}",
                        dropped,
                        MAX_CONSECUTIVE_ADAPTER_FAILURES,
                        incomplete.len(),
                        total_chunks,
                        incomplete,
                        last_error
                    );
                }
                bail!(
                    "Download incomplete: {} of {} chunks did not complete (ids {:?}). Last error: {}",
                    incomplete.len(),
                    total_chunks,
                    incomplete,
                    last_error
                );
            }
        }

        let downloaded = counters.downloaded.load(Ordering::SeqCst);
        if downloaded != probe.total_bytes {
            bail!(
                "Byte accounting mismatch: counted {} bytes, expected {}",
                downloaded,
                probe.total_bytes
            );
        }

        writer.sync().await.context("Failed syncing file to disk")?;

        let duration = start_time.elapsed().as_secs_f64();
        info!(
            "Download completed in {:.2}s (avg {:.2} MB/s)",
            duration,
            probe.total_bytes as f64 / duration.max(1e-9) / (1024.0 * 1024.0)
        );

        // Keep the sidecar until the hash succeeded: if a cancel/abort lands while hashing,
        // the next resume must still see "all chunks completed" instead of truncating a
        // finished file.
        let sha256 = self.hash_output(output_path, &mut cancel).await?;
        if let Err(e) = remove_resume_sidecar(output_path) {
            warn!(
                "Failed to remove resume sidecar for {:?}: {}",
                output_path, e
            );
        }
        info!("File SHA-256: {}", sha256);
        Ok(sha256)
    }

    async fn download_single_stream(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        target: &BindTarget,
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<String> {
        let adapter = self.new_adapter_state(target)?;
        let label = adapter.label.clone();

        let mut req = adapter.client.get(&probe.url);
        if let Some(h) = &probe.headers {
            if let Some(cookie) = &h.cookie {
                req = req.header(header::COOKIE, cookie);
            }
            if let Some(referer) = &h.referer {
                req = req.header(header::REFERER, referer);
            }
            if let Some(user_agent) = &h.user_agent {
                req = req.header(header::USER_AGENT, user_agent);
            }
        }

        let resp = tokio::select! {
            biased;
            _ = wait_for_true(&mut cancel) => return Err(cancelled_error()),
            r = req.send() => r.with_context(|| format!("Single-stream request to {} via {} failed", probe.url, label))?,
        };
        let status = resp.status();
        info!(
            "Single-stream GET via {} -> {} (remote {:?}, Content-Length {:?})",
            label,
            status,
            resp.remote_addr(),
            resp.content_length()
        );
        if !status.is_success() {
            bail!("Single-stream download failed: server returned {}", status);
        }
        let expected = resp.content_length();
        let total_for_progress = expected.unwrap_or(0);

        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await.with_context(|| {
                    format!("Failed to create parent directory for {:?}", output_path)
                })?;
            }
        }
        let mut file = tokio::fs::File::create(output_path)
            .await
            .with_context(|| format!("Failed to create output file {:?}", output_path))?;

        let counters = Arc::new(Counters::default());
        counters.active.store(1, Ordering::SeqCst);
        adapter.active.store(1, Ordering::SeqCst);
        let (progress_handle, progress_done) = spawn_progress_reporter(
            progress_tx,
            Arc::clone(&counters),
            Arc::new(RwLock::new(vec![Arc::clone(&adapter)])),
            None,
            total_for_progress,
            1,
        );
        let _progress_guard = NotifyOnDrop(Arc::clone(&progress_done));

        let result: Result<u64> = async {
            let mut stream = resp.bytes_stream();
            let mut received = 0u64;
            loop {
                let next = tokio::select! {
                    biased;
                    _ = wait_for_true(&mut cancel) => return Err(cancelled_error()),
                    n = stream.next() => n,
                };
                let Some(item) = next else { break };
                let bytes =
                    item.context("Error reading single-stream body (stall or disconnect)")?;
                let len = bytes.len() as u64;
                if let Some(expected) = expected {
                    if received + len > expected {
                        bail!("Server sent more than Content-Length {} bytes", expected);
                    }
                }
                file.write_all(&bytes)
                    .await
                    .with_context(|| format!("Failed writing to {:?}", output_path))?;
                received += len;
                counters.downloaded.fetch_add(len, Ordering::SeqCst);
                adapter.received.fetch_add(len, Ordering::SeqCst);
            }
            if let Some(expected) = expected {
                if received != expected {
                    bail!(
                        "Single-stream body ended short: received {} of {} bytes",
                        received,
                        expected
                    );
                }
            }
            file.flush().await?;
            file.sync_all()
                .await
                .context("Failed syncing file to disk")?;
            Ok(received)
        }
        .await;
        // tokio::fs::File may still have a write in flight on the blocking pool; wait for it
        // so that nothing touches the file after we return (including on cancel/error).
        let _ = file.flush().await;

        counters.active.store(0, Ordering::SeqCst);
        adapter.active.store(0, Ordering::SeqCst);
        if result.is_ok() {
            counters.completed.store(1, Ordering::SeqCst);
        }
        finish_progress(progress_handle, progress_done).await;
        drop(file);

        let received = result?;
        info!("Single-stream download finished: {} bytes", received);
        let sha256 = self.hash_output(output_path, &mut cancel).await?;
        info!("File SHA-256: {}", sha256);
        Ok(sha256)
    }
}

/// One worker: repeatedly claims a chunk, downloads it on `adapter`, and reports the result.
async fn run_chunk_worker(
    shared: Arc<ChunkedShared>,
    adapter_states: Arc<RwLock<Vec<Arc<AdapterWorkerState>>>>,
    adapter: Arc<AdapterWorkerState>,
    worker_idx: usize,
    mut stop: watch::Receiver<bool>,
) {
    let mut retire = adapter.retire_tx.subscribe();
    loop {
        if *stop.borrow() || *retire.borrow() || adapter.dropped.load(Ordering::SeqCst) {
            return;
        }

        let claim = shared
            .scheduler()
            .claim(Instant::now(), Some(adapter.label.clone()));
        let chunk = match claim {
            Claim::Finished => return,
            Claim::Wait(delay) => {
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = wait_for_true(&mut stop) => return,
                    _ = wait_for_true(&mut retire) => return,
                }
                continue;
            }
            Claim::Chunk(chunk) => chunk,
        };

        debug!(
            "Worker {} via {} starting chunk {} range [{}, {}] (attempt {})",
            worker_idx, adapter.label, chunk.id, chunk.start, chunk.end, chunk.attempts
        );

        shared.counters.active.fetch_add(1, Ordering::SeqCst);
        adapter.active.fetch_add(1, Ordering::SeqCst);
        let mut counted = 0u64;
        let result = fetch_chunk(
            &adapter,
            &shared,
            &chunk,
            &mut counted,
            &mut stop,
            &mut retire,
        )
        .await;
        adapter.active.fetch_sub(1, Ordering::SeqCst);
        shared.counters.active.fetch_sub(1, Ordering::SeqCst);

        match result {
            Ok(()) => {
                shared.scheduler().complete(chunk.id);
                shared.counters.completed.fetch_add(1, Ordering::SeqCst);
                adapter.consecutive_failures.store(0, Ordering::SeqCst);
                adapter.set_last_error(None);
                if adapter.ip.is_some() {
                    retire_default_route(&adapter_states, &adapter);
                }
                debug!(
                    "Worker {} via {} completed chunk {} ({} bytes)",
                    worker_idx, adapter.label, chunk.id, counted
                );
            }
            Err(AttemptError::Stopped) => {
                shared
                    .counters
                    .downloaded
                    .fetch_sub(counted, Ordering::SeqCst);
                shared.scheduler().release(chunk.id);
                return;
            }
            Err(AttemptError::ResourceChanged(e)) => {
                shared
                    .counters
                    .downloaded
                    .fetch_sub(counted, Ordering::SeqCst);
                shared.scheduler().release(chunk.id);
                let msg = format!(
                    "chunk {} range [{}, {}] via {}: {:#}",
                    chunk.id, chunk.start, chunk.end, adapter.label, e
                );
                error!("Remote resource changed; stopping download: {}", msg);
                shared.record_error(msg.clone());
                *shared
                    .resource_changed
                    .lock()
                    .unwrap_or_else(|p| p.into_inner()) = Some(msg);
                shared.stop_tx.send_replace(true);
                return;
            }
            Err(AttemptError::Failed(e)) => {
                // Only this attempt's own bytes are subtracted: no underflow possible.
                shared
                    .counters
                    .downloaded
                    .fetch_sub(counted, Ordering::SeqCst);
                let msg = format!(
                    "chunk {} range [{}, {}] via {}: {:#} (received {} of {} bytes)",
                    chunk.id, chunk.start, chunk.end, adapter.label, e, counted, chunk.size
                );
                shared.record_error(msg.clone());
                adapter.set_last_error(Some(describe_failure(&e)));

                let outcome = shared.scheduler().fail(chunk.id, Instant::now());
                match outcome {
                    FailOutcome::Retry { after } => warn!(
                        "Attempt {}/{} failed: {}; retry after {:?}",
                        chunk.attempts, MAX_CHUNK_ATTEMPTS, msg, after
                    ),
                    FailOutcome::Failed => {
                        error!(
                            "Chunk {} permanently failed after {} attempts: {}",
                            chunk.id, chunk.attempts, msg
                        );
                        shared.stop_tx.send_replace(true);
                    }
                }

                let failures = adapter.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= MAX_CONSECUTIVE_ADAPTER_FAILURES {
                    // Dropping the last usable adapter would end the download while its chunks
                    // still have retries left, so keep it and let the per-chunk attempt budget
                    // decide. The write lock makes concurrent drop decisions take turns.
                    let states = adapter_states.write().unwrap();
                    let others_alive = states
                        .iter()
                        .any(|s| !Arc::ptr_eq(s, &adapter) && !s.dropped.load(Ordering::SeqCst));
                    if others_alive {
                        if !adapter.dropped.load(Ordering::SeqCst) {
                            adapter.drop_with_reason(format!(
                                "dropped after {failures} consecutive failures"
                            ));
                            error!(
                                "Dropping adapter {} for this download after {} consecutive failures",
                                adapter.label, failures
                            );
                        }
                        return;
                    }
                    if failures == MAX_CONSECUTIVE_ADAPTER_FAILURES {
                        warn!(
                            "Adapter {} failed {} times in a row but is the last one left; keeping it",
                            adapter.label, failures
                        );
                    }
                }
            }
        }
    }
}

/// Retires every live default-route fallback once `proven` (an explicit adapter) has
/// completed a chunk. Retiring earlier could leave the download with only an adapter that
/// never works.
fn retire_default_route(
    adapter_states: &RwLock<Vec<Arc<AdapterWorkerState>>>,
    proven: &AdapterWorkerState,
) {
    if proven.dropped.load(Ordering::SeqCst) {
        return;
    }
    let states = adapter_states.read().unwrap();
    for fallback in states
        .iter()
        .filter(|s| s.ip.is_none() && !s.dropped.load(Ordering::SeqCst))
    {
        info!(
            "Retiring default-route fallback workers now that adapter {} completed a chunk",
            proven.label
        );
        fallback.drop_with_reason("retired: another adapter took over".to_string());
        fallback.retire_tx.send_replace(true);
    }
}

/// Downloads one chunk with strict validation. `counted` tracks bytes this attempt added to
/// the shared counter so the caller can subtract exactly that on failure.
///
/// Stop is only observed while waiting on the network, never during a disk write, so when
/// this returns no write issued by it is still in flight.
async fn fetch_chunk(
    adapter: &AdapterWorkerState,
    shared: &ChunkedShared,
    chunk: &Chunk,
    counted: &mut u64,
    stop: &mut watch::Receiver<bool>,
    retire: &mut watch::Receiver<bool>,
) -> Result<(), AttemptError> {
    let range = chunk.to_range_header();
    let mut request = adapter
        .client
        .get(&shared.url)
        .header(header::RANGE, &range);
    if let Some(validator) = &shared.if_range {
        request = request.header(header::IF_RANGE, validator);
    }
    if let Some(h) = &shared.headers {
        if let Some(cookie) = &h.cookie {
            request = request.header(header::COOKIE, cookie);
        }
        if let Some(referer) = &h.referer {
            request = request.header(header::REFERER, referer);
        }
        if let Some(user_agent) = &h.user_agent {
            request = request.header(header::USER_AGENT, user_agent);
        }
    }
    let request = request.send();

    let resp = tokio::select! {
        biased;
        _ = wait_for_true(stop) => return Err(AttemptError::Stopped),
        _ = wait_for_true(retire) => return Err(AttemptError::Stopped),
        r = request => r.with_context(|| format!("Request for {} failed", range))?,
    };

    let status = resp.status();
    let content_range = header_str(resp.headers(), header::CONTENT_RANGE).map(str::to_string);
    debug!(
        "Chunk {} via {} (remote {:?}) {} -> {} Content-Range {:?}",
        chunk.id,
        adapter.label,
        resp.remote_addr(),
        range,
        status,
        content_range
    );

    if status == StatusCode::OK && shared.if_range.is_some() {
        // With If-Range, a 200 means the validator no longer matches.
        return Err(AttemptError::ResourceChanged(anyhow!(
            "server answered {} with 200 instead of 206: If-Range {:?} no longer matches",
            range,
            shared.if_range
        )));
    }
    if status != StatusCode::PARTIAL_CONTENT {
        return Err(AttemptError::Failed(anyhow::Error::new(UnexpectedStatus {
            range,
            status,
        })));
    }
    // RFC 9110: a 206 carries the same validators a 200 would, so any difference
    // (including one appearing or disappearing) means a different representation.
    for (name, expected) in [
        (header::ETAG, &shared.etag),
        (header::LAST_MODIFIED, &shared.last_modified),
    ] {
        let got = header_str(resp.headers(), name.clone());
        if got != expected.as_deref() {
            return Err(AttemptError::ResourceChanged(anyhow!(
                "{} for {} is {:?}, probe saw {:?}",
                name,
                range,
                got,
                expected
            )));
        }
    }
    let parsed = content_range
        .as_deref()
        .and_then(parse_content_range)
        .ok_or_else(|| {
            anyhow!(
                "missing or invalid Content-Range {:?} for {}",
                content_range,
                range
            )
        })?;
    if parsed.start != chunk.start || parsed.end != chunk.end {
        return Err(AttemptError::Failed(anyhow!(
            "Content-Range {:?} does not match requested {}",
            content_range,
            range
        )));
    }
    if let Some(total) = parsed.total {
        if total != shared.total_bytes {
            return Err(AttemptError::Failed(anyhow!(
                "remote size changed: Content-Range total {} != probed {}",
                total,
                shared.total_bytes
            )));
        }
    }

    let mut stream = resp.bytes_stream();
    let mut offset = chunk.start;
    loop {
        let next = tokio::select! {
            biased;
            _ = wait_for_true(stop) => return Err(AttemptError::Stopped),
            _ = wait_for_true(retire) => return Err(AttemptError::Stopped),
            n = stream.next() => n,
        };
        let Some(item) = next else { break };
        let bytes = item.context("error reading chunk body (stall timeout or disconnect)")?;
        let len = bytes.len() as u64;
        if *counted + len > chunk.size {
            return Err(AttemptError::Failed(anyhow!(
                "server sent more than the {} bytes requested by {}",
                chunk.size,
                range
            )));
        }
        shared.writer.write_at(offset, bytes).await?;
        offset += len;
        *counted += len;
        shared.counters.downloaded.fetch_add(len, Ordering::SeqCst);
        adapter.received.fetch_add(len, Ordering::SeqCst);
    }

    if *counted != chunk.size {
        return Err(AttemptError::Failed(anyhow!(
            "body ended short: received {} of {} bytes for {}",
            counted,
            chunk.size,
            range
        )));
    }
    Ok(())
}

/// Writes the sidecar if the completed set changed since `last` (updating `last`).
///
/// Order matters for durability: snapshot the completed ids (their writes have all
/// returned), sync the data file, and only then record them.
async fn persist_sidecar(
    shared: &ChunkedShared,
    sidecar: &SidecarTarget,
    last: &mut Option<Vec<usize>>,
) {
    let ids = shared.scheduler().completed_ids();
    if last.as_ref() == Some(&ids) {
        return;
    }
    if let Err(e) = shared.writer.sync().await {
        warn!("Not updating resume sidecar: data sync failed: {:#}", e);
        return;
    }
    let state = ResumeState {
        completed: encode_ranges(&ids),
        ..sidecar.template.clone()
    };
    let path = sidecar.path.clone();
    match tokio::task::spawn_blocking(move || write_sidecar(&path, &state)).await {
        Ok(Ok(())) => {
            debug!(
                "Resume sidecar {:?} records {} completed chunks",
                sidecar.path,
                ids.len()
            );
            *last = Some(ids);
        }
        Ok(Err(e)) => warn!("Failed to write resume sidecar {:?}: {:#}", sidecar.path, e),
        Err(e) => warn!("Resume sidecar write task panicked: {:?}", e),
    }
}

/// Writes the sidecar immediately and then every [`SIDECAR_INTERVAL`] until `done` is
/// notified or the download stops (which also covers `download()` being dropped, via
/// [`StopOnDrop`]; otherwise this task would hold the open file forever).
/// Returns the last completed set it recorded.
async fn run_sidecar_persister(
    shared: Arc<ChunkedShared>,
    sidecar: Arc<SidecarTarget>,
    done: Arc<Notify>,
) -> Option<Vec<usize>> {
    let mut stop = shared.stop_tx.subscribe();
    let mut last = None;
    persist_sidecar(&shared, &sidecar, &mut last).await;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(SIDECAR_INTERVAL) => {}
            _ = done.notified() => return last,
            _ = wait_for_true(&mut stop) => {
                // Record what is durably complete so far, then end.
                persist_sidecar(&shared, &sidecar, &mut last).await;
                return last;
            }
        }
        persist_sidecar(&shared, &sidecar, &mut last).await;
    }
}

/// Exponential moving average used for all displayed speeds.
fn ema(prev: Option<f64>, sample: f64) -> f64 {
    match prev {
        Some(prev) => 0.3 * sample + 0.7 * prev,
        None => sample,
    }
}

/// Spawns the periodic progress reporter (if a sender was provided).
/// Periodic updates use `try_send` so a slow consumer can never stall the download.
/// `chunked` provides the chunk map; `None` for single-stream downloads.
fn spawn_progress_reporter(
    progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
    counters: Arc<Counters>,
    adapters: Arc<RwLock<Vec<Arc<AdapterWorkerState>>>>,
    chunked: Option<Arc<ChunkedShared>>,
    total_bytes: u64,
    total_chunks: usize,
) -> (Option<JoinHandle<()>>, Arc<Notify>) {
    let done = Arc::new(Notify::new());
    let Some(tx) = progress_tx else {
        return (None, done);
    };
    let done_rx = Arc::clone(&done);

    let handle = tokio::spawn(async move {
        let chunk_map = || chunked.as_ref().map(|c| c.scheduler().chunk_map());
        let snapshot = |speed: f64,
                        eta: u64,
                        active_adapters: &[Arc<AdapterWorkerState>],
                        adapter_speeds: &[f64],
                        map: Option<String>| ProgressUpdate {
            downloaded_bytes: counters.downloaded.load(Ordering::SeqCst),
            total_bytes,
            speed_bytes_sec: speed,
            active_chunks: counters.active.load(Ordering::SeqCst),
            completed_chunks: counters.completed.load(Ordering::SeqCst),
            total_chunks,
            eta_seconds: eta,
            adapters: active_adapters
                .iter()
                .zip(adapter_speeds)
                .map(|(a, s)| a.progress(*s))
                .collect(),
            chunk_map: map,
        };

        // Start from the current counts so resumed bytes don't show up as a speed spike.
        let mut last_bytes = counters.downloaded.load(Ordering::SeqCst);
        let mut last_instant = Instant::now();
        let mut speed_ema: Option<f64> = None;
        // Keyed by state id, not label: labels repeat when an adapter is re-added.
        let mut adapter_last: HashMap<u64, u64> = HashMap::new();
        let mut adapter_ema: HashMap<u64, f64> = HashMap::new();
        {
            let init = adapters.read().unwrap();
            for a in init.iter() {
                adapter_last.insert(a.id, a.received.load(Ordering::SeqCst));
            }
        }
        let mut tick: u32 = 0;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(PROGRESS_INTERVAL) => {}
                _ = done_rx.notified() => {
                    let active = adapters.read().unwrap().clone();
                    let zeros = vec![0.0; active.len()];
                    let final_update = snapshot(0.0, 0, &active, &zeros, chunk_map());
                    let _ = tokio::time::timeout(Duration::from_secs(1), tx.send(final_update)).await;
                    return;
                }
            }

            let now = Instant::now();
            let current = counters.downloaded.load(Ordering::SeqCst);
            let elapsed = now.duration_since(last_instant).as_secs_f64();
            let active = adapters.read().unwrap().clone();
            let mut adapter_speeds = Vec::with_capacity(active.len());

            if elapsed > 0.0 {
                let instant_speed = current.saturating_sub(last_bytes) as f64 / elapsed;
                speed_ema = Some(ema(speed_ema, instant_speed));
                for a in &active {
                    let received = a.received.load(Ordering::SeqCst);
                    let prev = *adapter_last.get(&a.id).unwrap_or(&received);
                    let sample = received.saturating_sub(prev) as f64 / elapsed;
                    let new_ema = ema(adapter_ema.get(&a.id).copied(), sample);
                    adapter_ema.insert(a.id, new_ema);
                    adapter_last.insert(a.id, received);
                    adapter_speeds.push(new_ema);
                }
            } else {
                for a in &active {
                    let s = adapter_ema.get(&a.id).copied().unwrap_or(0.0);
                    adapter_speeds.push(s);
                }
            }
            last_bytes = current;
            last_instant = now;

            let speed = speed_ema.unwrap_or(0.0);
            let eta = if speed > 0.0 && total_bytes > 0 {
                (total_bytes.saturating_sub(current) as f64 / speed).ceil() as u64
            } else {
                0
            };
            let map = if tick.is_multiple_of(CHUNK_MAP_EVERY_N_UPDATES) {
                chunk_map()
            } else {
                None
            };
            tick = tick.wrapping_add(1);

            match tx.try_send(snapshot(speed, eta, &active, &adapter_speeds, map)) {
                Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                Err(mpsc::error::TrySendError::Closed(_)) => return,
            }
        }
    });

    (Some(handle), done)
}

async fn finish_progress(handle: Option<JoinHandle<()>>, done: Arc<Notify>) {
    // `notify_one` stores a permit, so the reporter sees it even if not currently waiting.
    done.notify_one();
    if let Some(handle) = handle {
        if let Err(e) = handle.await {
            error!("Progress task panicked: {:?}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_failure_maps_common_causes() {
        use std::io::{Error, ErrorKind};
        let wrap = |e: Error| anyhow::Error::new(e).context("Request for bytes=0-9 failed");
        assert_eq!(
            describe_failure(&wrap(Error::from(ErrorKind::ConnectionRefused))),
            "connection refused"
        );
        assert_eq!(
            describe_failure(&wrap(Error::from_raw_os_error(10065))),
            "no route to host (does this adapter have its own gateway?)"
        );
        assert_eq!(
            describe_failure(&wrap(Error::from(ErrorKind::TimedOut))),
            "connect timed out"
        );
        let status = anyhow::Error::new(UnexpectedStatus {
            range: "bytes=0-9".into(),
            status: StatusCode::FORBIDDEN,
        });
        assert_eq!(
            describe_failure(&status),
            "server answered HTTP 403 Forbidden"
        );
        assert_eq!(
            describe_failure(&anyhow!("invalid peer certificate: UnknownIssuer")),
            "TLS error (certificate or handshake failed)"
        );
        assert_eq!(
            describe_failure(&anyhow!(
                "error reading chunk body (stall timeout or disconnect)"
            )),
            "stalled: no data received before the timeout"
        );
    }

    #[test]
    fn describe_failure_never_leaks_urls() {
        let e = anyhow!("weird failure at https://user:pw@host.example/f?token=abc went wrong");
        let text = describe_failure(&e);
        assert!(!text.contains("pw") && !text.contains("token"), "{text}");
        assert!(text.contains("<url>"));
    }

    #[test]
    fn drop_reason_keeps_first_reason() {
        let state = AdapterWorkerState::new(None, reqwest::Client::new());
        state.drop_with_reason("first".into());
        state.drop_with_reason("second".into());
        let p = state.progress(0.0);
        assert!(p.dropped);
        assert_eq!(p.drop_reason.as_deref(), Some("first"));
    }

    #[test]
    fn test_parse_content_range() {
        assert_eq!(
            parse_content_range("bytes 0-0/1234"),
            Some(ContentRange {
                start: 0,
                end: 0,
                total: Some(1234)
            })
        );
        assert_eq!(
            parse_content_range("bytes 100-199/*"),
            Some(ContentRange {
                start: 100,
                end: 199,
                total: None
            })
        );
        assert_eq!(parse_content_range("bytes */1234"), None);
        assert_eq!(parse_content_range("bytes 5-4/10"), None);
        assert_eq!(parse_content_range("bytes 0-10/10"), None);
        assert_eq!(parse_content_range("items 0-1/10"), None);
        assert_eq!(parse_content_range("garbage"), None);
    }

    #[test]
    fn test_new_rejects_zero_config() {
        assert!(DownloadEngine::new(0, 4).is_err());
        assert!(DownloadEngine::new(1024, 0).is_err());
        let e = DownloadEngine::new(1024, 2).unwrap();
        assert_eq!(e.chunk_size(), 1024);
        assert_eq!(e.connections_per_adapter(), 2);
    }

    #[test]
    fn test_download_cancelled_display_and_downcast() {
        let err = cancelled_error();
        assert_eq!(err.to_string(), "download cancelled");
        assert!(err.downcast_ref::<DownloadCancelled>().is_some());
    }

    #[test]
    fn test_select_bind_targets() {
        let mk = |ip: &str, enabled: bool, loopback: bool| NetworkAdapter {
            id: ip.to_string(),
            name: String::new(),
            ip: ip.parse().unwrap(),
            is_ipv4: ip.contains('.'),
            is_loopback: loopback,
            enabled,
        };
        let unbound = vec![BindTarget::default()];
        let bound = |ip: &str, interface: Option<&str>| BindTarget {
            ip: Some(ip.parse().unwrap()),
            interface: interface.map(str::to_string),
        };
        assert_eq!(DownloadEngine::select_bind_targets(&[]), unbound);
        let adapters = [
            mk("192.168.1.2", true, false),
            mk("10.0.0.2", false, false),
            mk("127.0.0.1", true, true),
            mk("fe80::1", true, false),
        ];
        assert_eq!(
            DownloadEngine::select_bind_targets(&adapters),
            vec![bound("192.168.1.2", None)]
        );
        assert_eq!(DownloadEngine::select_bind_targets(&adapters[1..]), unbound);

        // A non-empty interface name is carried through for egress pinning.
        let named = NetworkAdapter {
            name: "wlan0".into(),
            ..mk("10.1.1.5", true, false)
        };
        assert_eq!(
            DownloadEngine::select_bind_targets(&[named, mk("10.2.2.2", true, false)]),
            vec![bound("10.1.1.5", Some("wlan0")), bound("10.2.2.2", None)]
        );
    }

    fn probe_with(etag: Option<&str>, last_modified: Option<&str>) -> DownloadProbe {
        DownloadProbe {
            url: "http://x/y".into(),
            total_bytes: 10,
            supports_ranges: true,
            suggested_filename: "y".into(),
            etag: etag.map(str::to_string),
            last_modified: last_modified.map(str::to_string),
            headers: None,
        }
    }

    #[test]
    fn test_if_range_validator_prefers_strong_etag() {
        let lm = "Wed, 21 Oct 2015 07:28:00 GMT";
        let v = |e, l| if_range_validator(&probe_with(e, l));
        assert_eq!(v(Some("\"a\""), Some(lm)).as_deref(), Some("\"a\""));
        assert_eq!(v(Some("W/\"a\""), Some(lm)).as_deref(), Some(lm));
        assert_eq!(v(Some("W/\"a\""), None), None);
        assert_eq!(v(None, Some(lm)).as_deref(), Some(lm));
        assert_eq!(v(None, None), None);
    }

    #[test]
    fn test_adapter_state_ids_are_unique() {
        let client = reqwest::Client::new();
        let a = AdapterWorkerState::new(None, client.clone());
        let b = AdapterWorkerState::new(None, client);
        assert_eq!(a.label, b.label);
        assert_ne!(a.id, b.id);
    }
}
