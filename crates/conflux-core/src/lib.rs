pub mod adapter;
pub mod checksum;
pub mod chunk;
pub mod engine;
pub mod writer;

pub use adapter::{discover_adapters, NetworkAdapter};
pub use checksum::compute_sha256;
pub use chunk::{plan_chunks, Chunk, ChunkStatus};
pub use engine::{DownloadEngine, DownloadProbe, ProgressUpdate};
pub use writer::SparseFileWriter;
