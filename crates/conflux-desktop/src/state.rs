use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};

/// Lifecycle status of a download task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Downloading,
    /// Stopped by the user (or by quitting the app); resumable from its sidecar.
    Paused,
    Completed,
    /// Failed; resumable from its sidecar as well.
    Error,
}

/// Live per-adapter throughput of one task (mirrors `conflux_core::AdapterProgress`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterStat {
    /// Id of the matching `AdapterInfo`, or `None` for unbound default routing.
    pub adapter_id: Option<String>,
    pub name: String,
    pub ip: Option<String>,
    pub downloaded_bytes: u64,
    pub speed_bytes_sec: f64,
    pub active_connections: usize,
    pub dropped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTaskState {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub save_dir: String,
    pub save_path: String,
    /// Adapter ids the user selected (empty = discovery defaults). Reused on resume.
    #[serde(default)]
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
    #[serde(default)]
    pub completed_at_ms: Option<u64>,
    #[serde(default)]
    pub adapters: Vec<AdapterStat>,
    /// Last chunk map reported by the engine (see `conflux_core::ProgressUpdate::chunk_map`).
    #[serde(default)]
    pub chunk_map: Option<String>,
}

impl DownloadTaskState {
    /// Zeroes the fields that only mean something while the task is running.
    pub fn clear_runtime(&mut self) {
        self.speed_bytes_sec = 0.0;
        self.eta_seconds = 0;
        self.active_chunks = 0;
        for a in &mut self.adapters {
            a.speed_bytes_sec = 0.0;
            a.active_connections = 0;
        }
        if let Some(map) = &mut self.chunk_map {
            *map = map.replace('>', ".");
        }
    }
}

pub struct TaskHandle {
    pub cancel_tx: watch::Sender<bool>,
    pub adapter_tx: Option<tokio::sync::mpsc::Sender<conflux_core::AdapterUpdate>>,
    pub names: Arc<RwLock<HashMap<std::net::IpAddr, (String, String)>>>,
    pub join_handle: tokio::task::JoinHandle<()>,
    pub output_path: PathBuf,
}

pub struct AppState {
    pub tasks: Arc<RwLock<HashMap<String, DownloadTaskState>>>,
    pub handles: Arc<Mutex<HashMap<String, TaskHandle>>>,
    /// Output paths claimed by running tasks, so two concurrent downloads never
    /// resolve to the same file before either has created it on disk.
    pub reserved_paths: Arc<Mutex<HashSet<PathBuf>>>,
    pub settings: Arc<RwLock<Settings>>,
    /// `None` when the config dir could not be resolved (settings are then not saved).
    pub settings_path: Option<PathBuf>,
    pub history: Arc<crate::history::HistoryStore>,
    /// Last known snapshot of network adapters with user overrides applied,
    /// shared between background NetworkWatcher and UI toggle commands.
    pub last_adapters: Arc<RwLock<Vec<conflux_core::NetworkAdapter>>>,
}

impl AppState {
    pub fn new(
        settings: Settings,
        settings_path: Option<PathBuf>,
        history: crate::history::HistoryStore,
        tasks: Vec<DownloadTaskState>,
    ) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(
                tasks.into_iter().map(|t| (t.id.clone(), t)).collect(),
            )),
            handles: Arc::new(Mutex::new(HashMap::new())),
            reserved_paths: Arc::new(Mutex::new(HashSet::new())),
            settings: Arc::new(RwLock::new(settings)),
            settings_path,
            history: Arc::new(history),
            last_adapters: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Field names are the IPC contract with `ui/src/types.ts` (`DownloadTask`).
    #[test]
    fn download_task_serializes_contract_fields() {
        let task = DownloadTaskState {
            id: "a".into(),
            url: "u".into(),
            filename: "f".into(),
            save_dir: "d".into(),
            save_path: "p".into(),
            adapter_ids: vec![],
            total_bytes: 1,
            supports_ranges: true,
            downloaded_bytes: 0,
            status: TaskStatus::Paused,
            speed_bytes_sec: 0.0,
            eta_seconds: 0,
            active_chunks: 0,
            completed_chunks: 0,
            total_chunks: 0,
            sha256: None,
            error: None,
            created_at_ms: 0,
            completed_at_ms: None,
            adapters: vec![],
            chunk_map: None,
        };
        let json = serde_json::to_value(&task).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort_unstable();
        let mut expected = vec![
            "id",
            "url",
            "filename",
            "save_dir",
            "save_path",
            "adapter_ids",
            "total_bytes",
            "supports_ranges",
            "downloaded_bytes",
            "status",
            "speed_bytes_sec",
            "eta_seconds",
            "active_chunks",
            "completed_chunks",
            "total_chunks",
            "sha256",
            "error",
            "created_at_ms",
            "completed_at_ms",
            "adapters",
            "chunk_map",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert_eq!(json["status"], "paused");
        for (status, name) in [
            (TaskStatus::Downloading, "downloading"),
            (TaskStatus::Completed, "completed"),
            (TaskStatus::Error, "error"),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), name);
        }
    }
}
