use crate::controller::{TaskController, TaskEvents};
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Lifecycle status of a download task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Downloading,
    /// A stop was requested but the engine has not exited yet (e.g. a disk write is stuck on
    /// a stalled USB drive). The task still owns its file: it cannot be resumed or removed
    /// until the engine is gone, and then becomes `Paused` (or its final status).
    Stopping,
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
    /// Intended completed path; `save_path` is the physical .part file until published.
    /// Absent for legacy tasks, whose physical paths must never be inferred from suffixes.
    #[serde(default)]
    pub final_path: Option<String>,
    #[serde(default)]
    pub file_identity: Option<conflux_core::FileIdentity>,
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
    /// Cumulative active transfer duration in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// Average download speed across active duration in bytes/second.
    #[serde(default)]
    pub avg_speed_bytes_sec: Option<f64>,
    #[serde(default)]
    pub adapters: Vec<AdapterStat>,
    /// Last chunk map reported by the engine (see `conflux_core::ProgressUpdate::chunk_map`).
    #[serde(default)]
    pub chunk_map: Option<String>,
    /// In-process bookkeeping of [`TaskController`]; never serialized.
    #[serde(skip)]
    pub lifecycle: Lifecycle,
}

/// Per-task transition bookkeeping, only changed by [`TaskController`] under its task lock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lifecycle {
    /// Bumped by every lifecycle transition (start, resume, stop request, engine exit,
    /// removal). An operation that awaited something (a probe) commits only if the
    /// generation it started from is still current, so stale operations cannot act.
    pub generation: u64,
    /// A removal is in progress: no new operation may start on the task.
    pub removing: bool,
}

impl DownloadTaskState {
    /// Recover a crash after the rename but before the completion snapshot was saved.
    /// A saved hash is the receipt that the engine finished before publishing began.
    pub fn recover_published(&mut self) -> bool {
        let Some(path) = self.final_path.as_ref() else {
            return false;
        };
        let Some(identity) = self.file_identity.as_ref() else {
            return false;
        };
        if self.sha256.is_none()
            || conflux_core::FileIdentity::at(std::path::Path::new(path))
                .ok()
                .as_ref()
                != Some(identity)
        {
            return false;
        }
        self.save_path = path.clone();
        self.filename = std::path::Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into();
        self.final_path = None;
        self.status = TaskStatus::Completed;
        self.error = None;
        self.total_bytes = std::fs::metadata(&self.save_path).map_or(self.total_bytes, |m| m.len());
        self.downloaded_bytes = self.total_bytes;
        self.completed_chunks = self.total_chunks;
        self.completed_at_ms = Some(crate::controller::now_ms());
        self.clear_runtime();
        true
    }
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

pub struct AppState {
    /// Every download task: lifecycle transitions, engine handles and output paths.
    pub tasks: Arc<TaskController>,
    pub settings: Arc<RwLock<Settings>>,
    pub bandwidth_limiter: Arc<conflux_core::BandwidthLimiter>,
    /// `None` when the config dir could not be resolved (settings are then not saved).
    pub settings_path: Option<PathBuf>,
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
        events: Arc<dyn TaskEvents>,
    ) -> Self {
        Self {
            tasks: Arc::new(TaskController::new(tasks, Arc::new(history), events)),
            bandwidth_limiter: Arc::new(conflux_core::BandwidthLimiter::new(
                settings.bandwidth_limit(),
            )),
            settings: Arc::new(RwLock::new(settings)),
            settings_path,
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
            Arc::new(crate::controller::NoEvents),
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
            final_path: None,
            file_identity: None,
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
            duration_ms: 0,
            avg_speed_bytes_sec: None,
            adapters: vec![],
            chunk_map: None,
            lifecycle: Lifecycle {
                generation: 7,
                removing: true,
            },
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
            "final_path",
            "file_identity",
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
            "duration_ms",
            "avg_speed_bytes_sec",
            "adapters",
            "chunk_map",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert_eq!(json["status"], "paused");
        for (status, name) in [
            (TaskStatus::Downloading, "downloading"),
            (TaskStatus::Stopping, "stopping"),
            (TaskStatus::Completed, "completed"),
            (TaskStatus::Error, "error"),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), name);
        }
    }
}
