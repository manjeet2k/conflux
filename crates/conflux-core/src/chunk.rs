use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub id: usize,
    pub start: u64,
    pub end: u64,
    pub size: u64,
    pub downloaded: u64,
    pub status: ChunkStatus,
    pub adapter_ip: Option<String>,
    /// Number of download attempts started for this chunk so far.
    #[serde(default)]
    pub attempts: u32,
}

impl Chunk {
    pub fn new(id: usize, start: u64, end: u64) -> Self {
        let size = end.saturating_sub(start) + 1;
        Self {
            id,
            start,
            end,
            size,
            downloaded: 0,
            status: ChunkStatus::Pending,
            adapter_ip: None,
            attempts: 0,
        }
    }

    /// Formats the byte range as an HTTP Range header value: `bytes=start-end`
    pub fn to_range_header(&self) -> String {
        format!("bytes={}-{}", self.start, self.end)
    }

    pub fn is_complete(&self) -> bool {
        self.downloaded >= self.size
    }
}

/// Splits a file of `total_bytes` into non-overlapping, contiguous chunks of at most `chunk_size`.
/// Returns an empty plan when `total_bytes == 0` or `chunk_size == 0`.
pub fn plan_chunks(total_bytes: u64, chunk_size: u64) -> Vec<Chunk> {
    if total_bytes == 0 || chunk_size == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current_offset = 0;
    let mut chunk_id = 0;

    while current_offset < total_bytes {
        let remaining = total_bytes - current_offset;
        let this_chunk_size = remaining.min(chunk_size);
        let end_offset = current_offset + this_chunk_size - 1;

        chunks.push(Chunk::new(chunk_id, current_offset, end_offset));
        current_offset += this_chunk_size;
        chunk_id += 1;
    }

    chunks
}

/// Result of asking the scheduler for work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Claim {
    /// A chunk was handed out and is now `Downloading`.
    Chunk(Chunk),
    /// Nothing is claimable right now (chunks in flight or in backoff); retry after this delay.
    Wait(Duration),
    /// No more work: every chunk is completed, or some chunk failed permanently.
    Finished,
}

/// Result of reporting a failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailOutcome {
    /// The chunk is back to `Pending` and may be reclaimed after `after`.
    Retry { after: Duration },
    /// The chunk exhausted its attempts and is now `Failed`.
    Failed,
}

struct Slot {
    chunk: Chunk,
    not_before: Option<Instant>,
}

/// Work queue of chunks with bounded retries and exponential backoff.
///
/// State machine per chunk:
/// `Pending --claim--> Downloading --complete--> Completed`
/// `Downloading --fail--> Pending (backoff) | Failed (attempts exhausted)`
/// `Downloading --release--> Pending (no penalty; used when stopping)`
pub(crate) struct ChunkScheduler {
    slots: Vec<Slot>,
    max_attempts: u32,
    backoff_base: Duration,
}

/// How long to wait when only in-flight chunks remain.
const IDLE_POLL: Duration = Duration::from_millis(50);

impl ChunkScheduler {
    pub(crate) fn new(chunks: Vec<Chunk>, max_attempts: u32, backoff_base: Duration) -> Self {
        Self {
            slots: chunks
                .into_iter()
                .map(|chunk| Slot {
                    chunk,
                    not_before: None,
                })
                .collect(),
            max_attempts: max_attempts.max(1),
            backoff_base,
        }
    }

    /// Backoff before a chunk that has failed `failed_attempts` times may be reclaimed:
    /// base, 2*base, 4*base, 8*base, then capped at 16*base (0.5s..8s with the default base).
    pub(crate) fn backoff_for(&self, failed_attempts: u32) -> Duration {
        let exponent = failed_attempts.saturating_sub(1).min(4);
        self.backoff_base.saturating_mul(1u32 << exponent)
    }

    pub(crate) fn claim(&mut self, now: Instant, adapter_label: Option<String>) -> Claim {
        if self
            .slots
            .iter()
            .any(|s| s.chunk.status == ChunkStatus::Failed)
        {
            return Claim::Finished;
        }

        let mut soonest: Option<Duration> = None;
        let mut in_flight = false;
        for slot in self.slots.iter_mut() {
            match slot.chunk.status {
                ChunkStatus::Pending => {
                    let ready_in = slot
                        .not_before
                        .map(|t| t.saturating_duration_since(now))
                        .unwrap_or(Duration::ZERO);
                    if ready_in.is_zero() {
                        slot.chunk.status = ChunkStatus::Downloading;
                        slot.chunk.attempts += 1;
                        slot.chunk.adapter_ip = adapter_label;
                        slot.not_before = None;
                        return Claim::Chunk(slot.chunk.clone());
                    }
                    soonest = Some(soonest.map_or(ready_in, |s| s.min(ready_in)));
                }
                ChunkStatus::Downloading => in_flight = true,
                ChunkStatus::Completed | ChunkStatus::Failed => {}
            }
        }

        match (soonest, in_flight) {
            (Some(d), true) => Claim::Wait(d.min(IDLE_POLL)),
            (Some(d), false) => Claim::Wait(d),
            (None, true) => Claim::Wait(IDLE_POLL),
            (None, false) => Claim::Finished,
        }
    }

    pub(crate) fn complete(&mut self, id: usize) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.chunk.id == id) {
            slot.chunk.status = ChunkStatus::Completed;
            slot.chunk.downloaded = slot.chunk.size;
        }
    }

    pub(crate) fn fail(&mut self, id: usize, now: Instant) -> FailOutcome {
        let max_attempts = self.max_attempts;
        let Some(idx) = self.slots.iter().position(|s| s.chunk.id == id) else {
            return FailOutcome::Failed;
        };
        let attempts = self.slots[idx].chunk.attempts;
        let backoff = self.backoff_for(attempts);
        let slot = &mut self.slots[idx];
        slot.chunk.downloaded = 0;
        if attempts >= max_attempts {
            slot.chunk.status = ChunkStatus::Failed;
            slot.not_before = None;
            FailOutcome::Failed
        } else {
            slot.chunk.status = ChunkStatus::Pending;
            slot.not_before = Some(now + backoff);
            FailOutcome::Retry { after: backoff }
        }
    }

    /// Returns an in-flight chunk to `Pending` without counting the attempt (e.g. on cancel).
    pub(crate) fn release(&mut self, id: usize) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.chunk.id == id) {
            if slot.chunk.status == ChunkStatus::Downloading {
                slot.chunk.status = ChunkStatus::Pending;
                slot.chunk.attempts = slot.chunk.attempts.saturating_sub(1);
                slot.chunk.downloaded = 0;
            }
        }
    }

    /// Ids of all `Completed` chunks, ascending by position.
    pub(crate) fn completed_ids(&self) -> Vec<usize> {
        self.ids_with_status(ChunkStatus::Completed)
    }

    /// One char per chunk in plan order: `.` pending, `>` downloading, `#` completed, `!` failed.
    pub(crate) fn chunk_map(&self) -> String {
        self.slots
            .iter()
            .map(|s| match s.chunk.status {
                ChunkStatus::Pending => '.',
                ChunkStatus::Downloading => '>',
                ChunkStatus::Completed => '#',
                ChunkStatus::Failed => '!',
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn all_completed(&self) -> bool {
        self.slots
            .iter()
            .all(|s| s.chunk.status == ChunkStatus::Completed)
    }

    pub(crate) fn ids_with_status(&self, status: ChunkStatus) -> Vec<usize> {
        self.slots
            .iter()
            .filter(|s| s.chunk.status == status)
            .map(|s| s.chunk.id)
            .collect()
    }

    pub(crate) fn incomplete_ids(&self) -> Vec<usize> {
        self.slots
            .iter()
            .filter(|s| s.chunk.status != ChunkStatus::Completed)
            .map(|s| s.chunk.id)
            .collect()
    }

    #[cfg(test)]
    fn chunk(&self, id: usize) -> &Chunk {
        &self.slots.iter().find(|s| s.chunk.id == id).unwrap().chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_chunks_exact_division() {
        let chunks = plan_chunks(10 * 1024 * 1024, 5 * 1024 * 1024);
        assert_eq!(chunks.len(), 2);

        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks[0].end, 5 * 1024 * 1024 - 1);
        assert_eq!(chunks[0].size, 5 * 1024 * 1024);
        assert_eq!(chunks[0].to_range_header(), "bytes=0-5242879");

        assert_eq!(chunks[1].start, 5 * 1024 * 1024);
        assert_eq!(chunks[1].end, 10 * 1024 * 1024 - 1);
        assert_eq!(chunks[1].size, 5 * 1024 * 1024);
        assert_eq!(chunks[1].to_range_header(), "bytes=5242880-10485759");

        let total: u64 = chunks.iter().map(|c| c.size).sum();
        assert_eq!(total, 10 * 1024 * 1024);
    }

    #[test]
    fn test_plan_chunks_non_exact_division() {
        let total_bytes = 10;
        let chunk_size = 4;
        let chunks = plan_chunks(total_bytes, chunk_size);

        assert_eq!(chunks.len(), 3);
        assert_eq!((chunks[0].start, chunks[0].end, chunks[0].size), (0, 3, 4));
        assert_eq!((chunks[1].start, chunks[1].end, chunks[1].size), (4, 7, 4));
        assert_eq!((chunks[2].start, chunks[2].end, chunks[2].size), (8, 9, 2));

        let total: u64 = chunks.iter().map(|c| c.size).sum();
        assert_eq!(total, total_bytes);
    }

    #[test]
    fn test_plan_chunks_file_smaller_than_chunk_size() {
        let chunks = plan_chunks(500, 1024);
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            (chunks[0].start, chunks[0].end, chunks[0].size),
            (0, 499, 500)
        );
    }

    #[test]
    fn test_plan_chunks_zero_bytes() {
        assert!(plan_chunks(0, 1024).is_empty());
        assert!(plan_chunks(1024, 0).is_empty());
    }

    /// Invariant: for any total > 0 and chunk_size > 0, chunks cover [0, total-1]
    /// contiguously with zero gaps, zero overlaps, sequential ids and sizes <= chunk_size.
    #[test]
    fn test_plan_chunks_invariant_property() {
        let mut totals: Vec<u64> = (1..=300).collect();
        totals.extend([1023, 1024, 1025, 4096 * 7 + 3, 10_000_019, u64::MAX / 2]);
        let chunk_sizes: Vec<u64> = vec![1, 2, 3, 7, 16, 64, 100, 255, 256, 1000, 4096, 1 << 20];

        for &total in &totals {
            for &chunk_size in &chunk_sizes {
                // Keep the huge total cheap: only large chunk sizes.
                if total / chunk_size > 200_000 {
                    continue;
                }
                let chunks = plan_chunks(total, chunk_size);
                assert!(!chunks.is_empty());
                assert_eq!(chunks.len() as u64, total.div_ceil(chunk_size));
                let mut expected_start = 0u64;
                for (i, c) in chunks.iter().enumerate() {
                    assert_eq!(c.id, i);
                    assert_eq!(c.start, expected_start, "gap/overlap at chunk {}", i);
                    assert!(c.end >= c.start);
                    assert_eq!(c.size, c.end - c.start + 1);
                    assert!(c.size <= chunk_size);
                    assert_eq!(c.status, ChunkStatus::Pending);
                    expected_start = c.end + 1;
                }
                assert_eq!(chunks.last().unwrap().end, total - 1);
            }
        }
    }

    fn scheduler(n_chunks: u64, max_attempts: u32, base_ms: u64) -> ChunkScheduler {
        ChunkScheduler::new(
            plan_chunks(n_chunks * 10, 10),
            max_attempts,
            Duration::from_millis(base_ms),
        )
    }

    #[test]
    fn test_scheduler_claims_each_chunk_once() {
        let mut s = scheduler(3, 5, 100);
        let now = Instant::now();
        let mut ids = Vec::new();
        for _ in 0..3 {
            match s.claim(now, None) {
                Claim::Chunk(c) => {
                    assert_eq!(c.status, ChunkStatus::Downloading);
                    assert_eq!(c.attempts, 1);
                    ids.push(c.id);
                }
                other => panic!("expected chunk, got {:?}", other),
            }
        }
        assert_eq!(ids, vec![0, 1, 2]);
        assert!(matches!(s.claim(now, None), Claim::Wait(_)));
        for id in ids {
            s.complete(id);
        }
        assert!(s.all_completed());
        assert_eq!(s.claim(now, None), Claim::Finished);
    }

    #[test]
    fn test_scheduler_backoff_sequence_and_cap() {
        let s = scheduler(1, 5, 500);
        let secs: Vec<f64> = (1..=7).map(|a| s.backoff_for(a).as_secs_f64()).collect();
        assert_eq!(secs, vec![0.5, 1.0, 2.0, 4.0, 8.0, 8.0, 8.0]);
    }

    #[test]
    fn test_scheduler_retry_respects_backoff_then_fails_after_max_attempts() {
        let mut s = scheduler(2, 5, 100);
        let mut now = Instant::now();

        // Chunk 1 completes normally.
        let Claim::Chunk(c0) = s.claim(now, None) else {
            panic!()
        };
        let Claim::Chunk(c1) = s.claim(now, None) else {
            panic!()
        };
        s.complete(c1.id);

        for attempt in 1..=5u32 {
            let outcome = s.fail(c0.id, now);
            if attempt < 5 {
                let expected = s.backoff_for(attempt);
                assert_eq!(outcome, FailOutcome::Retry { after: expected });
                assert_eq!(s.chunk(c0.id).status, ChunkStatus::Pending);
                // Not claimable before the backoff elapses.
                match s.claim(now, None) {
                    Claim::Wait(d) => assert_eq!(d, expected),
                    other => panic!("expected wait, got {:?}", other),
                }
                now += expected;
                let Claim::Chunk(again) = s.claim(now, Some("10.0.0.1".into())) else {
                    panic!("chunk should be claimable after backoff")
                };
                assert_eq!(again.id, c0.id);
                assert_eq!(again.attempts, attempt + 1);
                assert_eq!(again.adapter_ip.as_deref(), Some("10.0.0.1"));
            } else {
                assert_eq!(outcome, FailOutcome::Failed);
            }
        }
        assert_eq!(s.chunk(c0.id).status, ChunkStatus::Failed);
        assert_eq!(s.ids_with_status(ChunkStatus::Failed), vec![c0.id]);
        assert_eq!(s.incomplete_ids(), vec![c0.id]);
        assert_eq!(s.claim(now, None), Claim::Finished);
        assert!(!s.all_completed());
    }

    #[test]
    fn test_scheduler_failed_chunk_stops_new_claims() {
        let mut s = scheduler(3, 1, 10);
        let now = Instant::now();
        let Claim::Chunk(c0) = s.claim(now, None) else {
            panic!()
        };
        assert_eq!(s.fail(c0.id, now), FailOutcome::Failed);
        // Remaining pending chunks are not handed out once the download is doomed.
        assert_eq!(s.claim(now, None), Claim::Finished);
    }

    #[test]
    fn test_scheduler_release_does_not_count_attempt() {
        let mut s = scheduler(1, 5, 10);
        let now = Instant::now();
        let Claim::Chunk(c0) = s.claim(now, None) else {
            panic!()
        };
        s.release(c0.id);
        assert_eq!(s.chunk(c0.id).status, ChunkStatus::Pending);
        assert_eq!(s.chunk(c0.id).attempts, 0);
        assert!(matches!(s.claim(now, None), Claim::Chunk(_)));
    }

    /// Randomised state-consistency check: whatever sequence of claims/completions/failures
    /// happens, every chunk ends in exactly one terminal state and nothing is lost.
    #[test]
    fn test_scheduler_random_sequence_consistency() {
        let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        for _round in 0..200 {
            let n = (next() % 20 + 1) as usize;
            let mut s = ChunkScheduler::new(plan_chunks(n as u64 * 4, 4), 5, Duration::ZERO);
            let now = Instant::now();
            let mut in_flight: Vec<usize> = Vec::new();
            loop {
                match s.claim(now, None) {
                    Claim::Chunk(c) => in_flight.push(c.id),
                    Claim::Finished => break,
                    Claim::Wait(_) => {}
                }
                if !in_flight.is_empty() && next() % 2 == 0 {
                    let idx = (next() as usize) % in_flight.len();
                    let id = in_flight.swap_remove(idx);
                    if next() % 4 == 0 {
                        s.fail(id, now);
                    } else {
                        s.complete(id);
                    }
                }
            }
            // Drain anything still in flight.
            for id in in_flight {
                s.complete(id);
            }
            let completed = s.ids_with_status(ChunkStatus::Completed).len();
            let failed = s.ids_with_status(ChunkStatus::Failed).len();
            let pending = s.ids_with_status(ChunkStatus::Pending).len();
            assert!(s.ids_with_status(ChunkStatus::Downloading).is_empty());
            assert_eq!(completed + failed + pending, s.len());
            if failed == 0 {
                assert!(s.all_completed());
            }
            for slot in &s.slots {
                assert!(slot.chunk.attempts <= 5);
            }
        }
    }

    #[test]
    fn test_scheduler_skips_precompleted_chunks_and_maps_states() {
        let mut chunks = plan_chunks(4 * 10, 10);
        chunks[1].status = ChunkStatus::Completed;
        chunks[1].downloaded = chunks[1].size;
        chunks[3].status = ChunkStatus::Completed;
        chunks[3].downloaded = chunks[3].size;
        let mut s = ChunkScheduler::new(chunks, 2, Duration::from_millis(10));
        assert_eq!(s.chunk_map(), ".#.#");
        assert_eq!(s.completed_ids(), vec![1, 3]);

        let now = Instant::now();
        let Claim::Chunk(a) = s.claim(now, None) else {
            panic!("expected a chunk")
        };
        assert_eq!(a.id, 0);
        assert_eq!(s.chunk_map(), ">#.#");
        let Claim::Chunk(b) = s.claim(now, None) else {
            panic!("expected a chunk")
        };
        assert_eq!(b.id, 2, "pre-completed chunk 1 must never be claimed");
        s.complete(0);
        s.complete(2);
        assert!(s.all_completed());
        assert_eq!(s.chunk_map(), "####");
        assert_eq!(s.claim(now, None), Claim::Finished);
    }

    #[test]
    fn test_chunk_map_marks_failed() {
        let mut s = scheduler(2, 1, 1);
        let now = Instant::now();
        let Claim::Chunk(c) = s.claim(now, None) else {
            panic!("expected a chunk")
        };
        assert_eq!(s.fail(c.id, now), FailOutcome::Failed);
        assert_eq!(s.chunk_map(), "!.");
    }
}
