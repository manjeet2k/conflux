//! Tiny hand-written HTTP/1.1 test server on `tokio::net::TcpListener`.
//!
//! One request per connection (`Connection: close`). Behaviour is configured per test via
//! [`ServerConfig`] to inject faults: ignored ranges, bad statuses, dropped connections,
//! stalls, wrong Content-Range, overlong bodies, per-peer-IP failures, throttling, and a
//! same-size content change mid-download (with `If-Range` evaluated like a real server).

#![allow(dead_code)]

use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// How the server responds to ranged GETs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeMode {
    /// Proper 206 responses with Content-Range.
    Honor,
    /// Ignore `Range` entirely: always 200 + full body.
    Ignore,
    /// 206 for the `bytes=0-0` probe, but 200 + full body for every other ranged GET.
    ProbeOnly,
    /// 206 with a Content-Range shifted by one byte from what was requested.
    WrongContentRange,
    /// 206 with correct headers but the body has extra trailing bytes (no Content-Length).
    Overlong,
}

#[derive(Clone)]
pub struct ServerConfig {
    pub payload: Arc<Vec<u8>>,
    pub range_mode: RangeMode,
    /// Send `Content-Length` on 200 responses.
    pub send_content_length: bool,
    /// For the first N chunk requests: send headers + half the body, then close.
    pub drop_first_n_chunks: usize,
    /// For the first N chunk requests: send headers, then hang (usize::MAX = always).
    pub stall_first_n_chunks: usize,
    /// Respond with this status to every chunk request (not the probe).
    pub chunk_status: Option<u16>,
    /// Respond 503 to chunk requests coming from this peer IP.
    pub fail_peer_ip: Option<IpAddr>,
    /// Sleep this long after each body write of `write_piece` bytes (throttling).
    pub throttle: Option<Duration>,
    pub write_piece: usize,
    pub content_disposition: Option<String>,
    /// Respond with this status (empty body) to every request, including HEAD.
    pub all_status: Option<u16>,
    /// Respond 405 to every GET (HEAD still works).
    pub get_status_405: bool,
    /// `ETag` header sent with every 200/206 response (including HEAD).
    pub etag: Option<String>,
    /// `Last-Modified` header sent with every 200/206 response (including HEAD).
    pub last_modified: Option<String>,
    /// Replace the served resource from chunk request number `after_chunks` on.
    pub content_change: Option<ContentChange>,
    /// Evaluate `If-Range` on chunk requests (mismatch => 200 + full body), like RFC 9110.
    /// `false` simulates a server that ignores the header.
    pub honor_if_range: bool,
    /// Advertise this total in the probe's `Content-Range` instead of the real length.
    pub probe_total_override: Option<u64>,
}

/// A new version of the resource, served from the `after_chunks`-th chunk request on
/// (0-based; the probe always sees the original).
#[derive(Clone)]
pub struct ContentChange {
    pub after_chunks: usize,
    pub payload: Arc<Vec<u8>>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl ServerConfig {
    pub fn new(payload: Arc<Vec<u8>>) -> Self {
        Self {
            payload,
            range_mode: RangeMode::Honor,
            send_content_length: true,
            drop_first_n_chunks: 0,
            stall_first_n_chunks: 0,
            chunk_status: None,
            fail_peer_ip: None,
            throttle: None,
            write_piece: 16 * 1024,
            content_disposition: None,
            all_status: None,
            get_status_405: false,
            etag: None,
            last_modified: None,
            content_change: None,
            honor_if_range: true,
            probe_total_override: None,
        }
    }

    /// The resource version served for chunk request `chunk_index` (`None` = not a chunk).
    fn version(&self, chunk_index: Option<usize>) -> Version<'_> {
        match (&self.content_change, chunk_index) {
            (Some(c), Some(n)) if n >= c.after_chunks => Version {
                payload: &c.payload,
                etag: c.etag.as_deref(),
                last_modified: c.last_modified.as_deref(),
            },
            _ => Version {
                payload: &self.payload,
                etag: self.etag.as_deref(),
                last_modified: self.last_modified.as_deref(),
            },
        }
    }
}

/// One version of the served resource and its validators.
#[derive(Clone, Copy)]
struct Version<'a> {
    payload: &'a Arc<Vec<u8>>,
    etag: Option<&'a str>,
    last_modified: Option<&'a str>,
}

impl Version<'_> {
    /// `If-Range` matches only a strong ETag or the exact Last-Modified date.
    fn if_range_matches(&self, value: &str) -> bool {
        let strong_etag = self.etag.filter(|e| !e.starts_with("W/"));
        strong_etag == Some(value) || self.last_modified == Some(value)
    }

    fn validator_headers(&self, headers: &mut Vec<(String, String)>) {
        if let Some(etag) = self.etag {
            headers.push(("ETag".to_string(), etag.to_string()));
        }
        if let Some(lm) = self.last_modified {
            headers.push(("Last-Modified".to_string(), lm.to_string()));
        }
    }
}

#[derive(Default)]
pub struct Stats {
    pub requests: AtomicUsize,
    pub chunk_requests: AtomicUsize,
    pub head_requests: AtomicUsize,
    pub body_bytes_sent: AtomicU64,
    pub requests_by_peer_127_0_0_2: AtomicUsize,
    /// `If-Range` value of every chunk request that carried one.
    pub if_range_values: Mutex<Vec<String>>,
}

pub struct TestServer {
    pub addr: SocketAddr,
    pub stats: Arc<Stats>,
}

impl TestServer {
    pub async fn start(config: ServerConfig) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let stats = Arc::new(Stats::default());
        let config = Arc::new(config);
        let stats_bg = Arc::clone(&stats);
        tokio::spawn(async move {
            loop {
                let Ok((sock, peer)) = listener.accept().await else {
                    return;
                };
                let config = Arc::clone(&config);
                let stats = Arc::clone(&stats_bg);
                tokio::spawn(async move {
                    let _ = handle(sock, peer, &config, &stats).await;
                });
            }
        });
        Self { addr, stats }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

/// Deterministic pseudo-random payload (xorshift64).
pub fn payload(len: usize, seed: u64) -> Arc<Vec<u8>> {
    let mut x = seed | 1;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        for b in x.to_le_bytes() {
            if out.len() < len {
                out.push(b);
            }
        }
    }
    Arc::new(out)
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

struct Request {
    method: String,
    range: Option<String>,
    if_range: Option<String>,
}

async fn read_request(sock: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = sock.read(&mut tmp).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 64 * 1024 {
            return Ok(None);
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let method = request_line.split(' ').next().unwrap_or("").to_string();
    let mut range = None;
    let mut if_range = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            } else if name.trim().eq_ignore_ascii_case("if-range") {
                if_range = Some(value.trim().to_string());
            }
        }
    }
    Ok(Some(Request {
        method,
        range,
        if_range,
    }))
}

/// Parses `bytes=a-b` (or `bytes=a-`) against `len`. `Err(())` = unsatisfiable.
fn parse_range(value: &str, len: u64) -> Result<(u64, u64), ()> {
    let spec = value.strip_prefix("bytes=").ok_or(())?;
    let (a, b) = spec.split_once('-').ok_or(())?;
    let start: u64 = a.trim().parse().map_err(|_| ())?;
    let end: u64 = if b.trim().is_empty() {
        len.saturating_sub(1)
    } else {
        b.trim().parse().map_err(|_| ())?
    };
    if len == 0 || start >= len || end < start {
        return Err(());
    }
    Ok((start, end.min(len - 1)))
}

fn status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        416 => "Range Not Satisfiable",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

async fn write_head(
    sock: &mut TcpStream,
    code: u16,
    headers: &[(String, String)],
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nConnection: close\r\n",
        code,
        status_text(code)
    );
    for (k, v) in headers {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("\r\n");
    sock.write_all(head.as_bytes()).await
}

async fn write_body(
    sock: &mut TcpStream,
    body: &[u8],
    config: &ServerConfig,
    stats: &Stats,
) -> std::io::Result<()> {
    for piece in body.chunks(config.write_piece.max(1)) {
        sock.write_all(piece).await?;
        stats
            .body_bytes_sent
            .fetch_add(piece.len() as u64, Ordering::SeqCst);
        if let Some(d) = config.throttle {
            tokio::time::sleep(d).await;
        }
    }
    Ok(())
}

async fn send_simple(sock: &mut TcpStream, code: u16) -> std::io::Result<()> {
    write_head(sock, code, &[("Content-Length".into(), "0".into())]).await
}

async fn send_full(
    sock: &mut TcpStream,
    config: &ServerConfig,
    version: Version<'_>,
    stats: &Stats,
    with_body: bool,
) -> std::io::Result<()> {
    let data = version.payload;
    let mut headers = Vec::new();
    if config.send_content_length {
        headers.push(("Content-Length".to_string(), data.len().to_string()));
    }
    if let Some(cd) = &config.content_disposition {
        headers.push(("Content-Disposition".to_string(), cd.clone()));
    }
    version.validator_headers(&mut headers);
    write_head(sock, 200, &headers).await?;
    if with_body {
        write_body(sock, data, config, stats).await?;
    }
    Ok(())
}

async fn handle(
    mut sock: TcpStream,
    peer: SocketAddr,
    config: &ServerConfig,
    stats: &Stats,
) -> std::io::Result<()> {
    let Some(req) = read_request(&mut sock).await? else {
        return Ok(());
    };
    stats.requests.fetch_add(1, Ordering::SeqCst);
    if peer.ip() == "127.0.0.2".parse::<IpAddr>().unwrap() {
        stats
            .requests_by_peer_127_0_0_2
            .fetch_add(1, Ordering::SeqCst);
    }
    let len = config.payload.len() as u64;

    if let Some(code) = config.all_status {
        send_simple(&mut sock, code).await?;
        return sock.shutdown().await;
    }
    if config.get_status_405 && req.method == "GET" {
        send_simple(&mut sock, 405).await?;
        return sock.shutdown().await;
    }

    if req.method == "HEAD" {
        stats.head_requests.fetch_add(1, Ordering::SeqCst);
        send_full(&mut sock, config, config.version(None), stats, false).await?;
        return sock.shutdown().await;
    }

    let Some(range) = req.range.clone() else {
        send_full(&mut sock, config, config.version(None), stats, true).await?;
        return sock.shutdown().await;
    };

    let is_probe = range == "bytes=0-0";
    let mut chunk_index = None;
    if !is_probe {
        let n = stats.chunk_requests.fetch_add(1, Ordering::SeqCst);
        chunk_index = Some(n);

        if let Some(code) = config.chunk_status {
            send_simple(&mut sock, code).await?;
            return sock.shutdown().await;
        }
        if config.fail_peer_ip == Some(peer.ip()) {
            send_simple(&mut sock, 503).await?;
            return sock.shutdown().await;
        }
        if n < config.stall_first_n_chunks {
            if let Ok((start, end)) = parse_range(&range, len) {
                write_head(
                    &mut sock,
                    206,
                    &[
                        ("Content-Length".into(), (end - start + 1).to_string()),
                        (
                            "Content-Range".into(),
                            format!("bytes {}-{}/{}", start, end, len),
                        ),
                    ],
                )
                .await?;
            }
            tokio::time::sleep(Duration::from_secs(120)).await;
            return Ok(());
        }
    }
    let version = config.version(chunk_index);
    if let (Some(_), Some(value)) = (chunk_index, &req.if_range) {
        stats.if_range_values.lock().unwrap().push(value.clone());
        if config.honor_if_range && !version.if_range_matches(value) {
            // The validator no longer matches: send the whole (new) representation.
            send_full(&mut sock, config, version, stats, true).await?;
            return sock.shutdown().await;
        }
    }

    match config.range_mode {
        RangeMode::Ignore => {
            send_full(&mut sock, config, version, stats, true).await?;
            return sock.shutdown().await;
        }
        RangeMode::ProbeOnly if !is_probe => {
            send_full(&mut sock, config, version, stats, true).await?;
            return sock.shutdown().await;
        }
        _ => {}
    }

    let (start, end) = match parse_range(&range, len) {
        Ok(r) => r,
        Err(()) => {
            write_head(
                &mut sock,
                416,
                &[
                    ("Content-Range".into(), format!("bytes */{}", len)),
                    ("Content-Length".into(), "0".into()),
                ],
            )
            .await?;
            return sock.shutdown().await;
        }
    };
    let body = &version.payload[start as usize..=end as usize];

    let mut headers = Vec::new();
    let (cr_start, cr_end) = if config.range_mode == RangeMode::WrongContentRange && !is_probe {
        (start + 1, (end + 1).min(len - 1))
    } else {
        (start, end)
    };
    let advertised_total = match (is_probe, config.probe_total_override) {
        (true, Some(t)) => t,
        _ => len,
    };
    headers.push((
        "Content-Range".to_string(),
        format!("bytes {}-{}/{}", cr_start, cr_end, advertised_total),
    ));
    if let Some(cd) = &config.content_disposition {
        headers.push(("Content-Disposition".to_string(), cd.clone()));
    }
    version.validator_headers(&mut headers);
    let overlong = config.range_mode == RangeMode::Overlong && !is_probe;
    if !overlong {
        headers.push(("Content-Length".to_string(), body.len().to_string()));
    }
    write_head(&mut sock, 206, &headers).await?;

    if chunk_index.is_some_and(|n| n < config.drop_first_n_chunks) {
        // Send half the body then drop the connection (simulated disconnect).
        let half = &body[..body.len() / 2];
        write_body(&mut sock, half, config, stats).await?;
        return Ok(()); // socket dropped without completing the body
    }

    write_body(&mut sock, body, config, stats).await?;
    if overlong {
        write_body(&mut sock, b"EXTRA-BYTES-BEYOND-RANGE", config, stats).await?;
    }
    sock.shutdown().await
}
