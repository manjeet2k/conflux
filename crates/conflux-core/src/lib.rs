pub mod adapter;
pub mod checksum;
pub mod chunk;
pub mod engine;
pub mod filename;
pub mod writer;

pub use adapter::{build_bound_http_client, discover_adapters, NetworkAdapter};
pub use checksum::compute_sha256;
pub use chunk::{plan_chunks, Chunk, ChunkStatus};
pub use engine::{DownloadCancelled, DownloadEngine, DownloadProbe, ProgressUpdate};
pub use filename::{sanitize_filename, unique_path};
pub use writer::SparseFileWriter;
