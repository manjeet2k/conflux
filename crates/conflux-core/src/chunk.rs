use serde::{Deserialize, Serialize};

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

/// Upper bound on the number of chunks one download may be planned into (~100 MB of
/// bookkeeping). Protects against hostile sizes from a probe or a tampered resume sidecar;
/// at the default 4 MiB chunk size it still allows files of ~8 TB.
pub const MAX_PLANNED_CHUNKS: u64 = 2_000_000;

/// Number of chunks `plan_chunks(total_bytes, chunk_size)` would produce (0 if either is 0).
pub fn chunk_count(total_bytes: u64, chunk_size: u64) -> u64 {
    if total_bytes == 0 || chunk_size == 0 {
        return 0;
    }
    total_bytes.div_ceil(chunk_size)
}

/// Like [`plan_chunks`], but refuses plans of more than [`MAX_PLANNED_CHUNKS`] chunks
/// before allocating anything.
pub fn try_plan_chunks(total_bytes: u64, chunk_size: u64) -> anyhow::Result<Vec<Chunk>> {
    let n = chunk_count(total_bytes, chunk_size);
    if n > MAX_PLANNED_CHUNKS {
        anyhow::bail!(
            "refusing to plan too many chunks: {} bytes at {} bytes per chunk is {} chunks (maximum {})",
            total_bytes,
            chunk_size,
            n,
            MAX_PLANNED_CHUNKS
        );
    }
    Ok(plan_chunks(total_bytes, chunk_size))
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
    fn test_try_plan_chunks_enforces_cap() {
        let cap = MAX_PLANNED_CHUNKS;
        // Exactly at the cap is fine and still satisfies the coverage invariant.
        let ok = try_plan_chunks(cap * 2, 2).unwrap();
        assert_eq!(ok.len() as u64, cap);
        assert_eq!(ok.last().unwrap().end, cap * 2 - 1);
        // One byte over the cap, absurd sizes and u64::MAX are refused without allocating.
        assert!(try_plan_chunks(cap * 2 + 1, 2).is_err());
        assert!(try_plan_chunks(1 << 60, 4 << 20).is_err());
        assert!(try_plan_chunks(u64::MAX, 1).is_err());
        assert!(try_plan_chunks(u64::MAX, u64::MAX).unwrap().len() == 1);
        assert!(try_plan_chunks(0, 10).unwrap().is_empty());
        assert_eq!(chunk_count(10, 3), 4);
        assert_eq!(chunk_count(u64::MAX, 1), u64::MAX);
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
}
