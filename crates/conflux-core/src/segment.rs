use crate::chunk::{Chunk, ChunkStatus};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Minimum block size for dynamic range splitting: 1 MiB.
pub const MIN_BLOCK_SIZE: u64 = 1024 * 1024;

/// Maximum number of blocks for any download: 8,192 blocks.
/// Keeps IPC message size bounded and keeps pause/failure rollback small.
pub const MAX_BLOCKS: u64 = 8192;

/// Default maximum number of attempts before a block range is marked permanently failed.
pub const MAX_CHUNK_ATTEMPTS: u32 = 5;

/// Default initial retry backoff base.
pub const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_millis(500);

/// How long to wait when in-flight segments may soon yield work.
pub const IDLE_POLL: Duration = Duration::from_millis(50);

/// Each request asks for about this much transfer time at the connection's own speed, so
/// fast and slow adapters pull work in proportion to their speed and every response is
/// read to its end (which keeps the connection alive and its TCP window open).
pub const PIECE_TARGET: Duration = Duration::from_secs(4);

/// Smallest request and smallest tail steal, in blocks (4 MiB at the default 1 MiB block).
pub const MIN_PIECE_BLOCKS: u64 = 4;

/// Largest single request.
pub const MAX_PIECE_BYTES: u64 = 256 * 1024 * 1024;

/// A tail steal cuts the victim's live response (its connection is lost), so it must
/// finish the remaining range at least this much sooner to be worth it.
pub const MIN_STEAL_GAIN: Duration = Duration::from_secs(1);

/// The download fails after this many rate-limit pauses in a row with no block committed.
pub const MAX_THROTTLE_ROUNDS: u32 = 8;

/// Upper bound on an honoured `Retry-After`.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);

/// Bytes one request should ask for on a connection measured at `rate` bytes/s
/// (`None`: no measurement yet), clamped to `[MIN_PIECE_BLOCKS blocks, MAX_PIECE_BYTES]`.
pub fn piece_bytes_for(rate: Option<f64>, block_size: u64) -> u64 {
    let min = block_size.saturating_mul(MIN_PIECE_BLOCKS);
    let max = MAX_PIECE_BYTES.max(min);
    match rate {
        Some(r) if r.is_finite() && r > 0.0 => {
            ((r * PIECE_TARGET.as_secs_f64()) as u64).clamp(min, max)
        }
        _ => min,
    }
}

/// Returns the smallest power of two that is at least 1 MiB and yields at most 8,192 blocks.
///
/// Examples:
/// - 0 B .. 8 GiB -> 1 MiB (at most 8,192 blocks)
/// - 20 GiB -> 4 MiB (5,120 blocks)
/// - 100 GiB -> 16 MiB (6,400 blocks)
pub fn block_size_for(total_bytes: u64) -> u64 {
    if total_bytes == 0 {
        return MIN_BLOCK_SIZE;
    }
    let min_needed = total_bytes.div_ceil(MAX_BLOCKS);
    let target = min_needed.max(MIN_BLOCK_SIZE);
    target.checked_next_power_of_two().unwrap_or(1 << 62)
}

/// Dynamic byte bounds of an in-flight segment, shared between worker and scheduler.
#[derive(Debug)]
pub struct SegmentBounds {
    /// Next byte offset accepted and accounted for by the worker.
    pub reserved: u64,
    /// Exclusive upper byte offset; can only shrink when split by a thief.
    pub end: u64,
    /// Measured speed of the worker's connection in bytes/s (`None` until data arrives).
    /// Used to estimate how long the segment still needs when deciding a tail steal.
    pub rate: Option<f64>,
}

impl SegmentBounds {
    pub fn new(reserved: u64, end: u64) -> Self {
        Self {
            reserved,
            end,
            rate: None,
        }
    }

    /// Reserves up to `len` bytes from the remaining segment range.
    ///
    /// Returns `(take, reached_end)`:
    /// - `take`: number of bytes reserved (0 if already at or past `end`).
    /// - `reached_end`: true if after taking, `reserved >= end`.
    pub fn take(&mut self, len: u64) -> (u64, bool) {
        let available = self.end.saturating_sub(self.reserved);
        let take = len.min(available);
        self.reserved += take;
        let reached_end = self.reserved >= self.end;
        (take, reached_end)
    }

    /// Remaining unreserved bytes in this segment.
    pub fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.reserved)
    }
}

/// Lease handed to a worker to stream a continuous segment.
#[derive(Clone, Debug)]
pub struct SegmentLease {
    pub id: usize,
    /// Inclusive starting byte offset of this segment.
    pub start: u64,
    /// Exclusive ending byte offset as requested over HTTP.
    pub end_at_request: u64,
    /// Dynamic bounds shared with the scheduler for work stealing.
    pub bounds: Arc<Mutex<SegmentBounds>>,
    /// Number of attempts previously recorded for this range.
    pub attempts: u32,
    /// Optional label of the network adapter claiming this lease.
    pub worker_label: Option<String>,
}

impl SegmentLease {
    /// Formats the byte range as an HTTP Range header value: `bytes=start-(end_at_request - 1)`
    pub fn to_range_header(&self) -> String {
        let end_inclusive = self.end_at_request.saturating_sub(1);
        format!("bytes={}-{}", self.start, end_inclusive)
    }
}

/// Result of asking the scheduler for work.
#[derive(Debug)]
pub enum Claim {
    /// A segment lease was granted and is now actively downloading.
    Segment(SegmentLease),
    /// No work is immediately claimable; retry after this duration.
    Wait(Duration),
    /// All blocks are completed, or at least one block failed permanently.
    Finished,
}

/// Result of reporting a segment failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOutcome {
    /// The uncommitted range was returned to pending with backoff delay.
    Retry { after: Duration },
    /// Max attempts were exhausted; uncommitted blocks are marked Failed.
    Failed,
}

/// A contiguous range of pending blocks `[start_block, end_block)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRange {
    pub start_block: usize,
    pub end_block: usize,
    pub attempts: u32,
    pub not_before: Option<Instant>,
}

/// Tracking state for an active segment.
#[derive(Debug)]
struct ActiveSegment {
    id: usize,
    start_block: usize,
    end_block: usize,
    bounds: Arc<Mutex<SegmentBounds>>,
    attempts: u32,
    committed_bytes: u64,
    /// When the request was handed out (tells whether it started after a rate-limit pause).
    claimed_at: Instant,
    #[allow(dead_code)]
    worker_label: Option<String>,
}

/// Speed-sized piece scheduler.
///
/// Decouples HTTP request ranges (segments) from accounting units (blocks). Each worker
/// pulls a piece of about [`PIECE_TARGET`] of transfer time at its own connection speed
/// and reads it to the end. Only when nothing is left to pull does an idle worker split
/// an in-flight segment, sized so both finish together. Disk persistence, sidecar
/// recording and retries operate on small blocks.
pub struct SegmentScheduler {
    blocks: Vec<Chunk>,
    active: Vec<ActiveSegment>,
    pending: Vec<PendingRange>,
    next_segment_id: usize,
    max_attempts: u32,
    backoff_base: Duration,
    total_bytes: u64,
    /// No new request may start before this instant (server rate limiting).
    throttled_until: Option<Instant>,
    /// Rate-limit pauses since a request started after a pause committed a block.
    throttle_rounds: u32,
    /// When the latest rate-limit pause began.
    throttle_started: Option<Instant>,
    /// Once the server has rate limited this download, at most one new request starts per
    /// this interval (the latest pause length) for the rest of it, and pieces are fair
    /// shares of the remaining work instead of speed-sized, so few requests are needed.
    request_spacing: Option<Duration>,
    /// The next request may not start before this instant (see `request_spacing`).
    next_claim_at: Option<Instant>,
    /// Workers currently claiming from this scheduler (see [`Self::add_worker`]).
    workers: usize,
}

impl SegmentScheduler {
    /// Creates a scheduler from planned blocks. Non-completed blocks form initial pending ranges.
    pub fn new(blocks: Vec<Chunk>, max_attempts: u32, backoff_base: Duration) -> Self {
        let total_bytes = blocks.last().map(|b| b.end + 1).unwrap_or(0);
        let mut pending = Vec::new();
        let mut range_start = None;

        for (idx, b) in blocks.iter().enumerate() {
            if b.status != ChunkStatus::Completed {
                if range_start.is_none() {
                    range_start = Some(idx);
                }
            } else if let Some(start) = range_start.take() {
                pending.push(PendingRange {
                    start_block: start,
                    end_block: idx,
                    attempts: 0,
                    not_before: None,
                });
            }
        }
        if let Some(start) = range_start {
            pending.push(PendingRange {
                start_block: start,
                end_block: blocks.len(),
                attempts: 0,
                not_before: None,
            });
        }

        Self {
            blocks,
            active: Vec::new(),
            pending,
            next_segment_id: 1,
            max_attempts: max_attempts.max(1),
            backoff_base,
            total_bytes,
            throttled_until: None,
            throttle_rounds: 0,
            throttle_started: None,
            request_spacing: None,
            next_claim_at: None,
            workers: 0,
        }
    }

    /// Registers a worker. Rate-limited downloads split the remaining work evenly among
    /// the idle workers, so the scheduler must know how many there are.
    pub fn add_worker(&mut self) {
        self.workers += 1;
    }

    /// Unregisters a worker that stopped claiming.
    pub fn remove_worker(&mut self) {
        self.workers = self.workers.saturating_sub(1);
    }

    /// Size of every block but possibly the last.
    pub fn block_size(&self) -> u64 {
        self.blocks
            .first()
            .map(|b| b.size)
            .unwrap_or(MIN_BLOCK_SIZE)
    }

    /// Backoff before a failed range may be reclaimed:
    /// base, 2*base, 4*base, 8*base, capped at 16*base.
    pub fn backoff_for(&self, failed_attempts: u32) -> Duration {
        let exponent = failed_attempts.saturating_sub(1).min(4);
        self.backoff_base.saturating_mul(1u32 << exponent)
    }

    /// Requests work for a worker whose connection was last measured at `conn_rate` bytes/s.
    ///
    /// Priority:
    /// 1. If any block is permanently `Failed`, or nothing is left, return `Finished`.
    /// 2. While the server is rate limiting, return `Wait`.
    /// 3. Take a speed-sized piece ([`piece_bytes_for`]) from the start of the
    ///    lowest-offset ready pending range.
    /// 4. Otherwise (tail), split the in-flight segment where that saves the most time.
    /// 5. If work is waiting on backoff or active segments are running, return `Wait`.
    pub fn claim(
        &mut self,
        now: Instant,
        conn_rate: Option<f64>,
        worker_label: Option<String>,
    ) -> Claim {
        if self.blocks.iter().any(|b| b.status == ChunkStatus::Failed) {
            return Claim::Finished;
        }
        if self.pending.is_empty() && self.active.is_empty() {
            return Claim::Finished;
        }
        if let Some(until) = self.throttled_until.filter(|u| *u > now) {
            return Claim::Wait(until - now);
        }
        if let Some(at) = self.next_claim_at.filter(|at| *at > now) {
            return Claim::Wait(at - now);
        }

        let mut best_pending_idx = None;
        let mut soonest_backoff: Option<Duration> = None;
        for (idx, range) in self.pending.iter().enumerate() {
            let ready_in = range
                .not_before
                .map(|t| t.saturating_duration_since(now))
                .unwrap_or(Duration::ZERO);
            if ready_in.is_zero() {
                let lower = best_pending_idx
                    .is_none_or(|b: usize| range.start_block < self.pending[b].start_block);
                if lower {
                    best_pending_idx = Some(idx);
                }
            } else {
                soonest_backoff = Some(soonest_backoff.map_or(ready_in, |s| s.min(ready_in)));
            }
        }

        let lease = match best_pending_idx {
            Some(idx) => Some(self.claim_piece(now, idx, conn_rate, worker_label)),
            None => self.try_split_active(now, conn_rate, worker_label),
        };
        if let Some(lease) = lease {
            self.next_claim_at = self.request_spacing.map(|d| now + d);
            return Claim::Segment(lease);
        }

        match (soonest_backoff, !self.active.is_empty()) {
            (Some(d), true) => Claim::Wait(d.min(IDLE_POLL)),
            (Some(d), false) => Claim::Wait(d),
            (None, true) => Claim::Wait(IDLE_POLL),
            (None, false) => Claim::Finished,
        }
    }

    /// Leases a speed-sized piece from the start of `pending[idx]`.
    fn claim_piece(
        &mut self,
        now: Instant,
        idx: usize,
        conn_rate: Option<f64>,
        worker_label: Option<String>,
    ) -> SegmentLease {
        let min_blocks = MIN_PIECE_BLOCKS as usize;
        let speed_blocks = piece_bytes_for(conn_rate, self.block_size())
            .div_ceil(self.block_size())
            .max(1) as usize;
        // Near the end, take no more than a fair share of what is left, so the last pieces
        // spread over all connections instead of one fast worker taking everything while
        // the others idle (and then have to cut its live response to help).
        let pending_blocks: usize = self
            .pending
            .iter()
            .map(|r| r.end_block - r.start_block)
            .sum();
        let want_blocks = if self.request_spacing.is_some() {
            // Rate-limited server: one long request per connection instead of one per 4 s,
            // the remaining work split evenly among the workers still waiting for some.
            let idle = self.workers.saturating_sub(self.active.len()).max(1);
            pending_blocks.div_ceil(idle).max(min_blocks)
        } else {
            let fair_blocks = pending_blocks.div_ceil(self.active.len() + 1);
            speed_blocks.min(fair_blocks).max(min_blocks)
        };

        let range = &mut self.pending[idx];
        let available = range.end_block - range.start_block;
        // Never leave a remainder smaller than a minimum piece behind: it would become a
        // tiny request of its own later.
        let take = if available < want_blocks + min_blocks {
            available
        } else {
            want_blocks
        };
        let start_block = range.start_block;
        let end_block = start_block + take;
        let attempts = range.attempts;
        if take == available {
            self.pending.remove(idx);
        } else {
            range.start_block = end_block;
        }

        let start = self.blocks[start_block].start;
        let end = self.blocks[end_block - 1].end + 1;
        self.new_active(
            now,
            start_block,
            end_block,
            start,
            end,
            attempts,
            worker_label,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_active(
        &mut self,
        now: Instant,
        start_block: usize,
        end_block: usize,
        start: u64,
        end: u64,
        attempts: u32,
        worker_label: Option<String>,
    ) -> SegmentLease {
        let bounds = Arc::new(Mutex::new(SegmentBounds::new(start, end)));
        let id = self.next_segment_id;
        self.next_segment_id += 1;
        self.active.push(ActiveSegment {
            id,
            start_block,
            end_block,
            bounds: Arc::clone(&bounds),
            attempts,
            committed_bytes: 0,
            claimed_at: now,
            worker_label: worker_label.clone(),
        });
        SegmentLease {
            id,
            start,
            end_at_request: end,
            bounds,
            attempts,
            worker_label,
        }
    }

    /// Tail steal: splits the in-flight segment where it saves the most time.
    ///
    /// The victim's remaining range is divided in proportion to the two connection speeds
    /// (victim keeps the lower part) so both finish together. A segment is only split if
    /// its speed is known, the thief's share is at least `MIN_PIECE_BLOCKS` blocks, and
    /// the range finishes at least `MIN_STEAL_GAIN` sooner.
    fn try_split_active(
        &mut self,
        now: Instant,
        thief_rate: Option<f64>,
        thief_label: Option<String>,
    ) -> Option<SegmentLease> {
        let min_steal = self.block_size().saturating_mul(MIN_PIECE_BLOCKS);
        // (active index, split block, time saved in seconds)
        let mut best: Option<(usize, usize, f64)> = None;

        for (seg_idx, seg) in self.active.iter().enumerate() {
            let bounds = seg.bounds.lock().unwrap();
            let Some(v) = bounds.rate.filter(|r| r.is_finite() && *r > 0.0) else {
                continue;
            };
            let t = thief_rate
                .filter(|r| r.is_finite() && *r > 0.0)
                .unwrap_or(v);
            let (reserved, end) = (bounds.reserved, bounds.end);
            drop(bounds);
            if end <= reserved {
                continue;
            }

            let rem = (end - reserved) as f64;
            let target_offset = end - (rem * t / (v + t)) as u64;
            let split_block = self.block_ceil_index(target_offset, seg.start_block, seg.end_block);
            if split_block >= seg.end_block {
                continue;
            }
            let split_offset = self.blocks[split_block].start;
            if split_offset <= reserved || end - split_offset < min_steal {
                continue;
            }

            let before = rem / v;
            let after = ((split_offset - reserved) as f64 / v).max((end - split_offset) as f64 / t);
            let gain = before - after;
            if gain >= MIN_STEAL_GAIN.as_secs_f64() && best.is_none_or(|b| gain > b.2) {
                best = Some((seg_idx, split_block, gain));
            }
        }

        let (seg_idx, split_block, _) = best?;
        let split_offset = self.blocks[split_block].start;
        let seg = &mut self.active[seg_idx];
        {
            let mut bounds = seg.bounds.lock().unwrap();
            // Re-check under the lock: the victim may have advanced since the scan.
            if split_offset <= bounds.reserved || split_offset >= bounds.end {
                return None;
            }
            bounds.end = split_offset;
        }
        let old_end_block = seg.end_block;
        seg.end_block = split_block;
        let thief_end = self.blocks[old_end_block - 1].end + 1;
        Some(self.new_active(
            now,
            split_block,
            old_end_block,
            split_offset,
            thief_end,
            0,
            thief_label,
        ))
    }

    /// Finds the first block index whose start is >= `offset`, clamped within `[start_block, end_block]`.
    fn block_ceil_index(&self, offset: u64, start_block: usize, end_block: usize) -> usize {
        for idx in start_block..end_block {
            let b = &self.blocks[idx];
            if offset <= b.start {
                return idx;
            }
            if offset <= b.end {
                return idx + 1;
            }
        }
        end_block
    }

    /// Commits all flushed bytes up to `flushed_offset` for the given segment.
    ///
    /// Marks completely covered blocks as `Completed` and returns newly committed bytes.
    pub fn commit(&mut self, segment_id: usize, flushed_offset: u64) -> u64 {
        let Some(seg_idx) = self.active.iter().position(|s| s.id == segment_id) else {
            return 0;
        };

        let start_block = self.active[seg_idx].start_block;
        let end_block = self.active[seg_idx].end_block;
        let mut newly_committed_bytes = 0u64;

        for idx in start_block..end_block {
            let b = &mut self.blocks[idx];
            if b.status != ChunkStatus::Completed && b.end < flushed_offset {
                b.status = ChunkStatus::Completed;
                b.downloaded = b.size;
                newly_committed_bytes += b.size;
            }
        }

        self.active[seg_idx].committed_bytes += newly_committed_bytes;
        // A request that started after the latest pause is serving data: the server accepts
        // requests again, so the give-up count restarts. (Responses already streaming when
        // the pause began prove nothing.)
        let after_pause = self
            .throttle_started
            .is_none_or(|t| self.active[seg_idx].claimed_at >= t);
        if newly_committed_bytes > 0 && after_pause {
            self.throttle_rounds = 0;
        }
        newly_committed_bytes
    }

    /// Records that the server rate limited a segment's request (HTTP 429, or 503 with
    /// `Retry-After`).
    ///
    /// The segment returns to pending without charging its retry budget, and no new request
    /// starts until `retry_after` (capped at [`MAX_RETRY_AFTER`]) or, without it, the
    /// exponential backoff has passed. For the rest of the download, requests then start
    /// one per pause length and pieces become even shares of the remaining work, so a
    /// server that limits requests per minute sees about one request per connection.
    /// Throttled responses that arrive during one pause count as a single round; after
    /// [`MAX_THROTTLE_ROUNDS`] rounds with no block committed, the range fails permanently.
    pub fn throttle(
        &mut self,
        segment_id: usize,
        now: Instant,
        retry_after: Option<Duration>,
    ) -> FailOutcome {
        let Some(pos) = self.active.iter().position(|s| s.id == segment_id) else {
            return FailOutcome::Failed;
        };
        let seg = self.active.remove(pos);

        let new_round = self.throttled_until.is_none_or(|u| now >= u);
        if new_round {
            self.throttle_rounds += 1;
            self.throttle_started = Some(now);
            let delay = retry_after
                .map(|d| d.min(MAX_RETRY_AFTER))
                .unwrap_or_else(|| self.backoff_for(self.throttle_rounds + 1));
            self.throttled_until = Some(now + delay);
            // Never shrinks: a server that rate limited once keeps its pace.
            let spacing = delay.max(self.backoff_base);
            self.request_spacing = Some(self.request_spacing.map_or(spacing, |s| s.max(spacing)));
        }
        let until = self.throttled_until.unwrap_or(now);

        let Some(start_block) = (seg.start_block..seg.end_block)
            .find(|&i| self.blocks[i].status != ChunkStatus::Completed)
        else {
            return FailOutcome::Retry { after: until - now };
        };

        if self.throttle_rounds > MAX_THROTTLE_ROUNDS {
            for idx in start_block..seg.end_block {
                self.blocks[idx].status = ChunkStatus::Failed;
            }
            return FailOutcome::Failed;
        }

        // No per-range delay: the global pause already holds every claim back, and a ready
        // range can merge with its neighbours into one request.
        self.push_pending(PendingRange {
            start_block,
            end_block: seg.end_block,
            attempts: seg.attempts,
            not_before: None,
        });
        FailOutcome::Retry { after: until - now }
    }

    /// Returns a range to pending, merged with adjacent ready ranges of the same attempt
    /// count so that it can be fetched as one request instead of several.
    fn push_pending(&mut self, mut range: PendingRange) {
        let mergeable = |a: &PendingRange, b: &PendingRange| {
            a.attempts == b.attempts && a.not_before.is_none() && b.not_before.is_none()
        };
        while let Some(i) = self.pending.iter().position(|p| {
            mergeable(p, &range)
                && (p.end_block == range.start_block || p.start_block == range.end_block)
        }) {
            let neighbour = self.pending.swap_remove(i);
            range.start_block = range.start_block.min(neighbour.start_block);
            range.end_block = range.end_block.max(neighbour.end_block);
        }
        self.pending.push(range);
    }

    /// Removes an active segment upon successful completion.
    /// If any blocks in the segment remain uncommitted, they are returned to pending.
    pub fn finish(&mut self, segment_id: usize) {
        if let Some(pos) = self.active.iter().position(|s| s.id == segment_id) {
            let seg = self.active.remove(pos);
            let mut first_uncommitted = None;
            for idx in seg.start_block..seg.end_block {
                if self.blocks[idx].status != ChunkStatus::Completed {
                    first_uncommitted = Some(idx);
                    break;
                }
            }
            if let Some(start_block) = first_uncommitted {
                self.push_pending(PendingRange {
                    start_block,
                    end_block: seg.end_block,
                    attempts: seg.attempts,
                    not_before: None,
                });
            }
        }
    }

    /// Records failure of a segment attempt.
    ///
    /// The uncommitted block range is returned to pending. If this attempt committed at least
    /// one block, attempts reset to 0 with no backoff. Otherwise attempts increment with backoff,
    /// failing permanently after max attempts.
    pub fn fail(&mut self, segment_id: usize, now: Instant) -> FailOutcome {
        let Some(pos) = self.active.iter().position(|s| s.id == segment_id) else {
            return FailOutcome::Failed;
        };
        let seg = self.active.remove(pos);

        // Find first uncommitted block in segment's assigned range
        let mut first_uncommitted = None;
        for idx in seg.start_block..seg.end_block {
            if self.blocks[idx].status != ChunkStatus::Completed {
                first_uncommitted = Some(idx);
                break;
            }
        }

        let Some(start_block) = first_uncommitted else {
            return FailOutcome::Retry {
                after: Duration::ZERO,
            };
        };

        if seg.committed_bytes > 0 {
            // Forward progress was made: reset attempt count and re-queue without backoff
            self.push_pending(PendingRange {
                start_block,
                end_block: seg.end_block,
                attempts: 0,
                not_before: None,
            });
            FailOutcome::Retry {
                after: Duration::ZERO,
            }
        } else {
            let attempts = seg.attempts + 1;
            if attempts >= self.max_attempts {
                for idx in start_block..seg.end_block {
                    self.blocks[idx].status = ChunkStatus::Failed;
                }
                FailOutcome::Failed
            } else {
                let backoff = self.backoff_for(attempts);
                self.push_pending(PendingRange {
                    start_block,
                    end_block: seg.end_block,
                    attempts,
                    not_before: Some(now + backoff),
                });
                FailOutcome::Retry { after: backoff }
            }
        }
    }

    /// Releases an active segment back to pending without penalty (e.g. on pause, cancel, or adapter retire).
    pub fn release(&mut self, segment_id: usize) {
        let Some(pos) = self.active.iter().position(|s| s.id == segment_id) else {
            return;
        };
        let seg = self.active.remove(pos);

        let mut first_uncommitted = None;
        for idx in seg.start_block..seg.end_block {
            if self.blocks[idx].status != ChunkStatus::Completed {
                first_uncommitted = Some(idx);
                break;
            }
        }

        if let Some(start_block) = first_uncommitted {
            self.push_pending(PendingRange {
                start_block,
                end_block: seg.end_block,
                attempts: seg.attempts,
                not_before: None,
            });
        }
    }

    /// Returns ids of all `Completed` blocks in ascending order.
    pub fn completed_ids(&self) -> Vec<usize> {
        self.ids_with_status(ChunkStatus::Completed)
    }

    /// Returns ids of blocks matching the specified status.
    pub fn ids_with_status(&self, status: ChunkStatus) -> Vec<usize> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.status == status)
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Returns true if all planned blocks are `Completed`.
    pub fn all_completed(&self) -> bool {
        self.blocks
            .iter()
            .all(|b| b.status == ChunkStatus::Completed)
    }

    /// Returns ids of all non-completed blocks.
    pub fn incomplete_ids(&self) -> Vec<usize> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.status != ChunkStatus::Completed)
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Count of currently active segments.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Count of completed blocks.
    pub fn completed_count(&self) -> usize {
        self.blocks
            .iter()
            .filter(|b| b.status == ChunkStatus::Completed)
            .count()
    }

    /// Total number of planned blocks.
    pub fn total_blocks(&self) -> usize {
        self.blocks.len()
    }

    /// Borrow of the underlying planned blocks.
    pub fn blocks(&self) -> &[Chunk] {
        &self.blocks
    }

    /// Total bytes across all blocks.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// One char per block in plan order:
    /// - `#`: completed
    /// - `>`: actively being written by an active segment
    /// - `!`: failed
    /// - `.`: pending
    pub fn chunk_map(&self) -> String {
        let mut map = vec!['.'; self.blocks.len()];

        // Mark completed and failed
        for (idx, b) in self.blocks.iter().enumerate() {
            match b.status {
                ChunkStatus::Completed => map[idx] = '#',
                ChunkStatus::Failed => map[idx] = '!',
                _ => {}
            }
        }

        // Mark active positions (where workers are currently reading/writing)
        for seg in &self.active {
            let reserved = seg.bounds.lock().unwrap().reserved;
            for (idx, b) in self
                .blocks
                .iter()
                .enumerate()
                .take(seg.end_block)
                .skip(seg.start_block)
            {
                if reserved >= b.start && reserved <= b.end && map[idx] != '#' {
                    map[idx] = '>';
                    break;
                }
            }
        }

        map.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::plan_chunks;

    #[test]
    fn test_block_size_for_bounds_and_examples() {
        assert_eq!(block_size_for(0), MIN_BLOCK_SIZE);
        assert_eq!(block_size_for(1), MIN_BLOCK_SIZE);
        assert_eq!(block_size_for(MIN_BLOCK_SIZE), MIN_BLOCK_SIZE);

        // 100 MB -> 1 MiB (100 blocks <= 8192)
        assert_eq!(block_size_for(100 * 1024 * 1024), 1024 * 1024);

        // 8 GiB -> exactly 8192 blocks of 1 MiB
        assert_eq!(block_size_for(8 * 1024 * 1024 * 1024), 1024 * 1024);

        // 20 GiB -> 4 MiB (5120 blocks <= 8192)
        assert_eq!(block_size_for(20 * 1024 * 1024 * 1024), 4 * 1024 * 1024);

        // 100 GiB -> 16 MiB (6400 blocks <= 8192)
        assert_eq!(block_size_for(100 * 1024 * 1024 * 1024), 16 * 1024 * 1024);

        // Assert block count never exceeds MAX_BLOCKS for any size up to 1 TB
        for gb in [1, 5, 10, 20, 50, 100, 500, 1000] {
            let total = gb * 1024 * 1024 * 1024;
            let bs = block_size_for(total);
            let n_blocks = total.div_ceil(bs);
            assert!(
                n_blocks <= MAX_BLOCKS,
                "total {} at block_size {} gives {} blocks > {}",
                total,
                bs,
                n_blocks,
                MAX_BLOCKS
            );
        }

        assert_eq!(block_size_for(u64::MAX), 1 << 51);
    }

    #[test]
    fn test_segment_bounds_take_and_cut_off() {
        let mut bounds = SegmentBounds::new(100, 300);
        assert_eq!(bounds.remaining(), 200);

        let (take1, reached1) = bounds.take(50);
        assert_eq!(take1, 50);
        assert!(!reached1);
        assert_eq!(bounds.reserved, 150);

        // Shrink end (simulating split)
        bounds.end = 200;
        assert_eq!(bounds.remaining(), 50);

        // Request 100 bytes: should only take 50 and reach end
        let (take2, reached2) = bounds.take(100);
        assert_eq!(take2, 50);
        assert!(reached2);
        assert_eq!(bounds.reserved, 200);
        assert_eq!(bounds.remaining(), 0);

        // Next take gives 0 bytes and reached_end is true
        let (take3, reached3) = bounds.take(10);
        assert_eq!(take3, 0);
        assert!(reached3);
    }

    const MIB: u64 = 1024 * 1024;

    fn lease_of(claim: Claim) -> SegmentLease {
        match claim {
            Claim::Segment(lease) => lease,
            other => panic!("expected a segment, got {:?}", other),
        }
    }

    /// Simulates the victim's worker having received `bytes` at `rate` bytes/s.
    fn advance(lease: &SegmentLease, bytes: u64, rate: f64) {
        let mut b = lease.bounds.lock().unwrap();
        b.take(bytes);
        b.rate = Some(rate);
    }

    #[test]
    fn test_piece_bytes_for_scales_with_speed_and_clamps() {
        // No measurement yet: minimum piece.
        assert_eq!(piece_bytes_for(None, MIB), 4 * MIB);
        assert_eq!(piece_bytes_for(Some(0.0), MIB), 4 * MIB);
        assert_eq!(piece_bytes_for(Some(f64::NAN), MIB), 4 * MIB);
        // 10 MB/s for 4 s.
        assert_eq!(piece_bytes_for(Some(10_000_000.0), MIB), 40_000_000);
        // Slow and fast connections are clamped.
        assert_eq!(piece_bytes_for(Some(100_000.0), MIB), 4 * MIB);
        assert_eq!(piece_bytes_for(Some(1e12), MIB), MAX_PIECE_BYTES);
        // Huge blocks raise the minimum above the maximum: the minimum wins.
        assert_eq!(piece_bytes_for(Some(1e12), 128 * MIB), 512 * MIB);
    }

    #[test]
    fn test_claims_are_speed_sized_pieces_in_file_order() {
        let blocks = plan_chunks(1024 * MIB, MIB);
        let mut scheduler = SegmentScheduler::new(blocks, 5, Duration::from_millis(100));
        let now = Instant::now();

        // A fresh connection gets the minimum piece from the start of the file.
        let first = lease_of(scheduler.claim(now, None, None));
        assert_eq!((first.start, first.end_at_request), (0, 4 * MIB));
        assert_eq!(first.to_range_header(), "bytes=0-4194303");

        // A 10 MB/s connection gets ~4 s of data (40 MB rounded up to whole blocks).
        let second = lease_of(scheduler.claim(now, Some(10_000_000.0), None));
        assert_eq!(second.start, 4 * MIB);
        assert_eq!(second.end_at_request, 4 * MIB + 39 * MIB);

        // Nothing in flight was split to make room.
        assert_eq!(first.bounds.lock().unwrap().end, 4 * MIB);
        assert_eq!(second.bounds.lock().unwrap().end, 43 * MIB);
    }

    #[test]
    fn test_claim_takes_whole_remainder_instead_of_leaving_a_sliver() {
        // 10 blocks: a 4-block piece would leave 6, which is fine; a 4-block piece out of
        // 7 would leave 3 (< MIN_PIECE_BLOCKS), so the claim takes all 7.
        let mut scheduler =
            SegmentScheduler::new(plan_chunks(7 * MIB, MIB), 5, Duration::from_millis(100));
        let lease = lease_of(scheduler.claim(Instant::now(), None, None));
        assert_eq!((lease.start, lease.end_at_request), (0, 7 * MIB));

        let mut scheduler =
            SegmentScheduler::new(plan_chunks(10 * MIB, MIB), 5, Duration::from_millis(100));
        let lease = lease_of(scheduler.claim(Instant::now(), None, None));
        assert_eq!((lease.start, lease.end_at_request), (0, 4 * MIB));
    }

    #[test]
    fn test_tail_steal_splits_in_proportion_to_speed() {
        let blocks = plan_chunks(64 * MIB, MIB);
        let mut scheduler = SegmentScheduler::new(blocks, 5, Duration::from_millis(100));
        let now = Instant::now();

        // A slow 1 MB/s connection holds the whole file (4 s pieces would be 4 MiB, so
        // claim with a fast rate to get it all), then reports its real speed.
        let victim = lease_of(scheduler.claim(now, Some(1e9), Some("4g".into())));
        assert_eq!(victim.end_at_request, 64 * MIB);
        advance(&victim, 4 * MIB, 1_000_000.0);

        // A 3 MB/s thief: victim keeps ~1/4 of the remaining 60 MiB, thief takes ~3/4.
        let thief = lease_of(scheduler.claim(now, Some(3_000_000.0), Some("eth".into())));
        let victim_end = victim.bounds.lock().unwrap().end;
        assert_eq!(
            victim_end, thief.start,
            "split must leave no gap or overlap"
        );
        assert_eq!(thief.end_at_request, 64 * MIB);
        assert_eq!(victim_end, 19 * MIB); // 4 MiB + ceil(15 MiB) at the block boundary
        assert_eq!(victim_end % MIB, 0, "split lands on a block boundary");
    }

    #[test]
    fn test_tail_steal_requires_known_speed_gain_and_minimum_size() {
        let now = Instant::now();

        // Unknown victim speed: no steal, just wait.
        let mut s = SegmentScheduler::new(plan_chunks(64 * MIB, MIB), 5, Duration::ZERO);
        let _v = lease_of(s.claim(now, Some(1e9), None));
        assert!(matches!(s.claim(now, None, None), Claim::Wait(IDLE_POLL)));

        // Fast victim: 60 MiB left at 100 MB/s is ~0.6 s; splitting saves < 1 s.
        let mut s = SegmentScheduler::new(plan_chunks(64 * MIB, MIB), 5, Duration::ZERO);
        let v = lease_of(s.claim(now, Some(1e9), None));
        advance(&v, 4 * MIB, 100_000_000.0);
        assert!(matches!(s.claim(now, None, None), Claim::Wait(_)));

        // Slow victim but only 6 blocks left: the thief's share would be < 4 blocks.
        let mut s = SegmentScheduler::new(plan_chunks(8 * MIB, MIB), 5, Duration::ZERO);
        let v = lease_of(s.claim(now, Some(1e9), None));
        advance(&v, 2 * MIB, 100_000.0);
        assert!(matches!(
            s.claim(now, Some(100_000.0), None),
            Claim::Wait(_)
        ));

        // Same victim with 16 blocks left: worth it.
        let mut s = SegmentScheduler::new(plan_chunks(18 * MIB, MIB), 5, Duration::ZERO);
        let v = lease_of(s.claim(now, Some(1e9), None));
        advance(&v, 2 * MIB, 100_000.0);
        let t = lease_of(s.claim(now, Some(100_000.0), None));
        assert_eq!(v.bounds.lock().unwrap().end, t.start);
        assert_eq!((t.start, t.end_at_request), (10 * MIB, 18 * MIB));
    }

    #[test]
    fn test_throttle_pauses_claims_without_charging_attempts() {
        let blocks = plan_chunks(16 * MIB, MIB);
        let mut s = SegmentScheduler::new(blocks, 2, Duration::from_millis(100));
        s.add_worker();
        s.add_worker();
        let now = Instant::now();
        let a = lease_of(s.claim(now, None, None));
        let b = lease_of(s.claim(now, None, None));

        // Two 429s in the same pause count as one round; Retry-After is honoured.
        let r1 = s.throttle(a.id, now, Some(Duration::from_secs(3)));
        assert_eq!(
            r1,
            FailOutcome::Retry {
                after: Duration::from_secs(3)
            }
        );
        let r2 = s.throttle(b.id, now + Duration::from_secs(1), None);
        assert_eq!(
            r2,
            FailOutcome::Retry {
                after: Duration::from_secs(2)
            }
        );
        assert_eq!(s.throttle_rounds, 1);

        // Nobody may start a request during the pause.
        assert!(matches!(
            s.claim(now + Duration::from_secs(2), None, None),
            Claim::Wait(d) if d == Duration::from_secs(1)
        ));

        // After it, requests restart one per pause length (3 s) and attempts were not
        // charged (max_attempts is 2, so a charged failure would already be fatal).
        let after = now + Duration::from_secs(3);
        let c = lease_of(s.claim(after, None, None));
        assert_eq!(c.attempts, 0);
        assert!(matches!(
            s.claim(after, None, None),
            Claim::Wait(d) if d == Duration::from_secs(3)
        ));
        let d = lease_of(s.claim(after + Duration::from_secs(3), None, None));
        assert_eq!(d.attempts, 0);

        // A block committed by a post-pause request restarts the give-up count.
        s.commit(c.id, c.start + MIB);
        assert_eq!(s.throttle_rounds, 0);
    }

    #[test]
    fn test_rate_limited_download_switches_to_fair_share_pieces_and_paced_requests() {
        let mut s = SegmentScheduler::new(plan_chunks(1024 * MIB, MIB), 5, Duration::ZERO);
        for _ in 0..4 {
            s.add_worker();
        }
        let now = Instant::now();
        let rate = Some(1_000_000.0); // 1 MB/s: speed-sized pieces would be 4 blocks
        let piece = |l: &SegmentLease| l.end_at_request - l.start;

        let a = lease_of(s.claim(now, rate, None));
        let b = lease_of(s.claim(now, rate, None));
        assert_eq!((piece(&a), piece(&b)), (4 * MIB, 4 * MIB));
        s.throttle(b.id, now, Some(Duration::from_secs(10)));

        // After the pause, the throttled range has merged with the rest of the file, so the
        // retry is one long request: an even share of the 1020 pending blocks among the 3
        // idle workers, not another 4 s piece.
        let t1 = now + Duration::from_secs(10);
        let retry = lease_of(s.claim(t1, rate, None));
        assert_eq!((retry.start, piece(&retry)), (4 * MIB, 340 * MIB));
        // The next request waits one pause length, then takes half of the remaining 680
        // blocks (2 of 4 workers idle).
        assert!(matches!(
            s.claim(t1, rate, None),
            Claim::Wait(d) if d == Duration::from_secs(10)
        ));
        let t2 = t1 + Duration::from_secs(10);
        let c = lease_of(s.claim(t2, rate, None));
        assert_eq!(piece(&c), 340 * MIB);

        // Pacing stays for the rest of the download, even after progress.
        s.commit(c.id, c.start + MIB);
        assert!(matches!(s.claim(t2, rate, None), Claim::Wait(_)));
    }

    #[test]
    fn test_responses_started_before_a_pause_do_not_reset_give_up_count() {
        let mut s =
            SegmentScheduler::new(plan_chunks(64 * MIB, MIB), 5, Duration::from_millis(100));
        let t0 = Instant::now();
        let streaming = lease_of(s.claim(t0, None, None));
        let rejected = lease_of(s.claim(t0, None, None));

        let t1 = t0 + Duration::from_secs(1);
        s.throttle(rejected.id, t1, Some(Duration::from_secs(1)));

        // The older response keeps delivering blocks during and after the pause...
        s.commit(streaming.id, streaming.start + MIB);
        assert_eq!(
            s.throttle_rounds, 1,
            "pre-pause response must not reset the give-up count"
        );

        // A request started after the pause committing a block restarts the count.
        let t2 = t1 + Duration::from_secs(1);
        let first = lease_of(s.claim(t2, None, None));
        s.commit(first.id, first.start + MIB);
        assert_eq!(s.throttle_rounds, 0);
    }

    #[test]
    fn test_throttle_fails_after_max_rounds_without_progress() {
        let mut s = SegmentScheduler::new(plan_chunks(4 * MIB, MIB), 5, Duration::from_millis(1));
        let mut now = Instant::now();
        for round in 1..=MAX_THROTTLE_ROUNDS {
            let lease = lease_of(s.claim(now, None, None));
            let outcome = s.throttle(lease.id, now, None);
            assert!(
                matches!(outcome, FailOutcome::Retry { .. }),
                "round {round}"
            );
            now += MAX_RETRY_AFTER;
        }
        let lease = lease_of(s.claim(now, None, None));
        assert_eq!(s.throttle(lease.id, now, None), FailOutcome::Failed);
        assert!(matches!(s.claim(now, None, None), Claim::Finished));
        assert_eq!(s.chunk_map(), "!!!!");
    }

    #[test]
    fn test_throttle_without_retry_after_uses_backoff_and_caps_retry_after() {
        let mut s = SegmentScheduler::new(plan_chunks(4 * MIB, MIB), 5, Duration::from_millis(100));
        let now = Instant::now();
        let lease = lease_of(s.claim(now, None, None));
        // Round 1 waits backoff_for(2) = 200 ms.
        assert_eq!(
            s.throttle(lease.id, now, None),
            FailOutcome::Retry {
                after: Duration::from_millis(200)
            }
        );
        let later = now + Duration::from_secs(1);
        let lease = lease_of(s.claim(later, None, None));
        assert_eq!(
            s.throttle(lease.id, later, Some(Duration::from_secs(3600))),
            FailOutcome::Retry {
                after: MAX_RETRY_AFTER
            }
        );
    }

    #[test]
    fn test_scheduler_commit_and_finish() {
        let blocks = plan_chunks(4 * 1024 * 1024, 1024 * 1024); // 4 blocks
        let mut scheduler = SegmentScheduler::new(blocks, 3, Duration::from_millis(50));

        let Claim::Segment(lease) = scheduler.claim(Instant::now(), None, None) else {
            panic!("expected segment");
        };

        // Flush past block 0 (offset 1024 * 1024)
        let newly_committed = scheduler.commit(lease.id, 1024 * 1024);
        assert_eq!(newly_committed, 1024 * 1024);
        assert_eq!(scheduler.completed_ids(), vec![0]);
        assert_eq!(scheduler.completed_count(), 1);

        // Flush remaining blocks up to 4 MB
        let committed_rest = scheduler.commit(lease.id, 4 * 1024 * 1024);
        assert_eq!(committed_rest, 3 * 1024 * 1024);
        assert!(scheduler.all_completed());

        scheduler.finish(lease.id);
        assert_eq!(scheduler.active_count(), 0);
        assert!(matches!(
            scheduler.claim(Instant::now(), None, None),
            Claim::Finished
        ));
    }

    #[test]
    fn test_scheduler_fail_with_progress_resets_attempts_and_no_backoff() {
        let blocks = plan_chunks(4 * 1024 * 1024, 1024 * 1024);
        let mut scheduler = SegmentScheduler::new(blocks, 3, Duration::from_millis(500));

        let Claim::Segment(lease) = scheduler.claim(Instant::now(), None, None) else {
            panic!("expected segment");
        };

        // Commit block 0, then fail mid-stream
        scheduler.commit(lease.id, 1024 * 1024);
        let outcome = scheduler.fail(lease.id, Instant::now());

        assert_eq!(
            outcome,
            FailOutcome::Retry {
                after: Duration::ZERO
            }
        );

        // Next claim should immediately get the uncommitted range [1, 4)
        let Claim::Segment(lease_retry) = scheduler.claim(Instant::now(), None, None) else {
            panic!("expected immediate retry");
        };
        assert_eq!(lease_retry.start, 1024 * 1024);
        assert_eq!(lease_retry.attempts, 0);
    }

    #[test]
    fn test_scheduler_fail_without_progress_backs_off_and_fails_permanently() {
        let blocks = plan_chunks(2 * 1024 * 1024, 1024 * 1024);
        let mut scheduler = SegmentScheduler::new(blocks, 2, Duration::from_millis(100));

        let now = Instant::now();
        let Claim::Segment(l1) = scheduler.claim(now, None, None) else {
            panic!("expected segment");
        };

        // Attempt 1 fails without committing anything
        let outcome1 = scheduler.fail(l1.id, now);
        assert_eq!(
            outcome1,
            FailOutcome::Retry {
                after: Duration::from_millis(100)
            }
        );

        // Immediate claim must wait
        assert!(matches!(scheduler.claim(now, None, None), Claim::Wait(_)));

        // After backoff, claim succeeds
        let Claim::Segment(l2) = scheduler.claim(now + Duration::from_millis(100), None, None)
        else {
            panic!("expected retry segment");
        };
        assert_eq!(l2.attempts, 1);

        // Attempt 2 fails -> max attempts reached -> permanent failure
        let outcome2 = scheduler.fail(l2.id, now + Duration::from_millis(100));
        assert_eq!(outcome2, FailOutcome::Failed);

        // Scheduler finishes with failure
        assert!(matches!(
            scheduler.claim(now + Duration::from_secs(1), None, None),
            Claim::Finished
        ));
        assert_eq!(scheduler.chunk_map(), "!!");
    }

    #[test]
    fn test_scheduler_release_has_no_penalty() {
        let blocks = plan_chunks(2 * 1024 * 1024, 1024 * 1024);
        let mut scheduler = SegmentScheduler::new(blocks, 3, Duration::from_millis(100));

        let Claim::Segment(lease) = scheduler.claim(Instant::now(), None, None) else {
            panic!("expected segment");
        };
        scheduler.release(lease.id);

        let Claim::Segment(lease_again) = scheduler.claim(Instant::now(), None, None) else {
            panic!("expected segment");
        };
        assert_eq!(lease_again.attempts, 0);
    }

    #[test]
    fn test_scheduler_resume_precompleted_blocks() {
        let mut blocks = plan_chunks(4 * 1024 * 1024, 1024 * 1024);
        // Pre-complete block 0 and block 2
        blocks[0].status = ChunkStatus::Completed;
        blocks[0].downloaded = blocks[0].size;
        blocks[2].status = ChunkStatus::Completed;
        blocks[2].downloaded = blocks[2].size;

        let mut scheduler = SegmentScheduler::new(blocks, 3, Duration::from_millis(100));
        assert_eq!(scheduler.completed_ids(), vec![0, 2]);

        // First claim gets block 1 (or block 3)
        let Claim::Segment(l1) = scheduler.claim(Instant::now(), None, None) else {
            panic!("expected segment");
        };
        assert!(l1.start == 1024 * 1024 || l1.start == 3 * 1024 * 1024);
    }

    #[test]
    fn test_concurrent_reserve_and_split_disjointness() {
        let total = 10_000_000u64;
        let bounds = Arc::new(Mutex::new(SegmentBounds::new(0, total)));

        let b_worker = Arc::clone(&bounds);
        let worker_handle = std::thread::spawn(move || {
            let mut ranges = Vec::new();
            loop {
                let mut b = b_worker.lock().unwrap();
                let (take, reached) = b.take(10_000);
                if take > 0 {
                    let start = b.reserved - take;
                    ranges.push((start, start + take));
                }
                if reached {
                    break;
                }
            }
            ranges
        });

        // Thief splits after small pause
        let b_thief = Arc::clone(&bounds);
        let thief_handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(2));
            let split_range = {
                let mut b = b_thief.lock().unwrap();
                if b.remaining() > 50_000 {
                    let split = b.reserved + (b.remaining() / 2);
                    let old_end = b.end;
                    b.end = split;
                    Some((split, old_end))
                } else {
                    None
                }
            };

            let mut thief_ranges = Vec::new();
            if let Some((start, end)) = split_range {
                let mut thief_bounds = SegmentBounds::new(start, end);
                loop {
                    let (take, reached) = thief_bounds.take(10_000);
                    if take > 0 {
                        let s = thief_bounds.reserved - take;
                        thief_ranges.push((s, s + take));
                    }
                    if reached {
                        break;
                    }
                }
            }
            thief_ranges
        });

        let worker_ranges = worker_handle.join().unwrap();
        let thief_ranges = thief_handle.join().unwrap();

        // Assert no overlapping ranges between worker and thief
        for (w_start, w_end) in &worker_ranges {
            for (t_start, t_end) in &thief_ranges {
                assert!(
                    *w_end <= *t_start || *w_start >= *t_end,
                    "overlap detected: worker [{}, {}] vs thief [{}, {}]",
                    w_start,
                    w_end,
                    t_start,
                    t_end
                );
            }
        }
    }

    #[test]
    fn test_scheduler_random_sequence_consistency() {
        let (mut steals, mut throttles) = (0, 0);
        for seed in 0..20u64 {
            let (s, t) =
                run_random_sequence(0xdeadbeef12345678 ^ seed.wrapping_mul(0x9e3779b97f4a7c15));
            steals += s;
            throttles += t;
        }
        // The run must actually exercise tail steals and rate-limit pauses.
        assert!(steals > 0, "no tail steal happened");
        assert!(throttles > 0, "no throttle happened");
    }

    /// Random claim/progress/commit/finish/fail/release/throttle sequence on 128 blocks,
    /// checking the block partition invariant after every step. Returns
    /// `(tail steals, throttles)` observed.
    #[allow(clippy::needless_range_loop)]
    fn run_random_sequence(seed: u64) -> (usize, usize) {
        let n_blocks = 128;
        let block_size = 1024 * 1024;
        let blocks = plan_chunks(n_blocks * block_size, block_size);
        // A large attempt budget keeps the run from ending in an early permanent failure.
        let mut scheduler = SegmentScheduler::new(blocks, 1000, Duration::from_millis(10));
        let (mut steals, mut throttles) = (0usize, 0usize);

        let mut rng_state: u64 = seed;
        let mut next_rand = || -> u64 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            rng_state
        };

        let mut completed_history = std::collections::HashSet::new();

        let start = Instant::now();
        for step in 0..2000u64 {
            // Weighted towards claims and partial progress (which publishes speeds) so runs
            // reach the tail phase where steals happen; rate limiting is rare.
            let action = match next_rand() % 40 {
                0 => 6,
                1..=12 => 0,
                13..=24 => 5,
                25..=28 => 1,
                29..=32 => 2,
                33..=36 => 3,
                _ => 4,
            };
            // Virtual clock so backoffs and rate-limit pauses expire during the run.
            let now = start + Duration::from_millis(step * 20);

            match action {
                0 => {
                    // Claim, with no measurement or a random connection speed
                    let rate = match next_rand() % 3 {
                        0 => None,
                        _ => Some((next_rand() % 50_000_000 + 10_000) as f64),
                    };
                    let ends_before: Vec<(usize, u64)> = scheduler
                        .active
                        .iter()
                        .map(|a| (a.id, a.bounds.lock().unwrap().end))
                        .collect();
                    if let Claim::Segment(_) = scheduler.claim(now, rate, Some("test".into())) {
                        let shrunk = scheduler.active.iter().any(|a| {
                            ends_before
                                .iter()
                                .any(|(id, end)| *id == a.id && a.bounds.lock().unwrap().end < *end)
                        });
                        steals += usize::from(shrunk);
                    }
                }
                5 => {
                    // Receive part of an active segment at a random speed and commit it,
                    // which makes tail steals possible on later claims.
                    if !scheduler.active.is_empty() {
                        let idx = (next_rand() as usize) % scheduler.active.len();
                        let seg_id = scheduler.active[idx].id;
                        let reserved = {
                            let mut b = scheduler.active[idx].bounds.lock().unwrap();
                            let len = next_rand() % (b.remaining() + 1);
                            b.take(len);
                            b.rate = Some((next_rand() % 5_000_000 + 10_000) as f64);
                            b.reserved
                        };
                        scheduler.commit(seg_id, reserved);
                    }
                }
                6 => {
                    // Server rate limits an active segment
                    if !scheduler.active.is_empty() {
                        let idx = (next_rand() as usize) % scheduler.active.len();
                        let seg_id = scheduler.active[idx].id;
                        let retry_after = (next_rand() % 2 == 0)
                            .then(|| Duration::from_millis(next_rand() % 200));
                        let _ = scheduler.throttle(seg_id, now, retry_after);
                        throttles += 1;
                    }
                }
                1 => {
                    // Commit on random active segment
                    if !scheduler.active.is_empty() {
                        let idx = (next_rand() as usize) % scheduler.active.len();
                        let seg_id = scheduler.active[idx].id;
                        let end_offset =
                            scheduler.blocks[scheduler.active[idx].end_block - 1].end + 1;
                        scheduler.commit(seg_id, end_offset);
                    }
                }
                2 => {
                    // Finish active segment if fully committed
                    if !scheduler.active.is_empty() {
                        let idx = (next_rand() as usize) % scheduler.active.len();
                        let seg_id = scheduler.active[idx].id;
                        scheduler.finish(seg_id);
                    }
                }
                3 => {
                    // Fail active segment
                    if !scheduler.active.is_empty() {
                        let idx = (next_rand() as usize) % scheduler.active.len();
                        let seg_id = scheduler.active[idx].id;
                        let _ = scheduler.fail(seg_id, now);
                    }
                }
                _ => {
                    // Release active segment
                    if !scheduler.active.is_empty() {
                        let idx = (next_rand() as usize) % scheduler.active.len();
                        let seg_id = scheduler.active[idx].id;
                        scheduler.release(seg_id);
                    }
                }
            }

            // Invariant check:
            // 1. Completed blocks never revert
            let current_completed = scheduler.completed_ids();
            for &c in &current_completed {
                completed_history.insert(c);
            }
            for &prev in &completed_history {
                assert!(
                    current_completed.contains(&prev),
                    "completed block {} was reverted!",
                    prev
                );
            }

            // 2. Disjoint partition: Every block is in exactly one of (completed, active, pending, failed)
            let mut block_counts = vec![0; n_blocks as usize];
            for (idx, b) in scheduler.blocks.iter().enumerate() {
                if b.status == ChunkStatus::Completed || b.status == ChunkStatus::Failed {
                    block_counts[idx] += 1;
                }
            }
            for seg in &scheduler.active {
                for idx in seg.start_block..seg.end_block {
                    if scheduler.blocks[idx].status != ChunkStatus::Completed {
                        block_counts[idx] += 1;
                    }
                }
            }
            for p in &scheduler.pending {
                for idx in p.start_block..p.end_block {
                    if scheduler.blocks[idx].status != ChunkStatus::Completed {
                        block_counts[idx] += 1;
                    }
                }
            }

            for (idx, &count) in block_counts.iter().enumerate() {
                assert_eq!(
                    count, 1,
                    "block {} has partition count {} != 1 at step",
                    idx, count
                );
            }
        }
        (steals, throttles)
    }
}
