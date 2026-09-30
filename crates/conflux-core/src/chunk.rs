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
    fn test_plan_chunks_zero_bytes() {
        let chunks = plan_chunks(0, 1024);
        assert!(chunks.is_empty());
    }
}
