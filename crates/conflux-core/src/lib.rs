pub mod adapter;
pub mod checksum;
pub mod chunk;
pub mod engine;
pub mod file_identity;
pub mod filename;
pub mod resume;
pub mod segment;
pub mod watcher;
pub mod writer;

pub use adapter::{
    build_bound_http_client, discover_adapters, is_link_local, looks_virtual, NetworkAdapter,
};
pub use checksum::compute_sha256;
pub use chunk::{plan_chunks, Chunk, ChunkStatus};
pub use engine::{
    AdapterProgress, AdapterUpdate, DownloadCancelled, DownloadEngine, DownloadProbe,
    ProgressUpdate, RequestHeaders,
};
pub use file_identity::{
    publish_owned_file, remove_owned_empty_file, remove_owned_file, FileIdentity,
};
pub use filename::{claim_unique_path, sanitize_filename};
pub use resume::{
    has_resume_sidecar, read_resume_headers, remove_resume_sidecar, resume_sidecar_path,
};
pub use segment::{
    block_size_for, Claim, FailOutcome, SegmentBounds, SegmentLease, SegmentScheduler,
};
pub use watcher::{diff_adapters, NetworkWatcher, DEFAULT_DEBOUNCE};
pub use writer::SparseFileWriter;
