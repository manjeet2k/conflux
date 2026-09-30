use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};

/// Lifecycle status of a download task, shared by `DownloadTaskState` and `ProgressEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Downloading,
    Completed,
    /// Stopped by the user. There is no resume yet; the UI offers a restart from zero.
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTaskState {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub save_dir: String,
    pub save_path: String,
    /// Adapter ids the user selected (empty = discovery defaults). Kept so a restart can reuse them.
    pub adapter_ids: Vec<String>,
    pub total_bytes: u64,
    pub supports_ranges: bool,
    pub downloaded_bytes: u64,
    pub status: TaskStatus,
    pub speed_bytes_sec: f64,
    pub eta_seconds: u64,
    pub active_chunks: usize,
    pub completed_chunks: usize,
    pub total_chunks: usize,
    pub sha256: Option<String>,
    pub error: Option<String>,
    /// Unix epoch milliseconds, used to list tasks newest-first.
    pub created_at_ms: u64,
}

pub struct TaskHandle {
    pub cancel_tx: watch::Sender<bool>,
    pub join_handle: tokio::task::JoinHandle<()>,
    pub output_path: PathBuf,
}

pub struct AppState {
    pub tasks: Arc<RwLock<HashMap<String, DownloadTaskState>>>,
    pub handles: Arc<Mutex<HashMap<String, TaskHandle>>>,
    /// Output paths claimed by running tasks, so two concurrent downloads never
    /// resolve to the same file before either has created it on disk.
    pub reserved_paths: Arc<Mutex<HashSet<PathBuf>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(Mutex::new(HashMap::new())),
            reserved_paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
