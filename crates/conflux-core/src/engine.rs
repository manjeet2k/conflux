use crate::adapter::{build_bound_http_client, NetworkAdapter};
use crate::checksum::compute_sha256;
use crate::chunk::{plan_chunks, ChunkStatus};
use crate::writer::SparseFileWriter;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use std::net::IpAddr;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct DownloadProbe {
    pub url: String,
    pub total_bytes: u64,
    pub supports_ranges: bool,
    pub suggested_filename: String,
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
}

pub struct DownloadEngine {
    chunk_size: u64,
    connections_per_adapter: usize,
}

impl Default for DownloadEngine {
    fn default() -> Self {
        Self {
            chunk_size: 4 * 1024 * 1024, // 4 MB default chunk size
            connections_per_adapter: 4,
        }
    }
}

impl DownloadEngine {
    pub fn new(chunk_size: u64, connections_per_adapter: usize) -> Self {
        Self {
            chunk_size,
            connections_per_adapter,
        }
    }

    /// Probes the target URL using a HEAD request to check range support and file size.
    pub async fn probe(&self, url: &str) -> Result<DownloadProbe> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let resp = client
            .head(url)
            .send()
            .await
            .with_context(|| format!("Failed to send HEAD request to {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            bail!("Remote server returned non-success status: {}", status);
        }

        let headers = resp.headers();

        let total_bytes = headers
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|val| val.to_str().ok())
            .and_then(|val| val.parse::<u64>().ok())
            .unwrap_or(0);

        let supports_ranges = headers
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|val| val.to_str().ok())
            .map(|val| val.to_lowercase().contains("bytes"))
            .unwrap_or(false);

        // Derive suggested filename from Content-Disposition or URL path
        let mut filename = "download.bin".to_string();
        if let Some(cd) = headers
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|h| h.to_str().ok())
        {
            if let Some(idx) = cd.find("filename=") {
                let part = &cd[idx + 9..].trim_matches('"');
                if let Some(end) = part.find(';') {
                    filename = part[..end].trim().to_string();
                } else {
                    filename = part.to_string();
                }
            }
        } else if let Ok(parsed_url) = reqwest::Url::parse(url) {
            if let Some(mut segments) = parsed_url.path_segments() {
                if let Some(last) = segments.next_back() {
                    if !last.is_empty() {
                        filename = last.to_string();
                    }
                }
            }
        }

        Ok(DownloadProbe {
            url: url.to_string(),
            total_bytes,
            supports_ranges,
            suggested_filename: filename,
        })
    }

    /// Executes the multi-chunk, multi-interface download.
    pub async fn download(
        &self,
        url: &str,
        output_path: &Path,
        adapters: &[NetworkAdapter],
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
    ) -> Result<String> {
        let probe = self.probe(url).await?;
        info!(
            "Probed target: {} (Size: {} bytes, Ranges: {})",
            probe.suggested_filename, probe.total_bytes, probe.supports_ranges
        );

        if probe.total_bytes == 0 {
            bail!("Cannot multi-chunk download a file of 0 bytes or unknown length");
        }

        if !probe.supports_ranges {
            warn!("Server does not support Accept-Ranges. Falling back to single-stream download.");
            return self
                .download_single_stream(url, output_path, progress_tx)
                .await;
        }

        // Plan non-overlapping chunks
        let chunks = plan_chunks(probe.total_bytes, self.chunk_size);
        let total_chunks = chunks.len();
        info!(
            "Planned {} chunks of {} MB each",
            total_chunks,
            self.chunk_size / (1024 * 1024)
        );

        // Create and pre-allocate sparse target file
        let writer = SparseFileWriter::create(output_path, probe.total_bytes).await?;

        // Determine active adapter IPs (filtering loopback unless it's the only one)
        let active_ips: Vec<Option<IpAddr>> = {
            let filtered: Vec<Option<IpAddr>> = adapters
                .iter()
                .filter(|a| a.enabled && !a.is_loopback && a.is_ipv4)
                .map(|a| Some(a.ip))
                .collect();

            if filtered.is_empty() {
                // If no external adapter, allow default OS routing (None)
                vec![None]
            } else {
                filtered
            }
        };

        info!(
            "Channel bonding across {} active network interfaces",
            active_ips.len()
        );

        let chunks_queue = Arc::new(Mutex::new(chunks));
        let total_downloaded = Arc::new(AtomicU64::new(0));
        let active_workers = Arc::new(AtomicU64::new(0));
        let completed_chunks = Arc::new(AtomicU64::new(0));

        let start_time = Instant::now();

        // Spawn worker pool across all adapters
        let mut worker_handles = Vec::new();

        for adapter_ip in &active_ips {
            let adapter_ip = *adapter_ip;
            for worker_idx in 0..self.connections_per_adapter {
                let chunks_queue = Arc::clone(&chunks_queue);
                let total_downloaded = Arc::clone(&total_downloaded);
                let active_workers = Arc::clone(&active_workers);
                let completed_chunks = Arc::clone(&completed_chunks);
                let writer = writer.clone();
                let url = url.to_string();

                let handle = tokio::spawn(async move {
                    let client = match build_bound_http_client(adapter_ip) {
                        Ok(c) => c,
                        Err(e) => {
                            error!(
                                "Failed to build HTTP client for adapter {:?}: {}",
                                adapter_ip, e
                            );
                            return;
                        }
                    };

                    loop {
                        // Work-stealing chunk claim
                        let chunk_to_download = {
                            let mut queue = chunks_queue.lock().await;
                            match queue.iter_mut().find(|c| c.status == ChunkStatus::Pending) {
                                Some(chunk) => {
                                    chunk.status = ChunkStatus::Downloading;
                                    chunk.adapter_ip = adapter_ip.map(|ip| ip.to_string());
                                    Some(chunk.clone())
                                }
                                None => {
                                    let has_inflight =
                                        queue.iter().any(|c| c.status == ChunkStatus::Downloading);
                                    if has_inflight {
                                        None // In-flight chunks remain; wait and retry
                                    } else {
                                        break; // All chunks finished or failed
                                    }
                                }
                            }
                        };

                        let chunk_to_dl = match chunk_to_download {
                            Some(c) => c,
                            None => {
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                continue;
                            }
                        };

                        active_workers.fetch_add(1, Ordering::SeqCst);

                        let range_header = chunk_to_dl.to_range_header();
                        let bytes_before = total_downloaded.load(Ordering::Relaxed);
                        let result = Self::download_chunk(
                            &client,
                            &url,
                            &range_header,
                            chunk_to_dl.start,
                            &writer,
                            &total_downloaded,
                        )
                        .await;

                        active_workers.fetch_sub(1, Ordering::SeqCst);

                        match result {
                            Ok(_) => {
                                completed_chunks.fetch_add(1, Ordering::SeqCst);
                                let mut queue = chunks_queue.lock().await;
                                if let Some(c) = queue.iter_mut().find(|c| c.id == chunk_to_dl.id) {
                                    c.status = ChunkStatus::Completed;
                                }
                            }
                            Err(e) => {
                                error!(
                                    "Worker {} on {:?} failed downloading chunk {}: {}. Re-queueing...",
                                    worker_idx, adapter_ip, chunk_to_dl.id, e
                                );
                                // Revert partial bytes downloaded in this attempt to avoid double counting
                                let bytes_after = total_downloaded.load(Ordering::Relaxed);
                                let bytes_this_attempt = bytes_after.saturating_sub(bytes_before);
                                total_downloaded.fetch_sub(bytes_this_attempt, Ordering::Relaxed);

                                // Re-queue failed chunk for surviving workers to steal
                                let mut queue = chunks_queue.lock().await;
                                if let Some(c) = queue.iter_mut().find(|c| c.id == chunk_to_dl.id) {
                                    c.status = ChunkStatus::Pending;
                                }
                            }
                        }
                    }
                });

                worker_handles.push(handle);
            }
        }

        let done_signal = Arc::new(tokio::sync::Notify::new());
        let done_signal_clone = Arc::clone(&done_signal);

        // Spawn progress monitor loop if progress sender provided
        let progress_handle = if let Some(tx) = progress_tx {
            let total_downloaded = Arc::clone(&total_downloaded);
            let active_workers = Arc::clone(&active_workers);
            let completed_chunks = Arc::clone(&completed_chunks);
            let total_bytes = probe.total_bytes;

            tokio::spawn(async move {
                let mut last_bytes = 0u64;
                let mut last_instant = Instant::now();

                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_millis(200)) => {},
                        _ = done_signal_clone.notified() => {
                            let current_bytes = total_downloaded.load(Ordering::Relaxed);
                            let update = ProgressUpdate {
                                downloaded_bytes: current_bytes,
                                total_bytes,
                                speed_bytes_sec: 0.0,
                                active_chunks: active_workers.load(Ordering::Relaxed) as usize,
                                completed_chunks: completed_chunks.load(Ordering::Relaxed) as usize,
                                total_chunks,
                                eta_seconds: 0,
                            };
                            let _ = tx.send(update).await;
                            break;
                        }
                    }

                    let current_bytes = total_downloaded.load(Ordering::Relaxed);
                    let now = Instant::now();
                    let elapsed = now.duration_since(last_instant).as_secs_f64();

                    let speed = if elapsed > 0.0 {
                        (current_bytes.saturating_sub(last_bytes)) as f64 / elapsed
                    } else {
                        0.0
                    };

                    last_bytes = current_bytes;
                    last_instant = now;

                    let remaining_bytes = total_bytes.saturating_sub(current_bytes);
                    let eta = if speed > 0.0 {
                        (remaining_bytes as f64 / speed) as u64
                    } else {
                        0
                    };

                    let update = ProgressUpdate {
                        downloaded_bytes: current_bytes,
                        total_bytes,
                        speed_bytes_sec: speed,
                        active_chunks: active_workers.load(Ordering::Relaxed) as usize,
                        completed_chunks: completed_chunks.load(Ordering::Relaxed) as usize,
                        total_chunks,
                        eta_seconds: eta,
                    };

                    if tx.send(update).await.is_err() {
                        break;
                    }

                    if current_bytes >= total_bytes {
                        break;
                    }
                }
            })
        } else {
            tokio::spawn(async {})
        };

        // Wait for all worker tasks to finish
        for handle in worker_handles {
            if let Err(e) = handle.await {
                error!("Worker task panicked: {:?}", e);
            }
        }

        // Ensure progress loop terminates
        done_signal.notify_one();
        if let Err(e) = progress_handle.await {
            error!("Progress task panicked: {:?}", e);
        }

        // Verify all chunks completed successfully
        {
            let queue = chunks_queue.lock().await;
            let incomplete: Vec<_> = queue
                .iter()
                .filter(|c| c.status != ChunkStatus::Completed)
                .collect();
            if !incomplete.is_empty() {
                bail!(
                    "Download incomplete: {} of {} chunks failed to complete. Chunk IDs: {:?}",
                    incomplete.len(),
                    total_chunks,
                    incomplete.iter().map(|c| c.id).collect::<Vec<_>>()
                );
            }
        }

        // Ensure all bytes flushed to disk
        writer.sync().await.context("Failed syncing file to disk")?;

        let duration = start_time.elapsed().as_secs_f64();
        let avg_speed = (probe.total_bytes as f64) / duration / (1024.0 * 1024.0);
        info!(
            "Download completed in {:.2}s (Avg Speed: {:.2} MB/s)",
            duration, avg_speed
        );

        // Compute and verify SHA-256
        let sha256 = compute_sha256(output_path).await?;
        info!("File SHA-256: {}", sha256);

        Ok(sha256)
    }

    async fn download_chunk(
        client: &reqwest::Client,
        url: &str,
        range_header: &str,
        start_offset: u64,
        writer: &SparseFileWriter,
        downloaded_counter: &Arc<AtomicU64>,
    ) -> Result<()> {
        let resp = client
            .get(url)
            .header(reqwest::header::RANGE, range_header)
            .send()
            .await
            .with_context(|| format!("Failed to request range {}", range_header))?;

        let status = resp.status();
        if status != reqwest::StatusCode::PARTIAL_CONTENT && status != reqwest::StatusCode::OK {
            bail!(
                "Server returned unexpected status {} for range {}",
                status,
                range_header
            );
        }

        let mut stream = resp.bytes_stream();
        let mut current_offset = start_offset;

        while let Some(chunk_res) = stream.next().await {
            let bytes = chunk_res.context("Error reading bytes from chunk stream")?;
            let len = bytes.len() as u64;

            writer.write_at(current_offset, &bytes).await?;
            current_offset += len;
            downloaded_counter.fetch_add(len, Ordering::Relaxed);
        }

        Ok(())
    }

    async fn download_single_stream(
        &self,
        url: &str,
        output_path: &Path,
        progress_tx: Option<mpsc::Sender<ProgressUpdate>>,
    ) -> Result<String> {
        let client = reqwest::Client::new();
        let resp = client
            .get(url)
            .send()
            .await
            .context("Failed single stream request")?;
        let total_bytes = resp.content_length().unwrap_or(0);

        let mut writer = tokio::fs::File::create(output_path).await?;
        let mut stream = resp.bytes_stream();
        let mut downloaded = 0u64;

        while let Some(item) = stream.next().await {
            let bytes = item?;
            tokio::io::AsyncWriteExt::write_all(&mut writer, &bytes).await?;
            downloaded += bytes.len() as u64;

            if let Some(ref tx) = progress_tx {
                let _ = tx
                    .send(ProgressUpdate {
                        downloaded_bytes: downloaded,
                        total_bytes,
                        speed_bytes_sec: 0.0,
                        active_chunks: 1,
                        completed_chunks: if downloaded == total_bytes { 1 } else { 0 },
                        total_chunks: 1,
                        eta_seconds: 0,
                    })
                    .await;
            }
        }

        tokio::io::AsyncWriteExt::flush(&mut writer).await?;
        let sha256 = compute_sha256(output_path).await?;
        Ok(sha256)
    }
}
