use crate::adapter::{build_bound_http_client, NetworkAdapter, DEFAULT_STALL_TIMEOUT};
use crate::checksum::compute_sha256;
use crate::chunk::{plan_chunks, Chunk, ChunkScheduler, ChunkStatus, Claim, FailOutcome};
use crate::filename::derive_filename;
use crate::resume::{
    decode_ranges, encode_ranges, read_sidecar, remove_resume_sidecar, resume_sidecar_path,
    validate, write_sidecar, ResumeState, SIDECAR_VERSION,
};
use crate::writer::SparseFileWriter;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use reqwest::{header, StatusCode};
use std::fmt;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch, Notify};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Maximum attempts per chunk before it is marked `Failed`.
pub const MAX_CHUNK_ATTEMPTS: u32 = 5;

/// An adapter is dropped for the rest of a download after this many consecutive failed
/// attempts (with no successful chunk in between).
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

#[derive(Debug, Clone)]
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
}

impl Default for DownloadEngine {
    fn default() -> Self {
        Self {
            chunk_size: 4 * 1024 * 1024, // 4 MB default chunk size
            connections_per_adapter: 4,
            retry_backoff: DEFAULT_RETRY_BACKOFF,
            stall_timeout: DEFAULT_STALL_TIMEOUT,
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

/// Per-adapter state shared by that adapter's workers.
struct AdapterWorkerState {
    label: String,
    ip: Option<IpAddr>,
    client: reqwest::Client,
    consecutive_failures: AtomicU32,
    dropped: AtomicBool,
    /// Gross body bytes received (never decremented; see `AdapterProgress`).
    received: AtomicU64,
    active: AtomicUsize,
}

impl AdapterWorkerState {
    fn new(ip: Option<IpAddr>, client: reqwest::Client) -> Self {
        Self {
            label: DownloadEngine::bind_label(ip),
            ip,
            client,
            consecutive_failures: AtomicU32::new(0),
            dropped: AtomicBool::new(false),
            received: AtomicU64::new(0),
            active: AtomicUsize::new(0),
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
        }
    }
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
    writer: SparseFileWriter,
    scheduler: Mutex<ChunkScheduler>,
    counters: Arc<Counters>,
    stop_tx: watch::Sender<bool>,
    last_error: Mutex<Option<String>>,
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

/// Signals workers to stop if `download()` is dropped before finishing.
struct StopOnDrop(Arc<ChunkedShared>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop_tx.send_replace(true);
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
        let parsed_url =
            reqwest::Url::parse(url).with_context(|| format!("Invalid URL: {}", url))?;
        let client = build_bound_http_client(None, self.stall_timeout)?;

        let get = tokio::time::timeout(
            PROBE_TIMEOUT,
            client
                .get(parsed_url.clone())
                .header(header::RANGE, "bytes=0-0")
                .send(),
        )
        .await;

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
            return self.probe_head(&client, url, &parsed_url).await;
        };

        let status = resp.status();
        let headers = resp.headers().clone();
        let final_url = resp.url().clone();
        info!(
            "Probe GET {} -> {} (remote {:?}, Content-Range {:?}, Content-Length {:?})",
            url,
            status,
            resp.remote_addr(),
            header_str(&headers, header::CONTENT_RANGE),
            header_str(&headers, header::CONTENT_LENGTH)
        );
        // Dropping the response without reading the body closes/reuses the connection;
        // for a 200 full-body answer this aborts the transfer.
        drop(resp);

        let suggested_filename = derive_filename(
            header_str(&headers, header::CONTENT_DISPOSITION),
            &final_url,
        );
        let etag = header_str(&headers, header::ETAG).map(str::to_string);
        let last_modified = header_str(&headers, header::LAST_MODIFIED).map(str::to_string);

        let (total_bytes, supports_ranges) = if status == StatusCode::PARTIAL_CONTENT {
            match header_str(&headers, header::CONTENT_RANGE).and_then(parse_content_range) {
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
            (content_length_header(&headers).unwrap_or(0), false)
        } else {
            bail!("Remote server returned non-success status: {}", status);
        };

        Ok(DownloadProbe {
            url: url.to_string(),
            total_bytes,
            supports_ranges,
            suggested_filename,
            etag,
            last_modified,
        })
    }

    async fn probe_head(
        &self,
        client: &reqwest::Client,
        url: &str,
        parsed_url: &reqwest::Url,
    ) -> Result<DownloadProbe> {
        let resp = tokio::time::timeout(PROBE_TIMEOUT, client.head(parsed_url.clone()).send())
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
                header_str(headers, header::CONTENT_DISPOSITION),
                resp.url(),
            ),
            etag: header_str(headers, header::ETAG).map(str::to_string),
            last_modified: header_str(headers, header::LAST_MODIFIED).map(str::to_string),
        })
    }

    /// Selects the adapters to bind to: enabled, non-loopback IPv4. Empty ⇒ `[None]`
    /// (unbound default OS routing).
    fn select_bind_ips(adapters: &[NetworkAdapter]) -> Vec<Option<IpAddr>> {
        let ips: Vec<Option<IpAddr>> = adapters
            .iter()
            .filter(|a| a.enabled && a.is_ipv4 && !a.is_loopback && a.ip.is_ipv4())
            .map(|a| Some(a.ip))
            .collect();
        if ips.is_empty() {
            vec![None]
        } else {
            ips
        }
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
    pub async fn download(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        adapters: &[NetworkAdapter],
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        cancel: watch::Receiver<bool>,
    ) -> Result<String> {
        self.run(probe, output_path, adapters, progress_tx, cancel, false)
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
        self.run(probe, output_path, adapters, progress_tx, cancel, true)
            .await
    }

    async fn run(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        adapters: &[NetworkAdapter],
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        mut cancel: watch::Receiver<bool>,
        resume: bool,
    ) -> Result<String> {
        if *cancel.borrow_and_update() {
            return Err(cancelled_error());
        }

        let bind_ips = Self::select_bind_ips(adapters);
        info!(
            "Download {} -> {:?} (size {} bytes, ranges {}, adapters {:?})",
            probe.url,
            output_path,
            probe.total_bytes,
            probe.supports_ranges,
            bind_ips
                .iter()
                .map(|ip| Self::bind_label(*ip))
                .collect::<Vec<_>>()
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
                .download_single_stream(probe, output_path, bind_ips[0], progress_tx, cancel)
                .await;
        }

        self.download_chunked(probe, output_path, &bind_ips, progress_tx, cancel, resume)
            .await
    }

    /// Opens `output_path` for resuming from its sidecar. Returns the chunk plan (with
    /// recorded chunks marked `Completed`), the chunk size used, and the writer.
    async fn open_for_resume(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
    ) -> Result<(Vec<Chunk>, u64, SparseFileWriter)> {
        let state = read_sidecar(&resume_sidecar_path(output_path))?;
        validate(&state, probe)?;
        let mut chunks = plan_chunks(probe.total_bytes, state.chunk_size);
        let done = decode_ranges(&state.completed, chunks.len())?;
        let writer = SparseFileWriter::open_existing(output_path, probe.total_bytes).await?;
        for id in done {
            // `plan_chunks` ids are their positions.
            chunks[id].status = ChunkStatus::Completed;
            chunks[id].downloaded = chunks[id].size;
        }
        Ok((chunks, state.chunk_size, writer))
    }

    async fn download_chunked(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        bind_ips: &[Option<IpAddr>],
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        mut cancel: watch::Receiver<bool>,
        resume: bool,
    ) -> Result<String> {
        let mut adapter_states = Vec::new();
        for ip in bind_ips {
            match build_bound_http_client(*ip, self.stall_timeout) {
                Ok(client) => adapter_states.push(Arc::new(AdapterWorkerState::new(*ip, client))),
                Err(e) => error!(
                    "Failed to build HTTP client bound to {}: {:#}",
                    Self::bind_label(*ip),
                    e
                ),
            }
        }
        if adapter_states.is_empty() {
            bail!("Could not build an HTTP client for any selected network adapter");
        }

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
        let (chunks, chunk_size, writer) = match resumed {
            Some(r) => r,
            None => {
                // Remove any stale sidecar *before* truncating, so it can never describe
                // the new, empty file.
                remove_resume_sidecar(output_path).with_context(|| {
                    format!("Failed to remove stale resume data for {:?}", output_path)
                })?;
                let chunks = plan_chunks(probe.total_bytes, self.chunk_size);
                if chunks.is_empty() {
                    bail!(
                        "Refusing to download: planned zero chunks for {} bytes (chunk size {})",
                        probe.total_bytes,
                        self.chunk_size
                    );
                }
                let writer = SparseFileWriter::create(output_path, probe.total_bytes).await?;
                (chunks, self.chunk_size, writer)
            }
        };
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
            writer: writer.clone(),
            scheduler: Mutex::new(ChunkScheduler::new(
                chunks,
                MAX_CHUNK_ATTEMPTS,
                self.retry_backoff,
            )),
            counters: Arc::clone(&counters),
            stop_tx,
            last_error: Mutex::new(None),
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
            adapter_states.clone(),
            Some(Arc::clone(&shared)),
            probe.total_bytes,
            total_chunks,
        );

        let mut handles = Vec::new();
        for adapter in &adapter_states {
            for worker_idx in 0..self.connections_per_adapter {
                handles.push(tokio::spawn(run_chunk_worker(
                    Arc::clone(&shared),
                    Arc::clone(adapter),
                    worker_idx,
                    stop_rx.clone(),
                )));
            }
        }

        let workers = futures_util::future::join_all(handles);
        tokio::pin!(workers);
        let (results, user_cancelled) = tokio::select! {
            results = &mut workers => (results, false),
            _ = wait_for_true(&mut cancel) => {
                info!("Cancel requested; stopping all workers");
                shared.stop_tx.send_replace(true);
                ((&mut workers).await, true)
            }
        };
        for r in results {
            if let Err(e) = r {
                error!("Worker task panicked: {:?}", e);
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
        if user_cancelled || !shared.scheduler().all_completed() {
            // Record the final state so a later `resume` loses at most in-flight chunks.
            persist_sidecar(&shared, &sidecar, &mut last_persisted).await;
        }

        if user_cancelled {
            return Err(cancelled_error());
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
                let dropped: Vec<&str> = adapter_states
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
                if dropped.len() == adapter_states.len() {
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
        if let Err(e) = remove_resume_sidecar(output_path) {
            warn!(
                "Failed to remove resume sidecar for {:?}: {}",
                output_path, e
            );
        }

        let duration = start_time.elapsed().as_secs_f64();
        info!(
            "Download completed in {:.2}s (avg {:.2} MB/s)",
            duration,
            probe.total_bytes as f64 / duration.max(1e-9) / (1024.0 * 1024.0)
        );

        let sha256 = compute_sha256(output_path).await?;
        info!("File SHA-256: {}", sha256);
        Ok(sha256)
    }

    async fn download_single_stream(
        &self,
        probe: &DownloadProbe,
        output_path: &Path,
        bind_ip: Option<IpAddr>,
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<String> {
        let client = build_bound_http_client(bind_ip, self.stall_timeout)?;
        let adapter = Arc::new(AdapterWorkerState::new(bind_ip, client));
        let label = adapter.label.clone();

        let resp = tokio::select! {
            biased;
            _ = wait_for_true(&mut cancel) => return Err(cancelled_error()),
            r = adapter.client.get(&probe.url).send() => r.with_context(|| format!("Single-stream request to {} via {} failed", probe.url, label))?,
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
            vec![Arc::clone(&adapter)],
            None,
            total_for_progress,
            1,
        );

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
        let sha256 = compute_sha256(output_path).await?;
        info!("File SHA-256: {}", sha256);
        Ok(sha256)
    }
}

/// One worker: repeatedly claims a chunk, downloads it on `adapter`, and reports the result.
async fn run_chunk_worker(
    shared: Arc<ChunkedShared>,
    adapter: Arc<AdapterWorkerState>,
    worker_idx: usize,
    mut stop: watch::Receiver<bool>,
) {
    loop {
        if *stop.borrow() || adapter.dropped.load(Ordering::SeqCst) {
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
        let result = fetch_chunk(&adapter, &shared, &chunk, &mut counted, &mut stop).await;
        adapter.active.fetch_sub(1, Ordering::SeqCst);
        shared.counters.active.fetch_sub(1, Ordering::SeqCst);

        match result {
            Ok(()) => {
                shared.scheduler().complete(chunk.id);
                shared.counters.completed.fetch_add(1, Ordering::SeqCst);
                adapter.consecutive_failures.store(0, Ordering::SeqCst);
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
                    if !adapter.dropped.swap(true, Ordering::SeqCst) {
                        error!(
                            "Dropping adapter {} for this download after {} consecutive failures",
                            adapter.label, failures
                        );
                    }
                    return;
                }
            }
        }
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
) -> Result<(), AttemptError> {
    let range = chunk.to_range_header();
    let request = adapter
        .client
        .get(&shared.url)
        .header(header::RANGE, &range)
        .send();

    let resp = tokio::select! {
        biased;
        _ = wait_for_true(stop) => return Err(AttemptError::Stopped),
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

    if status != StatusCode::PARTIAL_CONTENT {
        return Err(AttemptError::Failed(anyhow!(
            "expected 206 Partial Content for {}, got {}",
            range,
            status
        )));
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
/// notified. Returns the last completed set it recorded.
async fn run_sidecar_persister(
    shared: Arc<ChunkedShared>,
    sidecar: Arc<SidecarTarget>,
    done: Arc<Notify>,
) -> Option<Vec<usize>> {
    let mut last = None;
    persist_sidecar(&shared, &sidecar, &mut last).await;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(SIDECAR_INTERVAL) => {}
            _ = done.notified() => return last,
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
    adapters: Vec<Arc<AdapterWorkerState>>,
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
        let snapshot =
            |speed: f64, eta: u64, adapter_speeds: &[f64], map: Option<String>| ProgressUpdate {
                downloaded_bytes: counters.downloaded.load(Ordering::SeqCst),
                total_bytes,
                speed_bytes_sec: speed,
                active_chunks: counters.active.load(Ordering::SeqCst),
                completed_chunks: counters.completed.load(Ordering::SeqCst),
                total_chunks,
                eta_seconds: eta,
                adapters: adapters
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
        let mut adapter_last: Vec<u64> = adapters
            .iter()
            .map(|a| a.received.load(Ordering::SeqCst))
            .collect();
        let mut adapter_ema: Vec<Option<f64>> = vec![None; adapters.len()];
        let mut tick: u32 = 0;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(PROGRESS_INTERVAL) => {}
                _ = done_rx.notified() => {
                    let zeros = vec![0.0; adapters.len()];
                    let final_update = snapshot(0.0, 0, &zeros, chunk_map());
                    let _ = tokio::time::timeout(Duration::from_secs(1), tx.send(final_update)).await;
                    return;
                }
            }

            let now = Instant::now();
            let current = counters.downloaded.load(Ordering::SeqCst);
            let elapsed = now.duration_since(last_instant).as_secs_f64();
            if elapsed > 0.0 {
                let instant_speed = current.saturating_sub(last_bytes) as f64 / elapsed;
                speed_ema = Some(ema(speed_ema, instant_speed));
                for (i, a) in adapters.iter().enumerate() {
                    let received = a.received.load(Ordering::SeqCst);
                    let sample = received.saturating_sub(adapter_last[i]) as f64 / elapsed;
                    adapter_ema[i] = Some(ema(adapter_ema[i], sample));
                    adapter_last[i] = received;
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
            let adapter_speeds: Vec<f64> = adapter_ema.iter().map(|s| s.unwrap_or(0.0)).collect();
            let map = if tick.is_multiple_of(CHUNK_MAP_EVERY_N_UPDATES) {
                chunk_map()
            } else {
                None
            };
            tick = tick.wrapping_add(1);

            match tx.try_send(snapshot(speed, eta, &adapter_speeds, map)) {
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
    fn test_select_bind_ips() {
        let mk = |ip: &str, enabled: bool, loopback: bool| NetworkAdapter {
            id: ip.to_string(),
            ip: ip.parse().unwrap(),
            is_ipv4: ip.contains('.'),
            is_loopback: loopback,
            enabled,
        };
        assert_eq!(DownloadEngine::select_bind_ips(&[]), vec![None]);
        let adapters = [
            mk("192.168.1.2", true, false),
            mk("10.0.0.2", false, false),
            mk("127.0.0.1", true, true),
            mk("fe80::1", true, false),
        ];
        assert_eq!(
            DownloadEngine::select_bind_ips(&adapters),
            vec![Some("192.168.1.2".parse().unwrap())]
        );
        assert_eq!(DownloadEngine::select_bind_ips(&adapters[1..]), vec![None]);
    }
}
