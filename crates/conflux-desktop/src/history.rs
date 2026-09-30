//! Download history persisted as JSON so the list survives app restarts.

use crate::settings::write_atomic;
use crate::state::{DownloadTaskState, TaskStatus};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::{Mutex, RwLock};
use tracing::warn;

pub struct HistoryStore {
    /// `None` disables persistence (data dir could not be resolved).
    path: Option<PathBuf>,
    /// Serializes snapshot + write so an older snapshot never overwrites a newer one.
    write_lock: Mutex<()>,
}

impl HistoryStore {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            write_lock: Mutex::new(()),
        }
    }

    /// Loads saved tasks, newest first. Tasks that were running when the app exited
    /// become `Paused` (they can be resumed from their sidecar).
    pub fn load(&self) -> Vec<DownloadTaskState> {
        let Some(path) = &self.path else {
            return Vec::new();
        };
        load_from(path)
    }

    /// Writes all tasks. Errors are logged, never propagated: history is best-effort.
    pub async fn save(&self, tasks: &RwLock<HashMap<String, DownloadTaskState>>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let _guard = self.write_lock.lock().await;
        let mut snapshot: Vec<DownloadTaskState> = tasks.read().await.values().cloned().collect();
        snapshot.sort_by_key(|t| std::cmp::Reverse(t.created_at_ms));
        let result = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            let json = serde_json::to_vec(&snapshot)?;
            write_atomic(&path, &json)
        })
        .await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => warn!("Failed to save download history: {e}"),
            Err(e) => warn!("Download history save task panicked: {e:?}"),
        }
    }
}

fn load_from(path: &Path) -> Vec<DownloadTaskState> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            warn!(path = %path.display(), "Cannot read download history: {e}");
            return Vec::new();
        }
    };
    let mut tasks: Vec<DownloadTaskState> = match serde_json::from_str(&text) {
        Ok(t) => t,
        Err(e) => {
            warn!(path = %path.display(), "Invalid download history, starting empty: {e}");
            return Vec::new();
        }
    };
    for task in &mut tasks {
        if task.status == TaskStatus::Downloading {
            task.status = TaskStatus::Paused;
        }
        task.clear_runtime();
    }
    tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at_ms));
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AdapterStat;

    fn task(id: &str, status: TaskStatus, created: u64) -> DownloadTaskState {
        DownloadTaskState {
            id: id.into(),
            url: "http://x/y".into(),
            filename: "y".into(),
            save_dir: "/tmp".into(),
            save_path: "/tmp/y".into(),
            adapter_ids: vec![],
            total_bytes: 100,
            supports_ranges: true,
            downloaded_bytes: 40,
            status,
            speed_bytes_sec: 12.0,
            eta_seconds: 5,
            active_chunks: 3,
            completed_chunks: 1,
            total_chunks: 4,
            sha256: None,
            error: None,
            created_at_ms: created,
            completed_at_ms: None,
            adapters: vec![AdapterStat {
                adapter_id: None,
                name: "Default route".into(),
                ip: None,
                downloaded_bytes: 40,
                speed_bytes_sec: 12.0,
                active_connections: 2,
                dropped: false,
            }],
            chunk_map: Some("#>..".into()),
        }
    }

    #[tokio::test]
    async fn save_then_load_pauses_running_tasks_and_clears_runtime_fields() {
        let dir = std::env::temp_dir().join(format!("conflux-history-{}", uuid::Uuid::new_v4()));
        let store = HistoryStore::new(Some(dir.join("downloads.json")));
        assert!(store.load().is_empty());

        let tasks = RwLock::new(HashMap::from([
            ("a".to_string(), task("a", TaskStatus::Downloading, 1)),
            ("b".to_string(), task("b", TaskStatus::Completed, 2)),
        ]));
        store.save(&tasks).await;

        let loaded = store.load();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "b", "newest first");
        let a = &loaded[1];
        assert_eq!(a.status, TaskStatus::Paused);
        assert_eq!(a.speed_bytes_sec, 0.0);
        assert_eq!(a.active_chunks, 0);
        assert_eq!(a.downloaded_bytes, 40);
        assert_eq!(a.adapters[0].active_connections, 0);
        assert_eq!(a.chunk_map.as_deref(), Some("#..."));
        assert_eq!(loaded[0].status, TaskStatus::Completed);

        std::fs::write(dir.join("downloads.json"), b"not json").unwrap();
        assert!(store.load().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn older_history_without_new_fields_loads() {
        let dir = std::env::temp_dir().join(format!("conflux-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("downloads.json");
        let mut value = serde_json::to_value(vec![task("a", TaskStatus::Error, 1)]).unwrap();
        for key in ["completed_at_ms", "adapters", "chunk_map"] {
            value[0].as_object_mut().unwrap().remove(key);
        }
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].status, TaskStatus::Error);
        assert!(loaded[0].adapters.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
