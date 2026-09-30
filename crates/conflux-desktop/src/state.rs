use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTaskState {
    pub id: String,
    pub url: String,
    pub mirrors: Vec<String>,
    pub filename: String,
    pub save_path: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub status: String,
    pub speed_bytes_sec: f64,
    pub eta_seconds: u64,
    pub sha256: Option<String>,
    pub error: Option<String>,
}

pub struct TaskHandle {
    pub cancel_tx: watch::Sender<bool>,
    pub join_handle: tokio::task::JoinHandle<()>,
}

pub struct AppState {
    pub tasks: Arc<RwLock<HashMap<String, DownloadTaskState>>>,
    pub handles: Arc<Mutex<HashMap<String, TaskHandle>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
