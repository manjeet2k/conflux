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
}

impl SegmentBounds {
    pub fn new(reserved: u64, end: u64) -> Self {
        Self { reserved, end }
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
    #[allow(dead_code)]
    worker_label: Option<String>,
}

/// Dynamic range splitting work scheduler.
///
/// Decouples HTTP request ranges (segments) from accounting units (blocks).
/// Workers stream long continuous ranges and steal the upper half of active segments
/// when idle, while disk persistence, sidecar recording, and retries operate on small blocks.
pub struct SegmentScheduler {
    blocks: Vec<Chunk>,
    active: Vec<ActiveSegment>,
    pending: Vec<PendingRange>,
    next_segment_id: usize,
    max_attempts: u32,
    backoff_base: Duration,
    total_bytes: u64,
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
        }
    }

    /// Backoff before a failed range may be reclaimed:
    /// base, 2*base, 4*base, 8*base, capped at 16*base.
    pub fn backoff_for(&self, failed_attempts: u32) -> Duration {
        let exponent = failed_attempts.saturating_sub(1).min(4);
        self.backoff_base.saturating_mul(1u32 << exponent)
    }

    /// Requests work for a worker.
    ///
    /// Priority:
    /// 1. If any block is permanently `Failed`, return `Finished`.
    /// 2. Claim the largest ready range from `pending`.
    /// 3. Otherwise, steal (split) the upper half of the active segment with the most remaining unreserved bytes.
    /// 4. If work is waiting on backoff or active segments are running, return `Wait`.
    /// 5. Otherwise, return `Finished`.
    pub fn claim(&mut self, now: Instant, worker_label: Option<String>) -> Claim {
        if self.blocks.iter().any(|b| b.status == ChunkStatus::Failed) {
            return Claim::Finished;
        }

        // 1. Try claiming from ready pending ranges
        let mut best_pending_idx = None;
        let mut best_block_count = 0;
        let mut soonest_backoff: Option<Duration> = None;

        for (idx, range) in self.pending.iter().enumerate() {
            let ready_in = range
                .not_before
                .map(|t| t.saturating_duration_since(now))
                .unwrap_or(Duration::ZERO);

            if ready_in.is_zero() {
                let count = range.end_block - range.start_block;
                if count > best_block_count {
                    best_block_count = count;
                    best_pending_idx = Some(idx);
                }
            } else {
                soonest_backoff = Some(soonest_backoff.map_or(ready_in, |s| s.min(ready_in)));
            }
        }

        if let Some(idx) = best_pending_idx {
            let range = self.pending.remove(idx);
            let start = self.blocks[range.start_block].start;
            let end = self.blocks[range.end_block - 1].end + 1;
            let bounds = Arc::new(Mutex::new(SegmentBounds::new(start, end)));
            let id = self.next_segment_id;
            self.next_segment_id += 1;

            self.active.push(ActiveSegment {
                id,
                start_block: range.start_block,
                end_block: range.end_block,
                bounds: Arc::clone(&bounds),
                attempts: range.attempts,
                committed_bytes: 0,
                worker_label: worker_label.clone(),
            });

            return Claim::Segment(SegmentLease {
                id,
                start,
                end_at_request: end,
                bounds,
                attempts: range.attempts,
                worker_label,
            });
        }

        // 2. Try splitting an active segment (work stealing)
        if let Some(thief_lease) = self.try_split_active(worker_label.clone()) {
            return Claim::Segment(thief_lease);
        }

        // 3. Wait or finish
        let has_active = !self.active.is_empty();
        match (soonest_backoff, has_active) {
            (Some(d), true) => Claim::Wait(d.min(IDLE_POLL)),
            (Some(d), false) => Claim::Wait(d),
            (None, true) => Claim::Wait(IDLE_POLL),
            (None, false) => Claim::Finished,
        }
    }

    /// Attempts to split the active segment with the most remaining unreserved bytes.
    fn try_split_active(&mut self, thief_label: Option<String>) -> Option<SegmentLease> {
        // Collect candidate indices sorted by remaining bytes descending
        let mut candidates: Vec<(usize, u64)> = self
            .active
            .iter()
            .enumerate()
            .map(|(i, seg)| {
                let rem = seg.bounds.lock().unwrap().remaining();
                (i, rem)
            })
            .collect();
        candidates.sort_by_key(|a| std::cmp::Reverse(a.1));

        for (seg_idx, rem) in candidates {
            if rem == 0 {
                continue;
            }

            let seg = &self.active[seg_idx];
            let mut bounds = seg.bounds.lock().unwrap();
            let reserved = bounds.reserved;
            let end = bounds.end;

            // Find block containing reserved offset
            let first_splittable_block =
                self.block_ceil_index(reserved, seg.start_block, seg.end_block);
            let remaining_blocks = seg.end_block.saturating_sub(first_splittable_block);

            // Need at least 2 remaining blocks above block_ceil(reserved) to split
            if remaining_blocks < 2 {
                continue;
            }

            let split_block = first_splittable_block + (remaining_blocks / 2);
            let split_offset = self.blocks[split_block].start;

            if split_offset <= reserved || split_offset >= end {
                continue;
            }

            // Shrink victim's end
            bounds.end = split_offset;
            let old_end_block = seg.end_block;
            drop(bounds);

            self.active[seg_idx].end_block = split_block;

            // Create thief segment
            let thief_start = split_offset;
            let thief_end = self.blocks[old_end_block - 1].end + 1;
            let thief_bounds = Arc::new(Mutex::new(SegmentBounds::new(thief_start, thief_end)));
            let thief_id = self.next_segment_id;
            self.next_segment_id += 1;

            self.active.push(ActiveSegment {
                id: thief_id,
                start_block: split_block,
                end_block: old_end_block,
                bounds: Arc::clone(&thief_bounds),
                attempts: 0,
                committed_bytes: 0,
                worker_label: thief_label.clone(),
            });

            return Some(SegmentLease {
                id: thief_id,
                start: thief_start,
                end_at_request: thief_end,
                bounds: thief_bounds,
                attempts: 0,
                worker_label: thief_label,
            });
        }

        None
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
        newly_committed_bytes
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
                self.pending.push(PendingRange {
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
            self.pending.push(PendingRange {
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
                self.pending.push(PendingRange {
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
            self.pending.push(PendingRange {
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

    #[test]
    fn test_scheduler_first_claim_covers_whole_range_and_splits() {
        let blocks = plan_chunks(16 * 1024 * 1024, 1024 * 1024); // 16 blocks
        let mut scheduler = SegmentScheduler::new(blocks, 5, Duration::from_millis(100));

        // First worker claims the entire range
        let claim1 = scheduler.claim(Instant::now(), Some("eth0".into()));
        let Claim::Segment(lease1) = claim1 else {
            panic!("expected Segment claim");
        };
        assert_eq!(lease1.start, 0);
        assert_eq!(lease1.end_at_request, 16 * 1024 * 1024);
        assert_eq!(lease1.to_range_header(), "bytes=0-16777215");

        // Second worker steals the upper half
        let claim2 = scheduler.claim(Instant::now(), Some("wlan0".into()));
        let Claim::Segment(lease2) = claim2 else {
            panic!("expected second worker to steal segment");
        };

        // Check lease1 shrunk
        let b1 = lease1.bounds.lock().unwrap();
        assert_eq!(b1.reserved, 0);
        assert_eq!(b1.end, 8 * 1024 * 1024); // split at midpoint block 8

        // Check lease2 covers the upper half
        let b2 = lease2.bounds.lock().unwrap();
        assert_eq!(lease2.start, 8 * 1024 * 1024);
        assert_eq!(b2.reserved, 8 * 1024 * 1024);
        assert_eq!(b2.end, 16 * 1024 * 1024);
        assert_eq!(lease2.end_at_request, 16 * 1024 * 1024);

        drop(b1);
        drop(b2);

        // Third worker splits the larger segment (lease1 or lease2 both have 8 blocks)
        let claim3 = scheduler.claim(Instant::now(), Some("usb0".into()));
        assert!(matches!(claim3, Claim::Segment(_)));
    }

    #[test]
    fn test_scheduler_commit_and_finish() {
        let blocks = plan_chunks(4 * 1024 * 1024, 1024 * 1024); // 4 blocks
        let mut scheduler = SegmentScheduler::new(blocks, 3, Duration::from_millis(50));

        let Claim::Segment(lease) = scheduler.claim(Instant::now(), None) else {
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
            scheduler.claim(Instant::now(), None),
            Claim::Finished
        ));
    }

    #[test]
    fn test_scheduler_fail_with_progress_resets_attempts_and_no_backoff() {
        let blocks = plan_chunks(4 * 1024 * 1024, 1024 * 1024);
        let mut scheduler = SegmentScheduler::new(blocks, 3, Duration::from_millis(500));

        let Claim::Segment(lease) = scheduler.claim(Instant::now(), None) else {
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
        let Claim::Segment(lease_retry) = scheduler.claim(Instant::now(), None) else {
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
        let Claim::Segment(l1) = scheduler.claim(now, None) else {
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
        assert!(matches!(scheduler.claim(now, None), Claim::Wait(_)));

        // After backoff, claim succeeds
        let Claim::Segment(l2) = scheduler.claim(now + Duration::from_millis(100), None) else {
            panic!("expected retry segment");
        };
        assert_eq!(l2.attempts, 1);

        // Attempt 2 fails -> max attempts reached -> permanent failure
        let outcome2 = scheduler.fail(l2.id, now + Duration::from_millis(100));
        assert_eq!(outcome2, FailOutcome::Failed);

        // Scheduler finishes with failure
        assert!(matches!(
            scheduler.claim(now + Duration::from_secs(1), None),
            Claim::Finished
        ));
        assert_eq!(scheduler.chunk_map(), "!!");
    }

    #[test]
    fn test_scheduler_release_has_no_penalty() {
        let blocks = plan_chunks(2 * 1024 * 1024, 1024 * 1024);
        let mut scheduler = SegmentScheduler::new(blocks, 3, Duration::from_millis(100));

        let Claim::Segment(lease) = scheduler.claim(Instant::now(), None) else {
            panic!("expected segment");
        };
        scheduler.release(lease.id);

        let Claim::Segment(lease_again) = scheduler.claim(Instant::now(), None) else {
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
        let Claim::Segment(l1) = scheduler.claim(Instant::now(), None) else {
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
    #[allow(clippy::needless_range_loop)]
    fn test_scheduler_random_sequence_consistency() {
        let n_blocks = 32;
        let block_size = 1024 * 1024;
        let blocks = plan_chunks(n_blocks * block_size, block_size);
        let mut scheduler = SegmentScheduler::new(blocks, 3, Duration::from_millis(10));

        let mut rng_state: u64 = 0xdeadbeef12345678;
        let mut next_rand = || -> u64 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            rng_state
        };

        let mut completed_history = std::collections::HashSet::new();

        for _ in 0..500 {
            let action = next_rand() % 5;
            let now = Instant::now();

            match action {
                0 => {
                    // Claim
                    let _ = scheduler.claim(now, Some("test".into()));
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
    }
}
