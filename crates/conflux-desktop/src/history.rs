//! Download history persisted as JSON so the list survives app restarts.
//!
//! One writer per store: callers only *request* a save, and the writer coalesces requests,
//! always serializing the newest state (so an older snapshot can never land after a newer
//! one). Lifecycle changes are written promptly; progress ticks at most once per
//! [`PROGRESS_SAVE_INTERVAL`] however many downloads are running. Write failures are
//! published on [`HistoryStore::status`] until a later write succeeds.

use crate::settings::write_atomic;
use crate::state::{DownloadTaskState, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, Notify, RwLock};
use tracing::warn;

/// Version of the on-disk history layout. Version 0 is the original bare JSON array.
pub const SCHEMA_VERSION: u32 = 2;

/// Minimum interval between writes caused by progress ticks alone.
pub const PROGRESS_SAVE_INTERVAL: Duration = Duration::from_secs(5);

/// Current layout: `{"schema_version": 2, "tasks": [...]}`; includes staging/publish intent.
#[derive(Serialize)]
struct OnDiskRef<'a> {
    schema_version: u32,
    tasks: &'a [DownloadTaskState],
}

#[derive(Deserialize)]
struct Versioned {
    tasks: Vec<DownloadTaskState>,
}

pub type TaskMap = RwLock<HashMap<String, DownloadTaskState>>;

#[derive(Default)]
struct Pending {
    /// Bumped by every save request.
    requested: u64,
    /// The last `requested` value a write (successful or not) has covered.
    written: u64,
    /// Some unwritten request is a lifecycle change: write without throttling.
    urgent: bool,
    /// A writer task is running (it exits once nothing is pending).
    running: bool,
    last_write: Option<Instant>,
}

pub struct HistoryStore {
    /// `None` disables persistence (data dir could not be resolved).
    path: Option<PathBuf>,
    /// Set when the file on disk was written by a newer version: it is never overwritten.
    read_only: AtomicBool,
    pending: std::sync::Mutex<Pending>,
    /// Wakes a writer that is waiting out the progress throttle.
    wake: Notify,
    /// The newest request number covered by a finished write.
    written: watch::Sender<u64>,
    /// `Some(reason)` while history cannot be saved; `None` once a write succeeds.
    status: watch::Sender<Option<String>>,
}

impl HistoryStore {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            read_only: AtomicBool::new(false),
            pending: std::sync::Mutex::new(Pending::default()),
            wake: Notify::new(),
            written: watch::channel(0).0,
            status: watch::channel(None).0,
        }
    }

    /// Loads saved tasks, newest first. Tasks that were running when the app exited
    /// become `Paused` (they can be resumed from their sidecar).
    /// An unparseable file is renamed to `downloads.json.corrupt-<timestamp>` (at most 3
    /// kept); the second value is then a notice for the UI. A file from a newer version is
    /// left exactly as it is, and this store then never writes it.
    pub fn load(&self) -> (Vec<DownloadTaskState>, Option<String>) {
        let Some(path) = &self.path else {
            return (Vec::new(), None);
        };
        let loaded = load_from(path);
        if loaded.newer_version {
            self.read_only.store(true, Ordering::SeqCst);
            self.status
                .send_replace(Some(NEWER_VERSION_STATUS.to_string()));
        }
        (loaded.tasks, loaded.notice)
    }

    /// Current save failure, if any. Changes are observable through the receiver.
    pub fn status(&self) -> watch::Receiver<Option<String>> {
        self.status.subscribe()
    }

    /// Requests a prompt write of `tasks` (after a lifecycle change). Never blocks.
    pub fn request_save(self: &Arc<Self>, tasks: &Arc<TaskMap>) -> u64 {
        self.request(tasks, true)
    }

    /// Requests a write for progress only; coalesced to one per [`PROGRESS_SAVE_INTERVAL`].
    pub fn request_progress_save(self: &Arc<Self>, tasks: &Arc<TaskMap>) {
        self.request(tasks, false);
    }

    /// Writes the current state now and waits for that write. `Err` if it failed.
    pub async fn flush(self: &Arc<Self>, tasks: &Arc<TaskMap>) -> Result<(), String> {
        let ticket = self.request_save(tasks);
        let mut written = self.written.subscribe();
        let _ = written.wait_for(|done| *done >= ticket).await;
        match self.status.borrow().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn request(self: &Arc<Self>, tasks: &Arc<TaskMap>, urgent: bool) -> u64 {
        let mut pending = self.pending.lock().unwrap();
        pending.requested += 1;
        pending.urgent |= urgent;
        let ticket = pending.requested;
        if self.path.is_none() || self.read_only.load(Ordering::SeqCst) {
            // Nothing will be written; report the request as covered.
            pending.written = ticket;
            drop(pending);
            self.written.send_replace(ticket);
            return ticket;
        }
        if pending.running {
            drop(pending);
            if urgent {
                self.wake.notify_one();
            }
        } else {
            pending.running = true;
            drop(pending);
            tokio::spawn(Arc::clone(self).run_writer(Arc::clone(tasks)));
        }
        ticket
    }

    async fn run_writer(self: Arc<Self>, tasks: Arc<TaskMap>) {
        loop {
            let (ticket, wait_until) = {
                let mut pending = self.pending.lock().unwrap();
                if pending.written == pending.requested {
                    pending.running = false;
                    return;
                }
                let due = pending
                    .last_write
                    .map(|at| at + PROGRESS_SAVE_INTERVAL)
                    .filter(|due| !pending.urgent && *due > Instant::now());
                if due.is_none() {
                    pending.urgent = false;
                }
                (pending.requested, due)
            };
            if let Some(due) = wait_until {
                tokio::select! {
                    _ = tokio::time::sleep_until(due.into()) => {}
                    _ = self.wake.notified() => {}
                }
                continue;
            }

            // Snapshot after reading `ticket`: it includes every change requested so far.
            let mut snapshot: Vec<DownloadTaskState> =
                tasks.read().await.values().cloned().collect();
            snapshot.sort_by_key(|t| std::cmp::Reverse(t.created_at_ms));
            let result = match self.path.clone() {
                Some(path) => tokio::task::spawn_blocking(move || write_snapshot(&path, &snapshot))
                    .await
                    .unwrap_or_else(|e| Err(format!("history save task panicked: {e}"))),
                None => Ok(()),
            };
            {
                let mut pending = self.pending.lock().unwrap();
                pending.written = ticket;
                pending.last_write = Some(Instant::now());
            }
            match result {
                Ok(()) => {
                    self.status
                        .send_if_modified(|status| status.take().is_some());
                }
                Err(error) => {
                    warn!("Failed to save download history: {error}");
                    let message = format!("Download history could not be saved: {error}");
                    self.status.send_if_modified(|status| {
                        let changed = status.as_ref() != Some(&message);
                        *status = Some(message);
                        changed
                    });
                }
            }
            self.written.send_replace(ticket);
        }
    }
}

const NEWER_VERSION_STATUS: &str = "Download history was saved by a newer version of Conflux. \
It is left unchanged, and downloads from this session will not be remembered.";

fn write_snapshot(path: &Path, snapshot: &[DownloadTaskState]) -> Result<(), String> {
    let json = serde_json::to_vec(&OnDiskRef {
        schema_version: SCHEMA_VERSION,
        tasks: snapshot,
    })
    .map_err(|e| e.to_string())?;
    write_atomic(path, &json).map_err(|e| e.to_string())
}

struct Loaded {
    tasks: Vec<DownloadTaskState>,
    notice: Option<String>,
    newer_version: bool,
}

fn load_from(path: &Path) -> Loaded {
    let empty = |notice, newer_version| Loaded {
        tasks: Vec::new(),
        notice,
        newer_version,
    };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return empty(None, false),
        Err(e) => {
            warn!("Cannot read download history: {e}");
            return empty(None, false);
        }
    };
    let quarantine = |reason: &str| {
        warn!("Invalid download history, starting empty: {reason}");
        let notice = crate::fileutil::quarantine_with_notice(path, "download history");
        empty(Some(notice), false)
    };
    // The version is read first: a newer layout may not parse as ours at all, and must
    // then be preserved, not quarantined as corrupt.
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => return quarantine(&e.to_string()),
    };
    let version = match &value {
        serde_json::Value::Array(_) => 0,
        serde_json::Value::Object(map) => match map.get("schema_version").map(|v| v.as_u64()) {
            Some(Some(version)) => version,
            _ => return quarantine("missing or invalid schema_version"),
        },
        _ => return quarantine("not a history document"),
    };
    if version > u64::from(SCHEMA_VERSION) {
        warn!(
            version,
            supported = SCHEMA_VERSION,
            "Download history is from a newer version; leaving it untouched"
        );
        return empty(
            Some(format!(
                "Your download history was saved by a newer version of Conflux (format {version}). \
It was left unchanged; downloads from this session will not be remembered."
            )),
            true,
        );
    }
    let parsed = if version == 0 {
        serde_json::from_value::<Vec<DownloadTaskState>>(value)
    } else {
        serde_json::from_value::<Versioned>(value).map(|v| v.tasks)
    };
    let mut tasks = match parsed {
        Ok(tasks) => tasks,
        Err(e) => return quarantine(&e.to_string()),
    };
    for task in &mut tasks {
        task.recover_published();
        if matches!(task.status, TaskStatus::Downloading | TaskStatus::Stopping) {
            task.status = TaskStatus::Paused;
        }
        task.clear_runtime();
        // Backfill legacy completed tasks that didn't have duration_ms recorded
        if task.status == TaskStatus::Completed && task.duration_ms == 0 {
            if let Some(completed_ms) = task.completed_at_ms {
                if completed_ms >= task.created_at_ms {
                    let diff = completed_ms - task.created_at_ms;
                    task.duration_ms = diff;
                    if diff > 0 && task.downloaded_bytes > 0 && task.avg_speed_bytes_sec.is_none() {
                        task.avg_speed_bytes_sec =
                            Some((task.downloaded_bytes as f64) / (diff as f64 / 1000.0));
                    }
                }
            }
        }
    }
    tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at_ms));
    Loaded {
        tasks,
        notice: None,
        newer_version: false,
    }
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
            final_path: None,
            file_identity: None,
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
            duration_ms: 0,
            avg_speed_bytes_sec: None,
            adapters: vec![AdapterStat {
                adapter_id: None,
                name: "Default route".into(),
                ip: None,
                downloaded_bytes: 40,
                speed_bytes_sec: 12.0,
                active_connections: 2,
                dropped: false,
                last_error: None,
                drop_reason: None,
            }],
            chunk_map: Some("#>..".into()),
            lifecycle: Default::default(),
        }
    }

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("conflux-history-{}", uuid::Uuid::new_v4()))
    }

    fn map(tasks: Vec<DownloadTaskState>) -> Arc<TaskMap> {
        Arc::new(RwLock::new(
            tasks.into_iter().map(|t| (t.id.clone(), t)).collect(),
        ))
    }

    #[tokio::test]
    async fn save_then_load_pauses_running_tasks_and_clears_runtime_fields() {
        let dir = temp_dir();
        let store = Arc::new(HistoryStore::new(Some(dir.join("downloads.json"))));
        assert!(store.load().0.is_empty());

        let mut stopping = task("c", TaskStatus::Stopping, 0);
        stopping.downloaded_bytes = 7;
        let tasks = map(vec![
            task("a", TaskStatus::Downloading, 1),
            task("b", TaskStatus::Completed, 2),
            stopping,
        ]);
        store.flush(&tasks).await.unwrap();

        let (loaded, notice) = store.load();
        assert!(notice.is_none());
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].id, "b", "newest first");
        assert_eq!(
            loaded[2].status,
            TaskStatus::Paused,
            "a stopping task reloads paused"
        );
        let a = &loaded[1];
        assert_eq!(a.status, TaskStatus::Paused);
        assert_eq!(a.speed_bytes_sec, 0.0);
        assert_eq!(a.active_chunks, 0);
        assert_eq!(a.downloaded_bytes, 40);
        assert_eq!(a.adapters[0].active_connections, 0);
        assert_eq!(a.chunk_map.as_deref(), Some("#..."));
        assert_eq!(loaded[0].status, TaskStatus::Completed);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn saved_file_carries_schema_version() {
        let dir = temp_dir();
        let path = dir.join("downloads.json");
        let store = Arc::new(HistoryStore::new(Some(path.clone())));
        let tasks = map(vec![task("a", TaskStatus::Completed, 1)]);
        store.flush(&tasks).await.unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], SCHEMA_VERSION);
        assert_eq!(value["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(store.load().0.len(), 1, "versioned file round-trips");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_history_is_backed_up_not_lost() {
        let dir = std::env::temp_dir().join(format!("conflux-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("downloads.json");
        std::fs::write(&path, b"not json").unwrap();
        let store = HistoryStore::new(Some(path.clone()));
        let (tasks, notice) = store.load();
        assert!(tasks.is_empty());
        assert!(notice.unwrap().contains("downloads.json.corrupt-"));
        assert!(!path.exists());
        let backup = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap())
            .find(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .expect("backup exists");
        assert_eq!(std::fs::read(backup.path()).unwrap(), b"not json");
        // A second load finds nothing to complain about.
        let (tasks, notice) = store.load();
        assert!(tasks.is_empty() && notice.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn older_history_without_new_fields_loads() {
        let dir = std::env::temp_dir().join(format!("conflux-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("downloads.json");
        let mut value = serde_json::to_value(vec![task("a", TaskStatus::Error, 1)]).unwrap();
        for key in [
            "final_path",
            "completed_at_ms",
            "adapters",
            "chunk_map",
            "duration_ms",
            "avg_speed_bytes_sec",
        ] {
            value[0].as_object_mut().unwrap().remove(key);
        }
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let Loaded {
            tasks: loaded,
            notice,
            newer_version,
        } = load_from(&path);
        assert!(notice.is_none() && !newer_version);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].status, TaskStatus::Error);
        assert!(loaded[0].adapters.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn older_history_without_duration_backfills_for_completed_tasks() {
        let dir = std::env::temp_dir().join(format!("conflux-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("downloads.json");
        let mut t = task("a", TaskStatus::Completed, 1000);
        t.completed_at_ms = Some(5000);
        t.downloaded_bytes = 4000;
        let mut value = serde_json::to_value(vec![t]).unwrap();
        value[0].as_object_mut().unwrap().remove("duration_ms");
        value[0]
            .as_object_mut()
            .unwrap()
            .remove("avg_speed_bytes_sec");
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let Loaded {
            tasks: loaded,
            notice,
            ..
        } = load_from(&path);
        assert!(notice.is_none());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].duration_ms, 4000);
        assert_eq!(loaded[0].avg_speed_bytes_sec, Some(1000.0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn preserves_duration_and_avg_speed() {
        let dir = temp_dir();
        let path = dir.join("downloads.json");
        let store = Arc::new(HistoryStore::new(Some(path.clone())));
        let mut t = task("a", TaskStatus::Completed, 1000);
        t.duration_ms = 2500;
        t.avg_speed_bytes_sec = Some(1600.0);
        let tasks = map(vec![t]);
        store.flush(&tasks).await.unwrap();
        let (loaded, _) = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].duration_ms, 2500);
        assert_eq!(loaded[0].avg_speed_bytes_sec, Some(1600.0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn read_ids(path: &Path) -> Vec<String> {
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        value["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    }

    /// Many concurrent requests produce few writes, and the last write always holds the
    /// newest state: an older snapshot can never land after a newer one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_requests_coalesce_and_the_newest_state_wins() {
        let dir = temp_dir();
        let path = dir.join("downloads.json");
        let store = Arc::new(HistoryStore::new(Some(path.clone())));
        let tasks = map(vec![]);
        let mut joins = Vec::new();
        for i in 0..50u64 {
            let (store, tasks) = (Arc::clone(&store), Arc::clone(&tasks));
            joins.push(tokio::spawn(async move {
                let id = format!("t{i:02}");
                tasks
                    .write()
                    .await
                    .insert(id.clone(), task(&id, TaskStatus::Paused, i));
                store.request_save(&tasks);
            }));
        }
        for join in joins {
            join.await.unwrap();
        }
        store.flush(&tasks).await.unwrap();
        assert_eq!(read_ids(&path).len(), 50, "the final write has every task");
        let pending = store.pending.lock().unwrap();
        assert!(!pending.running, "the writer exits when idle");
        assert_eq!(pending.written, pending.requested);
        drop(pending);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn progress_saves_are_throttled_but_lifecycle_saves_are_not() {
        let dir = temp_dir();
        let path = dir.join("downloads.json");
        let store = Arc::new(HistoryStore::new(Some(path.clone())));
        let tasks = map(vec![task("a", TaskStatus::Downloading, 1)]);
        store.flush(&tasks).await.unwrap();

        // A progress tick right after a write waits out the interval ...
        tasks.write().await.get_mut("a").unwrap().downloaded_bytes = 41;
        store.request_progress_save(&tasks);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            *store.written.borrow(),
            1,
            "throttled progress write is pending"
        );

        // ... unless a lifecycle change arrives, which is written at once (with the tick).
        tasks.write().await.get_mut("a").unwrap().status = TaskStatus::Paused;
        let ticket = store.request_save(&tasks);
        let mut written = store.written.subscribe();
        tokio::time::timeout(
            Duration::from_millis(100),
            written.wait_for(|w| *w >= ticket),
        )
        .await
        .expect("lifecycle save is not throttled")
        .unwrap();
        let saved = HistoryStore::new(Some(path)).load().0;
        assert_eq!(saved[0].status, TaskStatus::Paused);
        assert_eq!(saved[0].downloaded_bytes, 41);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Disk errors (disk full, permissions, a directory in the way) are surfaced, not just
    /// logged, and cleared again once a write succeeds.
    #[tokio::test]
    async fn write_failures_are_published_until_a_write_succeeds() {
        let dir = temp_dir();
        let path = dir.join("downloads.json");
        // A directory at the target path makes every atomic rename fail.
        std::fs::create_dir_all(&path).unwrap();
        let store = Arc::new(HistoryStore::new(Some(path.clone())));
        let status = store.status();
        let tasks = map(vec![task("a", TaskStatus::Paused, 1)]);

        let error = store.flush(&tasks).await.unwrap_err();
        assert!(
            error.starts_with("Download history could not be saved"),
            "{error}"
        );
        assert_eq!(status.borrow().as_deref(), Some(error.as_str()));

        std::fs::remove_dir(&path).unwrap();
        store.flush(&tasks).await.unwrap();
        assert!(status.borrow().is_none(), "recovery clears the failure");
        assert_eq!(read_ids(&path), ["a"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn legacy_array_is_upgraded_to_the_current_layout() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("downloads.json");
        let legacy = serde_json::to_vec(&vec![task("a", TaskStatus::Paused, 1)]).unwrap();
        std::fs::write(&path, legacy).unwrap();
        let store = Arc::new(HistoryStore::new(Some(path.clone())));
        let (loaded, notice) = store.load();
        assert!(notice.is_none());
        let tasks = map(loaded);
        store.flush(&tasks).await.unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], SCHEMA_VERSION);
        assert_eq!(read_ids(&path), ["a"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Downgrade: a file written by a newer version is neither quarantined nor overwritten,
    /// even if its layout no longer parses as ours.
    #[tokio::test]
    async fn newer_history_is_preserved_and_never_written() {
        for contents in [
            format!(r#"{{"schema_version":{},"tasks":[]}}"#, SCHEMA_VERSION + 1),
            format!(
                r#"{{"schema_version":{},"entries":{{"x":1}}}}"#,
                SCHEMA_VERSION + 1
            ),
        ] {
            let dir = temp_dir();
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("downloads.json");
            std::fs::write(&path, &contents).unwrap();
            let store = Arc::new(HistoryStore::new(Some(path.clone())));
            let (loaded, notice) = store.load();
            assert!(loaded.is_empty());
            assert!(notice.unwrap().contains("newer version"));
            assert!(
                store.status().borrow().is_some(),
                "read-only state is visible"
            );

            let tasks = map(vec![task("a", TaskStatus::Completed, 1)]);
            assert!(store.flush(&tasks).await.is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
            let backups = std::fs::read_dir(&dir).unwrap().count();
            assert_eq!(backups, 1, "no corrupt backup was made");
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[test]
    fn current_version_with_invalid_tasks_is_quarantined() {
        for contents in [
            r#"{"tasks":[]}"#.to_string(),
            format!(r#"{{"schema_version":{SCHEMA_VERSION},"tasks":[{{"id":1}}]}}"#),
            "42".to_string(),
        ] {
            let dir = temp_dir();
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("downloads.json");
            std::fs::write(&path, &contents).unwrap();
            let loaded = load_from(&path);
            assert!(loaded.tasks.is_empty() && !loaded.newer_version);
            assert!(loaded.notice.unwrap().contains(".corrupt-"), "{contents}");
            assert!(!path.exists());
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}
