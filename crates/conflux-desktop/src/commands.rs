use crate::adapters::{adapter_names, adapter_stat, to_info, AdapterInfo};
use crate::history::HistoryStore;
use crate::settings::{self, Settings};
use crate::state::{AppState, DownloadTaskState, TaskHandle, TaskStatus};
use conflux_core::{
    discover_adapters as core_discover, remove_resume_sidecar, sanitize_filename, unique_path,
    DownloadCancelled, DownloadEngine, DownloadProbe, NetworkAdapter, ProgressUpdate,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Event carrying a full `DownloadTaskState` snapshot after every state change.
pub const PROGRESS_EVENT: &str = "download-progress";

/// How long a stop request waits for the engine to wind down its workers
/// before the task is forcibly aborted.
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for the progress forwarder to drain after the engine returns.
const FORWARDER_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Minimum interval between history saves caused by progress ticks alone.
const HISTORY_SAVE_INTERVAL: Duration = Duration::from_secs(5);

/// Shared handles a running task needs; cloned out of `AppState`.
#[derive(Clone)]
struct Shared {
    app: AppHandle,
    tasks: Arc<RwLock<HashMap<String, DownloadTaskState>>>,
    handles: Arc<Mutex<HashMap<String, TaskHandle>>>,
    reserved_paths: Arc<Mutex<HashSet<PathBuf>>>,
    settings: Arc<RwLock<Settings>>,
    history: Arc<HistoryStore>,
}

impl Shared {
    fn new(app: &AppHandle, state: &AppState) -> Self {
        Self {
            app: app.clone(),
            tasks: state.tasks.clone(),
            handles: state.handles.clone(),
            reserved_paths: state.reserved_paths.clone(),
            settings: state.settings.clone(),
            history: state.history.clone(),
        }
    }

    /// Applies `f` to the task and emits the resulting snapshot, both under the write lock
    /// so events are emitted in exactly the order the changes happened.
    /// Returns `None` (and emits nothing) if the task no longer exists.
    async fn update(
        &self,
        task_id: &str,
        f: impl FnOnce(&mut DownloadTaskState),
    ) -> Option<DownloadTaskState> {
        let mut tasks = self.tasks.write().await;
        let task = tasks.get_mut(task_id)?;
        f(task);
        let snapshot = task.clone();
        emit(&self.app, &snapshot);
        Some(snapshot)
    }

    async fn save_history(&self) {
        self.history.save(&self.tasks).await;
    }
}

fn emit(app: &AppHandle, task: &DownloadTaskState) {
    if let Err(e) = app.emit(PROGRESS_EVENT, task) {
        warn!(task_id = %task.id, "Failed to emit progress event: {e}");
    }
}

// ─── 1. Adapter Discovery ───────────────────────────────────
#[tauri::command]
pub fn discover_adapters() -> Result<Vec<AdapterInfo>, String> {
    let adapters = core_discover().map_err(|e| e.to_string())?;
    Ok(adapters.into_iter().map(to_info).collect())
}

// ─── 2. URL Probe ───────────────────────────────────────────
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub url: String,
    pub filename: String,
    pub total_bytes: u64,
    pub supports_ranges: bool,
}

#[tauri::command]
pub async fn probe_url(url: String) -> Result<ProbeResult, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("Download URL is empty".to_string());
    }
    let probe = DownloadEngine::default()
        .probe(&url)
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(ProbeResult {
        url: probe.url,
        filename: probe.suggested_filename,
        total_bytes: probe.total_bytes,
        supports_ranges: probe.supports_ranges,
    })
}

// ─── 3. Start Download ─────────────────────────────────────
#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    save_dir: String,
    filename: Option<String>,
    adapter_ids: Vec<String>,
) -> Result<DownloadTaskState, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("Download URL is empty".to_string());
    }
    let dir = validate_save_dir(&save_dir)?;
    let adapters = select_adapters(&adapter_ids)?;
    let engine = engine_from_settings(&*state.settings.read().await)?;

    // Probe exactly once; the same probe is handed to the engine.
    let probe = engine
        .probe(&url)
        .await
        .map_err(|e| format!("Probe failed: {e:#}"))?;

    let requested_name = filename
        .map(|f| sanitize_filename(&f))
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| probe.suggested_filename.clone());
    let output_path = reserve_output_path(&state.reserved_paths, &dir, &requested_name).await;
    let save_path = output_path.to_string_lossy().to_string();
    let filename = output_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or(requested_name);

    let task_id = Uuid::new_v4().to_string();
    info!(
        task_id = %task_id,
        url = %probe.url,
        total_bytes = probe.total_bytes,
        supports_ranges = probe.supports_ranges,
        output = %save_path,
        adapters = ?enabled_ids(&adapters),
        "Starting download"
    );

    let task = DownloadTaskState {
        id: task_id.clone(),
        url,
        filename,
        save_dir: dir.to_string_lossy().to_string(),
        save_path,
        adapter_ids,
        total_bytes: probe.total_bytes,
        supports_ranges: probe.supports_ranges,
        downloaded_bytes: 0,
        status: TaskStatus::Downloading,
        speed_bytes_sec: 0.0,
        eta_seconds: 0,
        active_chunks: 0,
        completed_chunks: 0,
        total_chunks: 0,
        sha256: None,
        error: None,
        created_at_ms: now_ms(),
        completed_at_ms: None,
        adapters: Vec::new(),
        chunk_map: None,
    };
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task_id.clone(), task.clone());
        emit(&app, &task);
    }

    let shared = Shared::new(&app, &state);
    shared.save_history().await;
    spawn_task(shared, task_id, engine, probe, output_path, adapters, false).await;
    Ok(task)
}

// ─── 4. Pause ───────────────────────────────────────────────
/// Stops the download, keeping its partial file and resume sidecar. Returns the resulting
/// snapshot (status `completed` if it finished before the stop took effect).
#[tauri::command]
pub async fn pause_download(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<DownloadTaskState, String> {
    let shared = Shared::new(&app, &state);
    let task = stop_task(&shared, &task_id)
        .await
        .ok_or_else(|| format!("Unknown task: {task_id}"))?;
    shared.save_history().await;
    Ok(task)
}

// ─── 5. Resume ──────────────────────────────────────────────
/// Continues a paused or failed download from its completed chunks (or from zero if the
/// engine finds its resume data unusable).
#[tauri::command]
pub async fn resume_download(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<DownloadTaskState, String> {
    let task = state
        .tasks
        .read()
        .await
        .get(&task_id)
        .cloned()
        .ok_or_else(|| format!("Unknown task: {task_id}"))?;
    match task.status {
        TaskStatus::Paused | TaskStatus::Error => {}
        TaskStatus::Downloading => return Ok(task),
        TaskStatus::Completed => return Err("Download is already complete".to_string()),
    }
    if state.handles.lock().await.contains_key(&task_id) {
        return Err("Download is still stopping; try again in a moment".to_string());
    }

    let dir = validate_save_dir(&task.save_dir)?;
    let adapters = select_adapters(&task.adapter_ids)?;
    let engine = engine_from_settings(&*state.settings.read().await)?;
    let probe = engine
        .probe(&task.url)
        .await
        .map_err(|e| format!("Probe failed: {e:#}"))?;

    let output_path = PathBuf::from(&task.save_path);
    if output_path.parent() != Some(dir.as_path()) {
        warn!(task_id = %task_id, "Saved path is outside the saved folder; resuming anyway");
    }
    {
        let mut reserved = state.reserved_paths.lock().await;
        if !reserved.insert(output_path.clone()) {
            return Err(format!(
                "{} is being written by another download",
                task.save_path
            ));
        }
    }

    info!(task_id = %task_id, url = %probe.url, output = %task.save_path, "Resuming download");
    let shared = Shared::new(&app, &state);
    let total_bytes = probe.total_bytes;
    let supports_ranges = probe.supports_ranges;
    let snapshot = shared
        .update(&task_id, |t| {
            t.status = TaskStatus::Downloading;
            t.total_bytes = total_bytes;
            t.supports_ranges = supports_ranges;
            t.error = None;
            t.sha256 = None;
            t.completed_at_ms = None;
            t.clear_runtime();
        })
        .await;
    let Some(snapshot) = snapshot else {
        // Removed while we were probing.
        state.reserved_paths.lock().await.remove(&output_path);
        return Err(format!("Unknown task: {task_id}"));
    };
    shared.save_history().await;
    spawn_task(shared, task_id, engine, probe, output_path, adapters, true).await;
    Ok(snapshot)
}

// ─── 6. Remove ──────────────────────────────────────────────
/// Stops the task if running and removes it from the list. An incomplete download's
/// partial file and resume data are always deleted; a completed file only if `delete_file`.
#[tauri::command]
pub async fn remove_download(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    delete_file: bool,
) -> Result<(), String> {
    let shared = Shared::new(&app, &state);
    stop_task(&shared, &task_id).await;

    let removed = state.tasks.write().await.remove(&task_id);
    let Some(task) = removed else {
        return Ok(());
    };
    shared.save_history().await;

    let completed = task.status == TaskStatus::Completed;
    if completed && !delete_file {
        return Ok(());
    }

    let path = PathBuf::from(&task.save_path);
    // Never delete a file that another task now owns (e.g. this task stopped before the
    // file was created on disk, and a later task was assigned the same path).
    let reserved = state.reserved_paths.lock().await.contains(&path);
    let owned_by_other_task = state
        .tasks
        .read()
        .await
        .values()
        .any(|t| t.save_path == task.save_path);
    if reserved || owned_by_other_task {
        info!(task_id = %task_id, path = %task.save_path, "File is owned by another task; not deleting");
        return Ok(());
    }

    if let Err(e) = remove_resume_sidecar(&path) {
        warn!(task_id = %task_id, path = %task.save_path, "Failed to delete resume data: {e}");
    }
    match tokio::fs::remove_file(&path).await {
        Ok(()) => info!(task_id = %task_id, path = %task.save_path, "Deleted file"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            warn!(task_id = %task_id, path = %task.save_path, "Failed to delete file: {e}");
            if completed {
                return Err(format!(
                    "Removed from the list, but the file could not be deleted: {e}"
                ));
            }
        }
    }
    Ok(())
}

// ─── 7. List Tasks ──────────────────────────────────────────
/// All known tasks, newest first.
#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<DownloadTaskState>, String> {
    let mut tasks: Vec<DownloadTaskState> = state.tasks.read().await.values().cloned().collect();
    tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at_ms));
    Ok(tasks)
}

// ─── 8. Settings ────────────────────────────────────────────
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.read().await.clone())
}

/// Validates, clamps, saves and applies settings. New values apply to downloads started
/// or resumed afterwards. Returns the settings as stored.
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let settings = settings.normalized();
    settings.validate()?;
    if let Some(path) = &state.settings_path {
        settings::save(path, &settings)?;
    }
    *state.settings.write().await = settings.clone();
    info!(?settings, "Settings updated");
    Ok(settings)
}

// ─── 9. Open / Reveal ───────────────────────────────────────
/// Opens a completed download with the default application.
#[tauri::command]
pub async fn open_file(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<(), String> {
    let task = state
        .tasks
        .read()
        .await
        .get(&task_id)
        .cloned()
        .ok_or_else(|| format!("Unknown task: {task_id}"))?;
    if task.status != TaskStatus::Completed {
        return Err("Only completed downloads can be opened".to_string());
    }
    if !Path::new(&task.save_path).is_file() {
        return Err(format!("File no longer exists: {}", task.save_path));
    }
    app.opener()
        .open_path(task.save_path, None::<&str>)
        .map_err(|e| format!("Failed to open file: {e}"))
}

/// Shows the download in the file manager (the folder if the file does not exist yet).
#[tauri::command]
pub async fn reveal_file(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<(), String> {
    let task = state
        .tasks
        .read()
        .await
        .get(&task_id)
        .cloned()
        .ok_or_else(|| format!("Unknown task: {task_id}"))?;
    let path = PathBuf::from(&task.save_path);
    if path.exists() {
        app.opener()
            .reveal_item_in_dir(&path)
            .map_err(|e| format!("Failed to show file: {e}"))
    } else if Path::new(&task.save_dir).is_dir() {
        app.opener()
            .open_path(task.save_dir, None::<&str>)
            .map_err(|e| format!("Failed to open folder: {e}"))
    } else {
        Err(format!("Folder no longer exists: {}", task.save_dir))
    }
}

// ─── 10. Window Theme / Mica ────────────────────────────────
#[derive(Debug, Clone, Serialize)]
pub struct WindowBackdrop {
    pub mica: bool,
}

/// Applies the Windows 11 Mica backdrop tinted for `dark`. Returns `mica: false` where it is
/// unsupported (Windows 10, Linux, macOS); the UI then paints an opaque background.
#[tauri::command]
pub fn apply_window_theme(window: WebviewWindow, dark: bool) -> WindowBackdrop {
    let theme = if dark {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    if let Err(e) = window.set_theme(Some(theme)) {
        warn!("Failed to set window theme: {e}");
    }
    match window_vibrancy::apply_mica(&window, Some(dark)) {
        Ok(()) => WindowBackdrop { mica: true },
        Err(e) => {
            info!("Mica backdrop unavailable: {e}");
            let _ = window_vibrancy::clear_mica(&window);
            WindowBackdrop { mica: false }
        }
    }
}

// ─── Running a task ─────────────────────────────────────────
/// Spawns the engine for `task_id` and registers its handle. `output_path` must already be
/// reserved; it is released when the task ends.
async fn spawn_task(
    shared: Shared,
    task_id: String,
    engine: DownloadEngine,
    probe: DownloadProbe,
    output_path: PathBuf,
    adapters: Vec<NetworkAdapter>,
    resume: bool,
) {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    // Hold the handles lock across spawn + insert: the task removes its own handle when it
    // finishes, and that removal must not run before the handle has been inserted.
    let mut handles = shared.handles.lock().await;
    let join_handle = tokio::spawn(run_download(
        shared.clone(),
        task_id.clone(),
        engine,
        probe,
        output_path.clone(),
        adapters,
        cancel_rx,
        resume,
    ));
    handles.insert(
        task_id,
        TaskHandle {
            cancel_tx,
            join_handle,
            output_path,
        },
    );
}

#[allow(clippy::too_many_arguments)]
async fn run_download(
    shared: Shared,
    task_id: String,
    engine: DownloadEngine,
    probe: DownloadProbe,
    output_path: PathBuf,
    adapters: Vec<NetworkAdapter>,
    cancel_rx: watch::Receiver<bool>,
    resume: bool,
) {
    let (tx, mut rx) = mpsc::channel::<ProgressUpdate>(100);
    let names = adapter_names(&adapters);

    let mut forwarder = {
        let shared = shared.clone();
        let id = task_id.clone();
        tokio::spawn(async move {
            let mut last_save = Instant::now();
            while let Some(p) = rx.recv().await {
                let stats = p.adapters.iter().map(|a| adapter_stat(a, &names)).collect();
                let exists = shared
                    .update(&id, |task| {
                        // A late tick must never undo a final status set elsewhere.
                        if task.status != TaskStatus::Downloading {
                            return;
                        }
                        task.downloaded_bytes = p.downloaded_bytes;
                        if p.total_bytes > 0 {
                            task.total_bytes = p.total_bytes;
                        }
                        task.speed_bytes_sec = p.speed_bytes_sec;
                        task.eta_seconds = p.eta_seconds;
                        task.active_chunks = p.active_chunks;
                        task.completed_chunks = p.completed_chunks;
                        task.total_chunks = p.total_chunks;
                        task.adapters = stats;
                        if let Some(map) = p.chunk_map {
                            task.chunk_map = Some(map);
                        }
                    })
                    .await
                    .is_some();
                if exists && last_save.elapsed() >= HISTORY_SAVE_INTERVAL {
                    last_save = Instant::now();
                    shared.save_history().await;
                }
            }
        })
    };

    let result = if resume {
        engine
            .resume(&probe, &output_path, &adapters, Some(tx), cancel_rx.clone())
            .await
    } else {
        engine
            .download(&probe, &output_path, &adapters, Some(tx), cancel_rx.clone())
            .await
    };

    // The engine drops its sender when it returns, which ends the forwarder. Bound the wait in
    // case a detached engine task still holds a clone, so the final event is never blocked.
    if tokio::time::timeout(FORWARDER_DRAIN_TIMEOUT, &mut forwarder)
        .await
        .is_err()
    {
        warn!(task_id = %task_id, "Progress forwarder did not drain in time; aborting it");
        forwarder.abort();
        let _ = forwarder.await;
    }

    let stop_requested = *cancel_rx.borrow();
    let (status, sha256, err_msg) = match result {
        Ok(hash) => {
            info!(task_id = %task_id, sha256 = %hash, "Download completed");
            (TaskStatus::Completed, Some(hash), None)
        }
        // Also treat any error after a stop request as a pause: tearing down sockets
        // mid-transfer can surface as an I/O error rather than `DownloadCancelled`.
        Err(e) if e.downcast_ref::<DownloadCancelled>().is_some() || stop_requested => {
            info!(task_id = %task_id, "Download paused by user");
            (TaskStatus::Paused, None, None)
        }
        Err(e) => {
            error!(task_id = %task_id, "Download failed: {e:#}");
            (TaskStatus::Error, None, Some(format!("{e:#}")))
        }
    };

    let final_task = shared
        .update(&task_id, |task| {
            task.status = status;
            task.clear_runtime();
            if status == TaskStatus::Completed {
                task.downloaded_bytes = task.total_bytes;
                task.completed_chunks = task.total_chunks;
                task.completed_at_ms = Some(now_ms());
            }
            task.sha256 = sha256;
            task.error = err_msg;
        })
        .await;

    shared.handles.lock().await.remove(&task_id);
    shared.reserved_paths.lock().await.remove(&output_path);
    shared.save_history().await;

    if let Some(task) = final_task {
        notify_finished(&shared, &task).await;
    }
}

async fn notify_finished(shared: &Shared, task: &DownloadTaskState) {
    if !shared.settings.read().await.notify_on_complete {
        return;
    }
    let (title, body) = match task.status {
        TaskStatus::Completed => ("Download complete", task.filename.clone()),
        TaskStatus::Error => (
            "Download failed",
            format!(
                "{}: {}",
                task.filename,
                task.error.as_deref().unwrap_or("unknown error")
            ),
        ),
        TaskStatus::Paused | TaskStatus::Downloading => return,
    };
    if let Err(e) = shared
        .app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        warn!(task_id = %task.id, "Failed to show notification: {e}");
    }
}

/// Signals the task to stop, waits for it to wind down (aborting after `STOP_TIMEOUT`),
/// and returns the task's resulting snapshot. `None` if the task id is unknown.
async fn stop_task(shared: &Shared, task_id: &str) -> Option<DownloadTaskState> {
    // Take the handle out and release the lock before awaiting: the task itself locks
    // `handles` during cleanup.
    let handle = shared.handles.lock().await.remove(task_id);

    if let Some(TaskHandle {
        cancel_tx,
        mut join_handle,
        output_path,
    }) = handle
    {
        let _ = cancel_tx.send(true);
        if tokio::time::timeout(STOP_TIMEOUT, &mut join_handle)
            .await
            .is_err()
        {
            warn!(
                task_id,
                "Download did not stop within {:?}; aborting", STOP_TIMEOUT
            );
            join_handle.abort();
            let _ = join_handle.await;
        }
        // Normally released by the task itself; required if it was aborted.
        shared.reserved_paths.lock().await.remove(&output_path);
        drop(cancel_tx);
    }

    let current = shared.tasks.read().await.get(task_id).cloned()?;
    if current.status != TaskStatus::Downloading {
        return Some(current);
    }
    // The task was aborted before it could record its own final status.
    shared
        .update(task_id, |task| {
            if task.status == TaskStatus::Downloading {
                task.status = TaskStatus::Paused;
                task.clear_runtime();
            }
        })
        .await
}

// ─── Helpers ────────────────────────────────────────────────
fn engine_from_settings(settings: &Settings) -> Result<DownloadEngine, String> {
    DownloadEngine::new(
        settings.chunk_size_bytes(),
        settings.connections_per_adapter as usize,
    )
    .map_err(|e| format!("Invalid engine settings: {e:#}"))
}

/// Discovers adapters and enables exactly `adapter_ids` (discovery defaults when empty).
fn select_adapters(adapter_ids: &[String]) -> Result<Vec<NetworkAdapter>, String> {
    let mut adapters = core_discover().map_err(|e| format!("Adapter discovery failed: {e:#}"))?;
    if !adapter_ids.is_empty() {
        for adapter in adapters.iter_mut() {
            adapter.enabled = adapter_ids.contains(&adapter.id);
        }
        if !adapters.iter().any(|a| a.enabled) {
            return Err(
                "None of the selected network adapters are currently available. \
                 Refresh the adapter list and choose again."
                    .to_string(),
            );
        }
    }
    Ok(adapters)
}

fn enabled_ids(adapters: &[NetworkAdapter]) -> Vec<&str> {
    adapters
        .iter()
        .filter(|a| a.enabled)
        .map(|a| a.id.as_str())
        .collect()
}

fn validate_save_dir(save_dir: &str) -> Result<PathBuf, String> {
    let trimmed = save_dir.trim();
    if trimmed.is_empty() {
        return Err("Choose a destination folder".to_string());
    }
    let dir = PathBuf::from(trimmed);
    if !dir.is_absolute() {
        return Err(format!(
            "Destination folder must be an absolute path: {trimmed}"
        ));
    }
    if !dir.is_dir() {
        return Err(format!("Destination folder does not exist: {trimmed}"));
    }
    Ok(dir)
}

/// Picks a path that neither exists on disk nor is reserved by a running task, and reserves it.
async fn reserve_output_path(
    reserved: &Mutex<HashSet<PathBuf>>,
    dir: &Path,
    filename: &str,
) -> PathBuf {
    let mut reserved = reserved.lock().await;
    // `unique_path` only knows about the filesystem; running tasks may not have created
    // their file yet, so also skip reserved candidates.
    let mut candidate = unique_path(dir, filename);
    let mut n: u32 = 1;
    while reserved.contains(&candidate) || candidate.exists() {
        candidate = dir.join(numbered_filename(filename, n));
        n += 1;
    }
    reserved.insert(candidate.clone());
    candidate
}

/// "name.ext" -> "name (n).ext"; "name" -> "name (n)"; ".hidden" -> ".hidden (n)".
fn numbered_filename(filename: &str, n: u32) -> String {
    match filename.rfind('.') {
        Some(idx) if idx > 0 => format!("{} ({}){}", &filename[..idx], n, &filename[idx..]),
        _ => format!("{} ({})", filename, n),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_filename_inserts_counter_before_extension() {
        assert_eq!(numbered_filename("file.iso", 1), "file (1).iso");
        assert_eq!(numbered_filename("archive.tar.gz", 2), "archive.tar (2).gz");
        assert_eq!(numbered_filename("README", 3), "README (3)");
        assert_eq!(numbered_filename(".bashrc", 1), ".bashrc (1)");
    }

    #[test]
    fn validate_save_dir_rejects_empty_and_relative() {
        assert!(validate_save_dir("").is_err());
        assert!(validate_save_dir("   ").is_err());
        assert!(validate_save_dir("Downloads").is_err());
        let tmp = std::env::temp_dir();
        assert_eq!(validate_save_dir(tmp.to_str().unwrap()).unwrap(), tmp);
    }

    #[tokio::test]
    async fn reserved_paths_never_collide() {
        let dir = std::env::temp_dir().join(format!("conflux-reserve-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let reserved = Mutex::new(HashSet::new());

        let a = reserve_output_path(&reserved, &dir, "file.bin").await;
        let b = reserve_output_path(&reserved, &dir, "file.bin").await;
        let c = reserve_output_path(&reserved, &dir, "file.bin").await;

        assert_eq!(a, dir.join("file.bin"));
        assert_eq!(b, dir.join("file (1).bin"));
        assert_eq!(c, dir.join("file (2).bin"));
        assert_eq!(reserved.lock().await.len(), 3);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
