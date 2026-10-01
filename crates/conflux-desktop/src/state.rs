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
    /// Human-readable cause of the adapter's latest failed attempt in this task (no URLs).
    #[serde(default)]
    pub last_error: Option<String>,
    /// Why the engine stopped using this adapter for this task.
    #[serde(default)]
    pub drop_reason: Option<String>,
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
    /// Adapter id -> `enabled` as discovery reported it, before user overrides. Written by
    /// `refresh_adapters` (under the `last_adapters` lock), read when building UI records.
    /// A plain mutex, never held across an await.
    pub adapter_defaults: std::sync::Mutex<HashMap<String, bool>>,
    /// One-shot messages for the UI (e.g. a data file was reset), drained by the UI once.
    notices: std::sync::Mutex<Vec<String>>,
    /// Pending external download from browser extension or command line on cold start.
    pending_download: std::sync::Mutex<Option<crate::browser_bridge::ExternalDownloadPayload>>,
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
            adapter_defaults: std::sync::Mutex::new(HashMap::new()),
            notices: std::sync::Mutex::new(Vec::new()),
            pending_download: std::sync::Mutex::new(None),
        }
    }

    pub fn set_pending_download(&self, payload: crate::browser_bridge::ExternalDownloadPayload) {
        if let Ok(mut slot) = self.pending_download.lock() {
            *slot = Some(payload);
        }
    }

    pub fn take_pending_download(&self) -> Option<crate::browser_bridge::ExternalDownloadPayload> {
        self.pending_download.lock().ok()?.take()
    }

    pub fn push_notice(&self, notice: String) {
        if let Ok(mut notices) = self.notices.lock() {
            notices.push(notice);
        }
    }

    /// Returns and clears the pending notices, so each is shown once.
    pub fn take_notices(&self) -> Vec<String> {
        self.notices
            .lock()
            .map(|mut n| std::mem::take(&mut *n))
            .unwrap_or_default()
    }

    /// UI records for `adapters`, including why each was disabled by default.
    pub fn adapter_infos(
        &self,
        adapters: Vec<conflux_core::NetworkAdapter>,
    ) -> Vec<crate::adapters::AdapterInfo> {
        let defaults = self
            .adapter_defaults
            .lock()
            .map(|d| d.clone())
            .unwrap_or_default();
        crate::adapters::to_infos(adapters, &defaults)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_are_delivered_once() {
        let state = AppState::new(
            Settings::default(),
            None,
            crate::history::HistoryStore::new(None),
            vec![],
        );
        assert!(state.take_notices().is_empty());
        state.push_notice("a".into());
        state.push_notice("b".into());
        assert_eq!(state.take_notices(), ["a", "b"]);
        assert!(state.take_notices().is_empty());
    }

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
