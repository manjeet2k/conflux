//! Owns the lifecycle of every download task: transitions, running engine handles, output
//! path ownership and cancellation. Tauri commands only call into this.
//!
//! Invariants:
//! - Every transition (start, resume commit, stop request, engine exit, removal) happens
//!   under the task lock and bumps the task's [`Lifecycle::generation`]. An operation that
//!   awaits something in between (a resume's probe) holds a ticket with the generation it
//!   started from and commits only if that is still current, so a stale resume can never
//!   restart a task that was paused, removed, resumed or finished in the meantime.
//! - A removal first marks the task `removing` (a tombstone) so nothing new can start, then
//!   stops it, and only removes it once its engine is gone.
//! - An output path stays reserved until the engine has actually returned. A stop that times
//!   out never aborts the engine (an aborted future cannot cancel a blocking disk write that
//!   is already running); the task shows `Stopping` and keeps its file until it exits.
//!
//! Lock order: `handles` -> `tasks` -> `reserved_paths`.

use crate::history::{HistoryStore, TaskMap};
use crate::state::{DownloadTaskState, TaskStatus};
use conflux_core::{remove_resume_sidecar, AdapterUpdate, FileIdentity};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, watch, Mutex, RwLock, RwLockReadGuard};
use tracing::{info, warn};

/// How long a stop request waits for the engine before reporting the task as `Stopping`.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(10);

pub const STILL_STOPPING: &str = "Download is still stopping; try again in a moment";

/// Adapter IP -> (adapter id, display name), shared with the progress forwarder.
pub type AdapterNames = Arc<RwLock<HashMap<std::net::IpAddr, (String, String)>>>;

/// Side effects of task changes (UI events, tray, notifications), so the controller can be
/// tested without a Tauri app.
pub trait TaskEvents: Send + Sync + 'static {
    /// `task` changed; `tasks` is the whole table after the change.
    fn task_changed(&self, task: &DownloadTaskState, tasks: &HashMap<String, DownloadTaskState>);
    /// A task was removed; `tasks` is the table after the removal.
    fn task_removed(&self, tasks: &HashMap<String, DownloadTaskState>);
    /// A task's engine exited and its final status was recorded.
    fn worker_finished(&self, task: &DownloadTaskState);
}

/// [`TaskEvents`] that does nothing.
#[cfg(test)]
pub struct NoEvents;
#[cfg(test)]
impl TaskEvents for NoEvents {
    fn task_changed(&self, _: &DownloadTaskState, _: &HashMap<String, DownloadTaskState>) {}
    fn task_removed(&self, _: &HashMap<String, DownloadTaskState>) {}
    fn worker_finished(&self, _: &DownloadTaskState) {}
}

/// A running engine. Removed (by the engine's own exit) only after its final status is
/// recorded; its presence means the task still owns its output path.
struct TaskHandle {
    cancel_tx: watch::Sender<bool>,
    adapter_tx: Option<mpsc::UnboundedSender<Vec<AdapterUpdate>>>,
    names: AdapterNames,
    /// Becomes `true` once the engine exited and the handle and path were released.
    done: watch::Receiver<bool>,
}

/// What the caller hands over when launching an engine.
pub struct WorkerChannels {
    pub cancel_tx: watch::Sender<bool>,
    pub adapter_tx: Option<mpsc::UnboundedSender<Vec<AdapterUpdate>>>,
    pub names: AdapterNames,
}

/// How an engine run ended, applied to the task by [`TaskController`].
#[derive(Debug, Clone)]
pub struct WorkerExit {
    pub status: TaskStatus,
    pub sha256: Option<String>,
    pub error: Option<String>,
    /// File length on disk, for completed downloads.
    pub completed_length: Option<u64>,
    /// Cumulative active duration; `None` keeps the recorded value.
    pub duration_ms: Option<u64>,
    /// Physical completed path after the .part file was successfully published.
    pub published_path: Option<PathBuf>,
}

impl WorkerExit {
    pub fn paused() -> Self {
        Self {
            status: TaskStatus::Paused,
            sha256: None,
            error: None,
            completed_length: None,
            duration_ms: None,
            published_path: None,
        }
    }

    fn crashed() -> Self {
        Self {
            status: TaskStatus::Error,
            error: Some("The download stopped unexpectedly (internal error)".into()),
            ..Self::paused()
        }
    }

    fn apply(self, task: &mut DownloadTaskState) {
        task.status = self.status;
        task.clear_runtime();
        if let Some(ms) = self.duration_ms {
            task.duration_ms = ms;
        }
        if self.status == TaskStatus::Completed {
            if let Some(path) = self.published_path {
                task.filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into();
                task.save_path = path.to_string_lossy().into();
                task.final_path = None;
            }
            task.total_bytes = self
                .completed_length
                .unwrap_or_else(|| task.total_bytes.max(task.downloaded_bytes));
            task.downloaded_bytes = task.total_bytes;
            task.completed_chunks = task.total_chunks;
            task.completed_at_ms = Some(now_ms());
        }
        if task.duration_ms > 0 && task.downloaded_bytes > 0 {
            task.avg_speed_bytes_sec =
                Some((task.downloaded_bytes as f64) / (task.duration_ms as f64 / 1000.0));
        }
        // A panic while publishing must not erase the persisted completion receipt:
        // the rename may already have succeeded before the worker unwound.
        if self.sha256.is_some() || task.final_path.is_none() {
            task.sha256 = self.sha256;
        }
        task.error = self.error;
    }
}

/// A resume that passed its checks; valid only while the task's generation is unchanged.
pub struct ResumeTicket {
    pub task: DownloadTaskState,
    generation: u64,
}

pub enum ResumeStart {
    Ready(ResumeTicket),
    /// Already downloading: nothing to do.
    AlreadyRunning(DownloadTaskState),
}

#[derive(Debug)]
pub enum CommitError {
    /// The task was removed meanwhile.
    Removed,
    /// Another transition happened meanwhile; this is the task's current state.
    Superseded(Box<DownloadTaskState>),
}

pub struct TaskController {
    tasks: Arc<TaskMap>,
    handles: Mutex<HashMap<String, TaskHandle>>,
    /// Output paths owned by tasks that are starting or running, so two downloads never
    /// resolve to the same file before either has created it on disk.
    reserved_paths: Mutex<HashSet<PathBuf>>,
    history: Arc<HistoryStore>,
    events: Arc<dyn TaskEvents>,
    stop_timeout: Duration,
}

impl TaskController {
    pub fn new(
        tasks: Vec<DownloadTaskState>,
        history: Arc<HistoryStore>,
        events: Arc<dyn TaskEvents>,
    ) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(
                tasks.into_iter().map(|t| (t.id.clone(), t)).collect(),
            )),
            handles: Mutex::new(HashMap::new()),
            reserved_paths: Mutex::new(HashSet::new()),
            history,
            events,
            stop_timeout: STOP_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_stop_timeout(mut self, timeout: Duration) -> Self {
        self.stop_timeout = timeout;
        self
    }

    // ── Reading ─────────────────────────────────────────────
    pub async fn read(&self) -> RwLockReadGuard<'_, HashMap<String, DownloadTaskState>> {
        self.tasks.read().await
    }

    /// Non-blocking read, for synchronous callers (startup).
    pub fn try_read(
        &self,
    ) -> Result<RwLockReadGuard<'_, HashMap<String, DownloadTaskState>>, tokio::sync::TryLockError>
    {
        self.tasks.try_read()
    }

    pub async fn get(&self, id: &str) -> Option<DownloadTaskState> {
        self.tasks.read().await.get(id).cloned()
    }

    /// All tasks, newest first.
    pub async fn list(&self) -> Vec<DownloadTaskState> {
        let mut tasks: Vec<_> = self.tasks.read().await.values().cloned().collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at_ms));
        tasks
    }

    pub async fn ids_with_status(&self, statuses: &[TaskStatus]) -> Vec<String> {
        self.tasks
            .read()
            .await
            .values()
            .filter(|t| statuses.contains(&t.status))
            .map(|t| t.id.clone())
            .collect()
    }

    /// Adapter channels of the running downloads, for hot-plugging adapters.
    pub async fn adapter_targets(
        &self,
    ) -> Vec<(
        String,
        Option<mpsc::UnboundedSender<Vec<AdapterUpdate>>>,
        AdapterNames,
    )> {
        let handles = self.handles.lock().await;
        let tasks = self.tasks.read().await;
        handles
            .iter()
            .filter(|(id, _)| {
                tasks
                    .get(*id)
                    .is_some_and(|t| t.status == TaskStatus::Downloading)
            })
            .map(|(id, h)| (id.clone(), h.adapter_tx.clone(), h.names.clone()))
            .collect()
    }

    // ── History ─────────────────────────────────────────────
    pub fn save_history(&self) {
        self.history.request_save(&self.tasks);
    }

    pub fn save_progress(&self) {
        self.history.request_progress_save(&self.tasks);
    }

    pub async fn flush_history(&self) -> Result<(), String> {
        self.history.flush(&self.tasks).await
    }

    pub fn history_status(&self) -> watch::Receiver<Option<String>> {
        self.history.status()
    }

    // ── Output paths ────────────────────────────────────────
    /// Picks a free name in `dir` (skipping every path a task knows), claims it on disk with
    /// an empty placeholder and reserves it. See [`reserve_output_path`].
    #[cfg(test)]
    pub async fn reserve_new_output(
        &self,
        dir: &Path,
        filename: &str,
    ) -> Result<(PathBuf, Option<FileIdentity>), String> {
        let claimed: Vec<PathBuf> = self
            .tasks
            .read()
            .await
            .values()
            .map(|t| PathBuf::from(&t.save_path))
            .collect();
        reserve_output_path(&self.reserved_paths, &claimed, dir, filename).await
    }

    /// Claim only the .part file on disk; keep the desired final name in history.
    pub async fn reserve_staged_output(
        &self,
        dir: &Path,
        filename: &str,
    ) -> Result<(PathBuf, PathBuf, FileIdentity), String> {
        let tasks = self.tasks.read().await;
        let mut reserved = self.reserved_paths.lock().await;
        for n in 0..u32::MAX {
            let name = if n == 0 {
                filename.to_string()
            } else {
                numbered_filename(filename, n)
            };
            let final_path = dir.join(&name);
            let part_path = dir.join(format!("{name}.part"));
            if [&final_path, &part_path].iter().any(|p| {
                reserved.contains(*p)
                    || tasks.values().any(|t| {
                        Path::new(&t.save_path) == *p
                            || t.final_path.as_deref().is_some_and(|f| Path::new(f) == *p)
                    })
                    || std::fs::symlink_metadata(p).is_ok()
                    || conflux_core::resume_sidecar_path(p).exists()
            }) {
                continue;
            }
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&part_path)
            {
                Ok(file) => {
                    let identity = FileIdentity::of(&file).map_err(|e| e.to_string())?;
                    reserved.insert(part_path.clone());
                    return Ok((part_path, final_path, identity));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Cannot create {}: {e}", part_path.display())),
            }
        }
        Err("No available download filename".into())
    }

    /// Persist both the completed hash and chosen destination before renaming. This
    /// makes an interrupted publish recoverable using file identity after app restart.
    pub async fn prepare_publish(&self, id: &str, hash: &str) -> Result<Option<PathBuf>, String> {
        {
            let mut tasks = self.tasks.write().await;
            let task = tasks.get(id).ok_or("Download no longer exists")?;
            let Some(desired) = &task.final_path else {
                return Ok(None);
            };
            let desired = PathBuf::from(desired);
            let dir = desired.parent().ok_or("Invalid final download path")?;
            let mut reserved = self.reserved_paths.lock().await;
            let mut selected = None;
            for n in 0..u32::MAX {
                let candidate = if n == 0 {
                    desired.clone()
                } else {
                    dir.join(numbered_filename(&task.filename, n))
                };
                if reserved.contains(&candidate)
                    || tasks.values().any(|t| {
                        t.id != id
                            && (Path::new(&t.save_path) == candidate
                                || t.final_path
                                    .as_deref()
                                    .is_some_and(|f| Path::new(f) == candidate))
                    })
                    || std::fs::symlink_metadata(&candidate).is_ok()
                    || conflux_core::resume_sidecar_path(&candidate).exists()
                {
                    continue;
                }
                selected = Some(candidate);
                break;
            }
            let path = selected.ok_or("No available completed filename")?;
            reserved.insert(path.clone());
            let task = tasks.get_mut(id).unwrap();
            task.final_path = Some(path.to_string_lossy().into());
            task.sha256 = Some(hash.into());
        }
        self.save_history();
        self.flush_history().await?;
        Ok(self
            .get(id)
            .await
            .and_then(|t| t.final_path.map(PathBuf::from)))
    }

    pub async fn publish_completed(&self, id: &str, hash: &str) -> Result<Option<PathBuf>, String> {
        let task = self.get(id).await.ok_or("Download no longer exists")?;
        if task.final_path.is_none() {
            return Ok(None);
        }
        let identity = task
            .file_identity
            .ok_or("Cannot publish a file with unknown ownership")?;
        loop {
            let target = self
                .prepare_publish(id, hash)
                .await?
                .ok_or("Missing completed path")?;
            match conflux_core::publish_owned_file(Path::new(&task.save_path), &target, &identity)
                .await
            {
                Ok(()) => return Ok(Some(target)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // A different process claimed the final name after it was selected.
                    self.release_path(&target).await;
                }
                Err(error) => {
                    return Err(format!(
                        "The completed .part file was kept; could not publish {}: {error}",
                        target.display()
                    ))
                }
            }
        }
    }

    /// Reserves an existing task's path; `false` if another task owns it.
    pub async fn reserve_path(&self, path: &Path) -> bool {
        self.reserved_paths.lock().await.insert(path.to_path_buf())
    }

    pub async fn release_path(&self, path: &Path) {
        self.reserved_paths.lock().await.remove(path);
    }

    // ── Updates ─────────────────────────────────────────────
    fn emit(&self, task: &DownloadTaskState, tasks: &HashMap<String, DownloadTaskState>) {
        self.events.task_changed(task, tasks);
    }

    /// Applies a progress update to a `Downloading` task (other statuses are final or
    /// winding down and must not be overwritten by late ticks). Returns whether the task
    /// still exists.
    pub async fn update_running(&self, id: &str, f: impl FnOnce(&mut DownloadTaskState)) -> bool {
        let mut tasks = self.tasks.write().await;
        let Some(task) = tasks.get_mut(id) else {
            return false;
        };
        if task.status == TaskStatus::Downloading {
            f(task);
            let snapshot = task.clone();
            self.emit(&snapshot, &tasks);
        }
        true
    }

    // ── Start ───────────────────────────────────────────────
    /// Adds a new `Downloading` task and returns its generation for [`Self::spawn_worker`].
    pub async fn insert_new(&self, mut task: DownloadTaskState) -> u64 {
        let mut tasks = self.tasks.write().await;
        task.status = TaskStatus::Downloading;
        task.lifecycle.generation = 1;
        task.lifecycle.removing = false;
        let id = task.id.clone();
        tasks.insert(id.clone(), task);
        let snapshot = tasks[&id].clone();
        self.emit(&snapshot, &tasks);
        snapshot.lifecycle.generation
    }

    // ── Resume ──────────────────────────────────────────────
    /// First half of a resume: checks the task can be resumed and returns a ticket. The
    /// caller then probes etc. and finishes with [`Self::commit_resume`].
    pub async fn begin_resume(&self, id: &str) -> Result<ResumeStart, String> {
        loop {
            // A stop that is still winding down (requested, engine not exited yet) is waited
            // out, so "pause, then resume" resumes instead of reporting "already running".
            let pending_stop = {
                let handles = self.handles.lock().await;
                let tasks = self.tasks.read().await;
                let task = tasks.get(id).ok_or_else(|| format!("Unknown task: {id}"))?;
                if task.lifecycle.removing {
                    return Err("Download is being removed".into());
                }
                match (task.status, handles.get(id)) {
                    (TaskStatus::Downloading, Some(handle)) if *handle.cancel_tx.borrow() => {
                        handle.done.clone()
                    }
                    (TaskStatus::Paused | TaskStatus::Error, _) => {
                        return Ok(ResumeStart::Ready(ResumeTicket {
                            task: task.clone(),
                            generation: task.lifecycle.generation,
                        }))
                    }
                    (TaskStatus::Downloading, _) => {
                        return Ok(ResumeStart::AlreadyRunning(task.clone()))
                    }
                    (TaskStatus::Stopping, _) => return Err(STILL_STOPPING.into()),
                    (TaskStatus::Completed, _) => return Err("Download is already complete".into()),
                }
            };
            let mut done = pending_stop;
            let timed_out = tokio::time::timeout(self.stop_timeout, done.wait_for(|d| *d))
                .await
                .is_err();
            if timed_out {
                return Err(STILL_STOPPING.into());
            }
        }
    }

    /// Second half of a resume: marks the task `Downloading` (with `f` applied) only if no
    /// other transition happened since [`Self::begin_resume`]. Returns the snapshot and the
    /// generation to pass to [`Self::spawn_worker`].
    pub async fn commit_resume(
        &self,
        ticket: ResumeTicket,
        f: impl FnOnce(&mut DownloadTaskState),
    ) -> Result<(DownloadTaskState, u64), CommitError> {
        let mut tasks = self.tasks.write().await;
        let task = tasks.get_mut(&ticket.task.id).ok_or(CommitError::Removed)?;
        if task.lifecycle.removing || task.lifecycle.generation != ticket.generation {
            return Err(CommitError::Superseded(Box::new(task.clone())));
        }
        f(task);
        task.status = TaskStatus::Downloading;
        task.clear_runtime();
        task.lifecycle.generation += 1;
        let snapshot = task.clone();
        self.emit(&snapshot, &tasks);
        Ok((snapshot.clone(), snapshot.lifecycle.generation))
    }

    // ── Engine ──────────────────────────────────────────────
    /// Runs `worker` (the engine) for task `id` if the task is still in the `generation` the
    /// caller committed (nobody paused or removed it meanwhile). `output_path` must already
    /// be reserved; it stays reserved until `worker` has returned, and is released here if
    /// the task stands down. `placeholder`: identity of a fresh empty file this start
    /// created, deleted if the task was removed before it could start.
    ///
    /// The caller should hold the `last_adapters` read lock across this call so an adapter
    /// change cannot slip between choosing the adapters and registering the handle.
    pub async fn spawn_worker<F>(
        self: &Arc<Self>,
        id: &str,
        generation: u64,
        output_path: PathBuf,
        placeholder: Option<FileIdentity>,
        channels: WorkerChannels,
        worker: F,
    ) -> bool
    where
        F: Future<Output = WorkerExit> + Send + 'static,
    {
        let mut handles = self.handles.lock().await;
        let (wanted, removed) = {
            let tasks = self.tasks.read().await;
            match tasks.get(id) {
                Some(t) => (
                    t.lifecycle.generation == generation
                        && t.status == TaskStatus::Downloading
                        && !t.lifecycle.removing,
                    false,
                ),
                None => (false, true),
            }
        };
        if !wanted {
            drop(handles);
            info!(task_id = %id, "Task was paused or removed before it started; not starting the engine");
            // `remove` skipped the file because this start still reserved it.
            if removed {
                remove_empty_placeholder(&output_path, placeholder).await;
            }
            self.release_path(&output_path).await;
            return false;
        }
        let (done_tx, done_rx) = watch::channel(false);
        let controller = Arc::clone(self);
        let task_id = id.to_string();
        let path = output_path;
        tokio::spawn(async move {
            // A nested task, so a panic in the engine still records a final status and
            // releases the path instead of leaving the task `Downloading` forever.
            let exit = tokio::spawn(worker).await.unwrap_or_else(|panic| {
                warn!(task_id = %task_id, "Download task panicked: {panic}");
                WorkerExit::crashed()
            });
            controller
                .finish_worker(&task_id, &path, exit, done_tx)
                .await;
        });
        handles.insert(
            id.to_string(),
            TaskHandle {
                cancel_tx: channels.cancel_tx,
                adapter_tx: channels.adapter_tx,
                names: channels.names,
                done: done_rx,
            },
        );
        true
    }

    /// Records the engine's exit: final status, handle removed and path released, all at once.
    async fn finish_worker(
        &self,
        id: &str,
        output_path: &Path,
        exit: WorkerExit,
        done: watch::Sender<bool>,
    ) {
        let final_task = {
            let mut handles = self.handles.lock().await;
            let mut tasks = self.tasks.write().await;
            let snapshot = tasks.get_mut(id).map(|task| {
                // The publish destination was reserved until the worker really exited.
                // Capture it before apply() replaces the task's physical path.
                let publish_path = task.final_path.clone();
                exit.apply(task);
                task.lifecycle.generation += 1;
                (task.clone(), publish_path)
            });
            if let Some((task, _)) = &snapshot {
                self.emit(task, &tasks);
            }
            handles.remove(id);
            let mut reserved = self.reserved_paths.lock().await;
            reserved.remove(output_path);
            if let Some((_, Some(path))) = &snapshot {
                reserved.remove(Path::new(path));
            }
            snapshot.map(|(task, _)| task)
        };
        done.send_replace(true);
        self.save_history();
        if let Some(task) = final_task {
            self.events.worker_finished(&task);
        }
    }

    // ── Stop ────────────────────────────────────────────────
    /// Stops the task and returns its resulting snapshot (`None` for an unknown id). Waits
    /// up to the stop timeout for the engine; if it is still running after that the task is
    /// shown as `Stopping` and keeps its file until the engine really exits.
    ///
    /// Always counts as a transition, so a resume that is still probing will stand down.
    pub async fn stop(&self, id: &str) -> Option<DownloadTaskState> {
        let done = {
            let handles = self.handles.lock().await;
            let mut tasks = self.tasks.write().await;
            let task = tasks.get_mut(id)?;
            task.lifecycle.generation += 1;
            match handles.get(id) {
                Some(handle) => {
                    let _ = handle.cancel_tx.send(true);
                    Some(handle.done.clone())
                }
                None => {
                    // Committed but not spawned yet: `spawn_worker` will see the new
                    // generation and stand down.
                    if task.status == TaskStatus::Downloading {
                        task.status = TaskStatus::Paused;
                        task.clear_runtime();
                        let snapshot = task.clone();
                        self.emit(&snapshot, &tasks);
                    }
                    None
                }
            }
        };
        if let Some(mut done) = done {
            let timed_out = tokio::time::timeout(self.stop_timeout, done.wait_for(|d| *d))
                .await
                .is_err();
            if timed_out {
                warn!(
                    task_id = id,
                    "Download did not stop within {:?}; it keeps its file until it exits",
                    self.stop_timeout
                );
                self.mark_stopping(id).await;
            }
        }
        self.save_history();
        self.get(id).await
    }

    async fn mark_stopping(&self, id: &str) {
        let handles = self.handles.lock().await;
        let mut tasks = self.tasks.write().await;
        let Some(task) = tasks.get_mut(id) else {
            return;
        };
        if handles.contains_key(id) && task.status == TaskStatus::Downloading {
            task.status = TaskStatus::Stopping;
            task.clear_runtime();
            task.lifecycle.generation += 1;
            let snapshot = task.clone();
            self.emit(&snapshot, &tasks);
        }
    }

    /// Stops every downloading task concurrently (bounded by one stop timeout in total),
    /// then writes history and waits for that write.
    pub async fn stop_all(self: &Arc<Self>) {
        let ids = self.ids_with_status(&[TaskStatus::Downloading]).await;
        let mut stops = tokio::task::JoinSet::new();
        for id in ids {
            let controller = Arc::clone(self);
            stops.spawn(async move {
                controller.stop(&id).await;
            });
        }
        while let Some(result) = stops.join_next().await {
            if let Err(e) = result {
                warn!("Stopping a task failed: {e}");
            }
        }
        if let Err(e) = self.flush_history().await {
            warn!("Final history save failed: {e}");
        }
    }

    // ── Remove ──────────────────────────────────────────────
    /// Stops the task and removes it from the list. Its partial file and resume data are
    /// deleted (only after verifying file identity) unless it completed; a completed file
    /// only if `delete_file`, with the same check. Refuses while the engine is still running.
    pub async fn remove(&self, id: &str, delete_file: bool) -> Result<(), String> {
        {
            let mut tasks = self.tasks.write().await;
            let Some(task) = tasks.get_mut(id) else {
                return Ok(());
            };
            if task.lifecycle.removing {
                return Ok(()); // Another removal is in progress.
            }
            task.lifecycle.removing = true;
            task.lifecycle.generation += 1;
        }
        self.stop(id).await;

        let (task, owned_elsewhere) = {
            let handles = self.handles.lock().await;
            let mut tasks = self.tasks.write().await;
            if handles.contains_key(id) {
                if let Some(task) = tasks.get_mut(id) {
                    task.lifecycle.removing = false;
                }
                return Err(format!("{STILL_STOPPING}; it was not removed"));
            }
            let Some(task) = tasks.remove(id) else {
                return Ok(());
            };
            self.events.task_removed(&tasks);
            // Never delete a file another task now owns (e.g. this task stopped before the
            // file was created on disk, and a later task was assigned the same path).
            let path = PathBuf::from(&task.save_path);
            let reserved = self.reserved_paths.lock().await.contains(&path);
            let claimed = tasks.values().any(|t| t.save_path == task.save_path);
            (task, reserved || claimed)
        };
        self.save_history();

        if task.status == TaskStatus::Completed && !delete_file {
            return Ok(());
        }
        if owned_elsewhere {
            info!(task_id = %id, path = %task.save_path, "File is owned by another task; not deleting");
            return Ok(());
        }
        let Some(identity) = task.file_identity else {
            info!(task_id = %id, "File ownership is unknown; preserving output and resume data");
            return Ok(());
        };
        let path = PathBuf::from(&task.save_path);
        let delete_path = path.clone();
        match tokio::task::spawn_blocking(move || {
            conflux_core::remove_owned_file(&delete_path, &identity)
        })
        .await
        {
            Ok(Ok(true)) => {
                if let Err(error) = remove_resume_sidecar(&path) {
                    warn!(task_id = %id, "Failed to delete resume data: {error}");
                }
                info!(task_id = %id, "Deleted the owned file");
            }
            Ok(Ok(false)) => {
                info!(task_id = %id, "File identity changed or file is missing; preserving resume data")
            }
            result => {
                warn!(task_id = %id, "Could not safely delete owned file: {result:?}");
                return Err(
                    "Removed from the list, but the file could not safely be deleted".into(),
                );
            }
        }
        Ok(())
    }
}

/// Picks a free name in `dir` (`name`, `name (1)`, ...), claims it on disk by creating
/// an empty placeholder file, and reserves it for this process.
///
/// The claim uses `create_new`, so it is atomic across processes (e.g. the desktop app and
/// the CLI can never pick the same file). This mirrors `conflux_core::claim_unique_path`,
/// but additionally skips paths reserved by running tasks: a resumed task's file may be
/// missing on disk while its engine is about to recreate it.
#[cfg(test)]
pub async fn reserve_output_path(
    reserved: &Mutex<HashSet<PathBuf>>,
    claimed_paths: &[PathBuf],
    dir: &Path,
    filename: &str,
) -> Result<(PathBuf, Option<FileIdentity>), String> {
    let mut reserved = reserved.lock().await;
    let mut n: u32 = 0;
    loop {
        let candidate = if n == 0 {
            dir.join(filename)
        } else {
            dir.join(numbered_filename(filename, n))
        };
        n += 1;
        if reserved.contains(&candidate) || claimed_paths.iter().any(|p| p == &candidate) {
            continue;
        }
        let sidecar = conflux_core::resume_sidecar_path(&candidate);
        if sidecar.exists() {
            continue;
        }
        let created = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate);
        match created {
            Ok(file) => {
                let identity = FileIdentity::of(&file).ok();
                reserved.insert(candidate.clone());
                return Ok((candidate, identity));
            }
            // Taken on disk (possibly a moment ago by another process): try the next name.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!("Cannot create {}: {e}", candidate.display()));
            }
        }
    }
}

/// Deletes only our unchanged empty placeholder, using an exclusive identity-checked handle.
pub async fn remove_empty_placeholder(path: &Path, identity: Option<FileIdentity>) {
    let Some(identity) = identity else { return };
    let path = path.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        conflux_core::remove_owned_empty_file(&path, &identity)
    })
    .await;
}

/// "name.ext" -> "name (n).ext"; "name" -> "name (n)"; ".hidden" -> ".hidden (n)".
pub fn numbered_filename(filename: &str, n: u32) -> String {
    match filename.rfind('.') {
        Some(idx) if idx > 0 => format!("{} ({}){}", &filename[..idx], n, &filename[idx..]),
        _ => format!("{} ({})", filename, n),
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Lifecycle;
    use tokio::sync::oneshot;

    /// Records every status change, so tests can wait for and assert on transitions.
    #[derive(Default)]
    struct Recorder {
        changes: std::sync::Mutex<Vec<(String, TaskStatus)>>,
        finished: std::sync::Mutex<Vec<(String, TaskStatus)>>,
    }

    impl TaskEvents for Recorder {
        fn task_changed(&self, task: &DownloadTaskState, _: &HashMap<String, DownloadTaskState>) {
            self.changes
                .lock()
                .unwrap()
                .push((task.id.clone(), task.status));
        }
        fn task_removed(&self, _: &HashMap<String, DownloadTaskState>) {}
        fn worker_finished(&self, task: &DownloadTaskState) {
            self.finished
                .lock()
                .unwrap()
                .push((task.id.clone(), task.status));
        }
    }

    struct Fixture {
        controller: Arc<TaskController>,
        events: Arc<Recorder>,
        dir: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn fixture(tasks: Vec<DownloadTaskState>) -> Fixture {
        let events = Arc::new(Recorder::default());
        let controller = TaskController::new(
            tasks,
            Arc::new(HistoryStore::new(None)),
            events.clone() as Arc<dyn TaskEvents>,
        )
        .with_stop_timeout(Duration::from_millis(100));
        let dir = std::env::temp_dir().join(format!("conflux-controller-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Fixture {
            controller: Arc::new(controller),
            events,
            dir,
        }
    }

    fn task(id: &str, status: TaskStatus, path: &Path) -> DownloadTaskState {
        DownloadTaskState {
            final_path: None,
            id: id.into(),
            url: "http://example.invalid/file.bin".into(),
            filename: "file.bin".into(),
            save_dir: path.parent().unwrap().to_string_lossy().into(),
            save_path: path.to_string_lossy().into(),
            file_identity: None,
            adapter_ids: vec![],
            total_bytes: 9,
            supports_ranges: true,
            downloaded_bytes: 0,
            status,
            speed_bytes_sec: 0.0,
            eta_seconds: 0,
            active_chunks: 0,
            completed_chunks: 0,
            total_chunks: 0,
            sha256: None,
            error: None,
            created_at_ms: 1,
            completed_at_ms: None,
            duration_ms: 0,
            avg_speed_bytes_sec: None,
            adapters: vec![],
            chunk_map: None,
            lifecycle: Lifecycle::default(),
        }
    }

    fn worker_channels() -> (WorkerChannels, watch::Receiver<bool>) {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        (
            WorkerChannels {
                cancel_tx,
                adapter_tx: None,
                names: Arc::default(),
            },
            cancel_rx,
        )
    }

    #[tokio::test]
    async fn staging_claims_part_names_and_skips_files_sidecars_and_task_claims() {
        let f = fixture(vec![]);
        let c = &f.controller;
        std::fs::write(f.dir.join("file.bin"), b"existing").unwrap();
        std::fs::write(f.dir.join("file (1).bin.part.conflux.json"), b"resume").unwrap();
        let (part, final_path, identity) =
            c.reserve_staged_output(&f.dir, "file.bin").await.unwrap();
        assert_eq!(part, f.dir.join("file (2).bin.part"));
        assert_eq!(final_path, f.dir.join("file (2).bin"));
        assert_eq!(FileIdentity::at(&part).unwrap(), identity);
        assert!(!final_path.exists());
        let mut paused = task("a", TaskStatus::Paused, &part);
        paused.final_path = Some(final_path.to_string_lossy().into());
        c.insert_new(paused).await;
        // Even after a user deletes a paused task's partial file, its intended name
        // must not be assigned to a different download.
        std::fs::remove_file(&part).unwrap();
        c.release_path(&part).await;
        let (_, other_final, _) = c.reserve_staged_output(&f.dir, "file.bin").await.unwrap();
        assert_eq!(other_final, f.dir.join("file (3).bin"));
        assert_eq!(std::fs::read(f.dir.join("file.bin")).unwrap(), b"existing");
    }

    #[tokio::test]
    async fn publish_collision_records_the_actual_final_path_and_releases_both_paths() {
        let f = fixture(vec![]);
        let c = &f.controller;
        let (part, desired, identity) = c.reserve_staged_output(&f.dir, "file.bin").await.unwrap();
        std::fs::write(&part, b"complete").unwrap();
        let hash = conflux_core::compute_sha256(&part).await.unwrap();
        let mut t = task("a", TaskStatus::Downloading, &part);
        t.file_identity = Some(identity.clone());
        t.final_path = Some(desired.to_string_lossy().into());
        let generation = c.insert_new(t).await;
        // A file appears at the intended final name while the download is running.
        std::fs::write(&desired, b"user's file").unwrap();
        let worker_controller = Arc::clone(c);
        let (channels, _) = worker_channels();
        let worker_hash = hash.clone();
        let worker = async move {
            let path = worker_controller
                .publish_completed("a", &worker_hash)
                .await
                .unwrap();
            WorkerExit {
                status: TaskStatus::Completed,
                sha256: Some(worker_hash),
                completed_length: Some(8),
                published_path: path,
                ..WorkerExit::paused()
            }
        };
        c.spawn_worker("a", generation, part.clone(), None, channels, worker)
            .await;
        wait_until_idle(c, "a").await;
        let t = c.get("a").await.unwrap();
        let actual = f.dir.join("file (1).bin");
        assert_eq!(t.status, TaskStatus::Completed);
        assert_eq!(t.save_path, actual.to_string_lossy());
        assert_eq!(t.filename, "file (1).bin");
        assert_eq!(t.final_path, None);
        assert_eq!(t.sha256.as_deref(), Some(hash.as_str()));
        assert_eq!(FileIdentity::at(&actual).unwrap(), identity);
        assert_eq!(std::fs::read(&desired).unwrap(), b"user's file");
        assert!(!part.exists());
        assert!(c.reserved_paths.lock().await.is_empty());
        c.remove("a", true).await.unwrap();
        assert!(!actual.exists());
        assert_eq!(std::fs::read(&desired).unwrap(), b"user's file");
    }

    #[tokio::test]
    async fn persisted_publish_intent_recovers_a_crash_after_rename() {
        let f = fixture(vec![]);
        let history_path = f.dir.join("downloads.json");
        let c = Arc::new(TaskController::new(
            vec![],
            Arc::new(HistoryStore::new(Some(history_path.clone()))),
            Arc::new(NoEvents),
        ));
        let (part, final_path, identity) =
            c.reserve_staged_output(&f.dir, "file.bin").await.unwrap();
        std::fs::write(&part, b"complete").unwrap();
        let mut t = task("a", TaskStatus::Downloading, &part);
        t.file_identity = Some(identity.clone());
        t.final_path = Some(final_path.to_string_lossy().into());
        c.insert_new(t).await;
        let hash = conflux_core::compute_sha256(&part).await.unwrap();
        let target = c.prepare_publish("a", &hash).await.unwrap().unwrap();
        let before = HistoryStore::new(Some(history_path.clone())).load().0;
        assert_eq!(before[0].status, TaskStatus::Paused);
        assert_eq!(before[0].save_path, part.to_string_lossy());
        conflux_core::publish_owned_file(&part, &target, &identity)
            .await
            .unwrap();
        let mut panicked = c.get("a").await.unwrap();
        WorkerExit::crashed().apply(&mut panicked);
        assert_eq!(panicked.sha256.as_deref(), Some(hash.as_str()));
        assert!(panicked.recover_published());
        // No finish_worker/history update: simulate the process exiting right here.
        let recovered = HistoryStore::new(Some(history_path)).load().0;
        assert_eq!(recovered[0].status, TaskStatus::Completed);
        assert_eq!(recovered[0].save_path, final_path.to_string_lossy());
        assert_eq!(recovered[0].final_path, None);
        assert_eq!(recovered[0].sha256.as_deref(), Some(hash.as_str()));
        // A same-size replacement at the final name is never accepted as our file.
        std::fs::rename(&final_path, f.dir.join("original.bin")).unwrap();
        std::fs::write(&final_path, b"complete").unwrap();
        assert_eq!(c.get("a").await.unwrap().status, TaskStatus::Downloading);
        let mut replaced = c.get("a").await.unwrap();
        assert!(!replaced.recover_published());
    }

    #[tokio::test]
    async fn publishing_refuses_to_rename_if_history_cannot_record_the_intent() {
        let f = fixture(vec![]);
        let bad_history_path = f.dir.join("history.json");
        std::fs::create_dir(&bad_history_path).unwrap();
        let c = TaskController::new(
            vec![],
            Arc::new(HistoryStore::new(Some(bad_history_path))),
            Arc::new(NoEvents),
        );
        let (part, final_path, identity) =
            c.reserve_staged_output(&f.dir, "file.bin").await.unwrap();
        std::fs::write(&part, b"complete").unwrap();
        let mut t = task("a", TaskStatus::Downloading, &part);
        t.file_identity = Some(identity);
        t.final_path = Some(final_path.to_string_lossy().into());
        c.insert_new(t).await;
        assert!(c.publish_completed("a", "hash").await.is_err());
        assert_eq!(std::fs::read(&part).unwrap(), b"complete");
        assert!(!final_path.exists());
    }

    /// An engine that notices cancellation, then is stuck (like a blocking write on a
    /// stalled drive) until `release` fires, and only then performs its last write.
    async fn stuck_engine(
        mut cancel: watch::Receiver<bool>,
        release: oneshot::Receiver<()>,
        path: PathBuf,
    ) -> WorkerExit {
        let _ = cancel.wait_for(|c| *c).await;
        let _ = release.await;
        std::fs::write(&path, b"OLD").unwrap(); // the late, in-flight write
        WorkerExit::paused()
    }

    async fn wait_for_status(c: &TaskController, id: &str, status: TaskStatus) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while c.get(id).await.map(|t| t.status) != Some(status) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("task {id} never became {status:?}"));
    }

    async fn wait_until_idle(c: &TaskController, id: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while c.handles.lock().await.contains_key(id) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("engine never exited");
    }

    fn ticket(start: ResumeStart) -> ResumeTicket {
        match start {
            ResumeStart::Ready(ticket) => ticket,
            ResumeStart::AlreadyRunning(t) => panic!("unexpectedly running: {:?}", t.status),
        }
    }

    /// P1: a stop that times out must not hand the file to anyone else while the engine
    /// still has a write in flight; resuming waits until that write has landed, so a restart
    /// against changed remote content can never be overwritten by the old session.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_timeout_keeps_ownership_until_the_engine_exits() {
        let f = fixture(vec![]);
        let c = &f.controller;
        let path = f.dir.join("file.bin");
        let generation = c
            .insert_new(task("a", TaskStatus::Downloading, &path))
            .await;
        assert!(c.reserve_path(&path).await);
        let (channels, cancel_rx) = worker_channels();
        let (release, released) = oneshot::channel();
        let engine = stuck_engine(cancel_rx, released, path.clone());
        assert!(
            c.spawn_worker("a", generation, path.clone(), None, channels, engine)
                .await
        );

        let snapshot = c.stop("a").await.unwrap();
        assert_eq!(snapshot.status, TaskStatus::Stopping);
        assert!(
            !c.reserve_path(&path).await,
            "path still owned by the stuck engine"
        );
        assert_eq!(
            c.begin_resume("a").await.err().as_deref(),
            Some(STILL_STOPPING)
        );
        let removal = c.remove("a", true).await.unwrap_err();
        assert!(removal.starts_with(STILL_STOPPING), "{removal}");
        let kept = c.get("a").await.expect("not removed while stopping");
        assert_eq!(kept.status, TaskStatus::Stopping);
        assert!(
            !kept.lifecycle.removing,
            "removal tombstone cleared after refusing"
        );
        assert!(
            c.adapter_targets().await.is_empty(),
            "no hot-plugging into a stopping task"
        );

        // The stuck write completes; only then is the task paused and the path free.
        release.send(()).unwrap();
        wait_for_status(c, "a", TaskStatus::Paused).await;
        wait_until_idle(c, "a").await;
        assert_eq!(std::fs::read(&path).unwrap(), b"OLD");

        // A resume against changed content restarts from zero, after the old write.
        let ticket = ticket(c.begin_resume("a").await.unwrap());
        assert!(
            c.reserve_path(&path).await,
            "released once the engine exited"
        );
        let (_, generation) = c.commit_resume(ticket, |_| {}).await.unwrap();
        let (channels, _cancel) = worker_channels();
        let fresh = path.clone();
        let engine = async move {
            std::fs::write(&fresh, b"NEWNEWNEW").unwrap();
            WorkerExit {
                status: TaskStatus::Completed,
                completed_length: Some(9),
                ..WorkerExit::paused()
            }
        };
        assert!(
            c.spawn_worker("a", generation, path.clone(), None, channels, engine)
                .await
        );
        wait_for_status(c, "a", TaskStatus::Completed).await;
        assert_eq!(std::fs::read(&path).unwrap(), b"NEWNEWNEW");
    }

    /// P3: a resume that was probing when the task got removed must not resurrect it.
    #[tokio::test]
    async fn removal_during_a_resume_probe_wins() {
        let path = std::env::temp_dir().join(format!("conflux-{}.bin", uuid::Uuid::new_v4()));
        let f = fixture(vec![task("a", TaskStatus::Paused, &path)]);
        let c = &f.controller;
        let ticket = ticket(c.begin_resume("a").await.unwrap());
        c.remove("a", false).await.unwrap();
        assert!(matches!(
            c.commit_resume(ticket, |_| {}).await,
            Err(CommitError::Removed)
        ));
        assert!(c.get("a").await.is_none());
    }

    /// P3: while a removal waits for the engine to stop, no resume may start (the old code
    /// let one slip between "stop finished" and "remove from the list").
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_resume_can_start_while_a_removal_is_stopping_the_task() {
        let f = fixture(vec![]);
        let c = &f.controller;
        let path = f.dir.join("file.bin");
        let generation = c
            .insert_new(task("a", TaskStatus::Downloading, &path))
            .await;
        assert!(c.reserve_path(&path).await);
        let (channels, mut cancel_rx) = worker_channels();
        let (release, released) = oneshot::channel::<()>();
        let engine = async move {
            let _ = cancel_rx.wait_for(|c| *c).await;
            let _ = released.await;
            WorkerExit::paused()
        };
        assert!(
            c.spawn_worker("a", generation, path.clone(), None, channels, engine)
                .await
        );

        let removal = {
            let c = Arc::clone(c);
            tokio::spawn(async move { c.remove("a", false).await })
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            while !c.get("a").await.unwrap().lifecycle.removing {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        // Barrier reached: the removal is waiting for the engine.
        assert_eq!(
            c.begin_resume("a").await.err().as_deref(),
            Some("Download is being removed")
        );
        release.send(()).unwrap();
        // The engine exits within the stop timeout? Not necessarily: the removal may have
        // given up. Either way no resume ran and no task is left running.
        match removal.await.unwrap() {
            Ok(()) => assert!(c.get("a").await.is_none()),
            Err(e) => {
                assert!(e.starts_with(STILL_STOPPING));
                wait_until_idle(c, "a").await;
                c.remove("a", false).await.unwrap();
                assert!(c.get("a").await.is_none());
            }
        }
        assert!(c.handles.lock().await.is_empty());
        assert!(c.reserve_path(&path).await, "path released");
    }

    /// P3: two resumes that both saw `Paused`; only the first may start the engine, and a
    /// late one must not turn a finished download back into `Downloading`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn double_resume_starts_one_engine_and_never_reopens_a_completed_task() {
        let path = std::env::temp_dir().join(format!("conflux-{}.bin", uuid::Uuid::new_v4()));
        let f = fixture(vec![task("a", TaskStatus::Paused, &path)]);
        let c = &f.controller;
        let first = ticket(c.begin_resume("a").await.unwrap());
        let second = ticket(c.begin_resume("a").await.unwrap());

        let (_, generation) = c.commit_resume(first, |_| {}).await.unwrap();
        let (channels, _cancel) = worker_channels();
        let engine = async {
            WorkerExit {
                status: TaskStatus::Completed,
                ..WorkerExit::paused()
            }
        };
        assert!(
            c.spawn_worker("a", generation, path.clone(), None, channels, engine)
                .await
        );
        wait_for_status(c, "a", TaskStatus::Completed).await;

        // The second resume's delayed probe returns now.
        match c.commit_resume(second, |_| {}).await {
            Err(CommitError::Superseded(current)) => {
                assert_eq!(current.status, TaskStatus::Completed)
            }
            other => panic!("stale resume committed: {other:?}"),
        }
        assert_eq!(c.get("a").await.unwrap().status, TaskStatus::Completed);
        assert_eq!(f.events.finished.lock().unwrap().len(), 1, "one engine ran");
    }

    /// "Pause, then resume" while the engine is still winding down resumes once it exited,
    /// instead of reporting "already downloading" and losing the resume.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resume_waits_out_a_stop_in_progress() {
        let f = fixture(vec![]);
        let c = &f.controller;
        let path = f.dir.join("file.bin");
        let generation = c
            .insert_new(task("a", TaskStatus::Downloading, &path))
            .await;
        assert!(c.reserve_path(&path).await);
        let (channels, mut cancel_rx) = worker_channels();
        let engine = async move {
            let _ = cancel_rx.wait_for(|c| *c).await;
            tokio::time::sleep(Duration::from_millis(30)).await; // winding down
            WorkerExit::paused()
        };
        assert!(
            c.spawn_worker("a", generation, path, None, channels, engine)
                .await
        );
        let pause = {
            let c = Arc::clone(c);
            tokio::spawn(async move { c.stop("a").await })
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            while c
                .handles
                .lock()
                .await
                .get("a")
                .is_some_and(|h| !*h.cancel_tx.borrow())
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        let ticket = ticket(c.begin_resume("a").await.unwrap());
        assert_eq!(ticket.task.status, TaskStatus::Paused);
        assert_eq!(pause.await.unwrap().unwrap().status, TaskStatus::Paused);
        assert!(c.commit_resume(ticket, |_| {}).await.is_ok());
    }

    /// P3: pausing while a resume is still probing cancels that resume.
    #[tokio::test]
    async fn pause_during_a_resume_probe_cancels_the_resume() {
        let path = std::env::temp_dir().join(format!("conflux-{}.bin", uuid::Uuid::new_v4()));
        let f = fixture(vec![task("a", TaskStatus::Paused, &path)]);
        let c = &f.controller;
        let ticket = ticket(c.begin_resume("a").await.unwrap());
        assert_eq!(c.stop("a").await.unwrap().status, TaskStatus::Paused);
        match c.commit_resume(ticket, |_| {}).await {
            Err(CommitError::Superseded(current)) => {
                assert_eq!(current.status, TaskStatus::Paused)
            }
            other => panic!("resume survived the pause: {other:?}"),
        }
    }

    /// A pause between committing a start and spawning its engine: the engine never runs.
    #[tokio::test]
    async fn pause_before_spawn_stands_the_engine_down() {
        let f = fixture(vec![]);
        let c = &f.controller;
        let path = f.dir.join("file.bin");
        let generation = c
            .insert_new(task("a", TaskStatus::Downloading, &path))
            .await;
        assert!(c.reserve_path(&path).await);
        assert_eq!(c.stop("a").await.unwrap().status, TaskStatus::Paused);
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = ran.clone();
        let (channels, _cancel) = worker_channels();
        let engine = async move {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            WorkerExit::paused()
        };
        assert!(
            !c.spawn_worker("a", generation, path.clone(), None, channels, engine)
                .await
        );
        tokio::task::yield_now().await;
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
        assert!(c.reserve_path(&path).await, "path released");
    }

    /// A removal before the engine started leaves the placeholder to the start, which then
    /// deletes it (it is empty and still ours).
    #[tokio::test]
    async fn removal_before_spawn_deletes_only_our_empty_placeholder() {
        let f = fixture(vec![]);
        let c = &f.controller;
        let (path, identity) = c.reserve_new_output(&f.dir, "file.bin").await.unwrap();
        let mut new = task("a", TaskStatus::Downloading, &path);
        new.file_identity = identity.clone();
        let generation = c.insert_new(new).await;
        c.remove("a", false).await.unwrap();
        assert!(
            path.exists(),
            "reserved by the pending start: remove leaves it"
        );
        let (channels, _cancel) = worker_channels();
        let engine = async { WorkerExit::paused() };
        assert!(
            !c.spawn_worker("a", generation, path.clone(), identity, channels, engine)
                .await
        );
        assert!(!path.exists(), "the start cleaned up its own placeholder");
    }

    #[tokio::test]
    async fn engine_panic_records_an_error_and_releases_the_path() {
        let f = fixture(vec![]);
        let c = &f.controller;
        let path = f.dir.join("file.bin");
        let generation = c
            .insert_new(task("a", TaskStatus::Downloading, &path))
            .await;
        assert!(c.reserve_path(&path).await);
        let (channels, _cancel) = worker_channels();
        let engine = async {
            if true {
                panic!("engine bug");
            }
            WorkerExit::paused()
        };
        assert!(
            c.spawn_worker("a", generation, path.clone(), None, channels, engine)
                .await
        );
        wait_for_status(c, "a", TaskStatus::Error).await;
        wait_until_idle(c, "a").await;
        assert!(c.reserve_path(&path).await);
        assert!(c.get("a").await.unwrap().error.is_some());
    }

    #[tokio::test]
    async fn late_progress_never_overwrites_a_final_status() {
        let path = std::env::temp_dir().join("unused.bin");
        let f = fixture(vec![task("a", TaskStatus::Paused, &path)]);
        let c = &f.controller;
        assert!(c.update_running("a", |t| t.downloaded_bytes = 5).await);
        assert_eq!(c.get("a").await.unwrap().downloaded_bytes, 0);
        assert!(!c.update_running("missing", |_| {}).await);
    }

    #[test]
    fn numbered_filename_inserts_counter_before_extension() {
        assert_eq!(numbered_filename("file.iso", 1), "file (1).iso");
        assert_eq!(numbered_filename("archive.tar.gz", 2), "archive.tar (2).gz");
        assert_eq!(numbered_filename("README", 3), "README (3)");
        assert_eq!(numbered_filename(".bashrc", 1), ".bashrc (1)");
    }

    #[tokio::test]
    async fn reserved_paths_never_collide() {
        let dir = std::env::temp_dir().join(format!("conflux-reserve-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let reserved = Mutex::new(HashSet::new());

        let a = reserve_output_path(&reserved, &[], &dir, "file.bin")
            .await
            .unwrap()
            .0;
        let b = reserve_output_path(&reserved, &[], &dir, "file.bin")
            .await
            .unwrap()
            .0;
        let c = reserve_output_path(&reserved, &[], &dir, "file.bin")
            .await
            .unwrap()
            .0;

        assert_eq!(a, dir.join("file.bin"));
        assert_eq!(b, dir.join("file (1).bin"));
        assert_eq!(c, dir.join("file (2).bin"));
        assert_eq!(reserved.lock().await.len(), 3);
        // Each name is claimed on disk with an empty placeholder.
        for p in [&a, &b, &c] {
            assert_eq!(std::fs::metadata(p).unwrap().len(), 0);
        }

        // A file created by someone else (another process) is skipped, even unreserved.
        std::fs::write(dir.join("other.bin"), b"keep").unwrap();
        let d = reserve_output_path(&reserved, &[], &dir, "other.bin")
            .await
            .unwrap()
            .0;
        assert_eq!(d, dir.join("other (1).bin"));
        assert_eq!(std::fs::read(dir.join("other.bin")).unwrap(), b"keep");

        // A reserved path missing on disk (running task) is skipped too.
        let resumed = dir.join("resumed.bin");
        reserved.lock().await.insert(resumed.clone());
        let e = reserve_output_path(&reserved, &[], &dir, "resumed.bin")
            .await
            .unwrap()
            .0;
        assert_eq!(e, dir.join("resumed (1).bin"));
        assert!(!resumed.exists());

        // A claimed path from an inactive/paused task (missing on disk) is skipped.
        let paused_path = dir.join("paused.bin");
        let f = reserve_output_path(
            &reserved,
            std::slice::from_ref(&paused_path),
            &dir,
            "paused.bin",
        )
        .await
        .unwrap()
        .0;
        assert_eq!(f, dir.join("paused (1).bin"));

        // A candidate with an existing resume sidecar is skipped.
        let sidecar_target = dir.join("with_sidecar.bin");
        std::fs::write(conflux_core::resume_sidecar_path(&sidecar_target), b"{}").unwrap();
        let g = reserve_output_path(&reserved, &[], &dir, "with_sidecar.bin")
            .await
            .unwrap()
            .0;
        assert_eq!(g, dir.join("with_sidecar (1).bin"));

        assert!(
            reserve_output_path(&reserved, &[], &dir.join("missing"), "x.bin")
                .await
                .is_err()
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
