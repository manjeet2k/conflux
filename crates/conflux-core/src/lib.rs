pub mod adapter;
pub mod checksum;
pub mod chunk;
pub mod engine;
pub mod filename;
pub mod resume;
pub mod watcher;
pub mod writer;

pub use adapter::{
    build_bound_http_client, discover_adapters, is_link_local, looks_virtual, NetworkAdapter,
};
pub use checksum::compute_sha256;
pub use chunk::{plan_chunks, Chunk, ChunkStatus};
pub use engine::{
    AdapterProgress, AdapterUpdate, DownloadCancelled, DownloadEngine, DownloadProbe,
    ProgressUpdate,
};
pub use filename::{claim_unique_path, sanitize_filename, unique_path};
pub use resume::{remove_resume_sidecar, resume_sidecar_path};
pub use watcher::{diff_adapters, NetworkWatcher, DEFAULT_DEBOUNCE};
pub use writer::SparseFileWriter;
