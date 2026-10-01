use crate::adapters::{adapter_names, adapter_stat, AdapterInfo};
use crate::history::HistoryStore;
use crate::redact::redact_error;
use crate::settings::{self, Settings};
use crate::state::{AppState, DownloadTaskState, TaskHandle, TaskStatus};
use conflux_core::{
    discover_adapters as core_discover, remove_resume_sidecar, resume_sidecar_path,
    sanitize_filename, AdapterUpdate, DownloadCancelled, DownloadEngine, DownloadProbe,
    NetworkAdapter, ProgressUpdate,
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
        crate::tray::update_tray_tooltip(&self.app, &tasks);
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
pub async fn discover_adapters(state: State<'_, AppState>) -> Result<Vec<AdapterInfo>, String> {
    let adapters = refresh_adapters(&state, None, AddPolicy::IfAutoAggregate).await?;
    Ok(state.adapter_infos(adapters))
}

/// Whether adapters that became enabled are hot-added to running downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddPolicy {
    /// Only when the `auto_aggregate_adapters` setting is on (passive refreshes).
    IfAutoAggregate,
    /// Always (the user explicitly enabled an adapter).
    Always,
}

/// Discovers the adapters, applies the current overrides, records the result as
/// `last_adapters`, and forwards the difference to running downloads. Returns the adapters
/// with overrides applied. If discovery fails, `fallback` (the watcher's own snapshot) is used
/// when given, otherwise the error is returned.
///
/// Every writer of `last_adapters` goes through here (or [`set_adapter_enabled`], which holds
/// the same lock and calls [`refresh_locked`]). Discovery itself happens only after the
/// `last_adapters` write lock is taken, so two concurrent refreshes cannot publish an older
/// snapshot over a newer one. The settings are read under that lock too, and the diff is
/// forwarded before it is released, so a concurrent adapter toggle can never be overwritten
/// with stale overrides (which would re-add a just-disabled adapter) and add/remove updates
/// reach the engine in order.
/// Lock order: `last_adapters` -> `settings` -> `handles` -> `tasks`; nothing may take
/// `last_adapters` while holding `settings`.
pub async fn refresh_adapters(
    state: &AppState,
    fallback: Option<Vec<NetworkAdapter>>,
    policy: AddPolicy,
) -> Result<Vec<NetworkAdapter>, String> {
    let mut last = state.last_adapters.write().await;
    let discovered = match (core_discover(), fallback) {
        (Ok(discovered), _) => discovered,
        (Err(e), Some(snapshot)) => {
            warn!("Adapter discovery failed, using the watcher snapshot: {e:#}");
            snapshot
        }
        (Err(e), None) => return Err(redact_error(&format!("{e:#}"))),
    };
    Ok(refresh_locked(state, &mut last, discovered, policy).await)
}

/// The body of [`refresh_adapters`], for callers that already hold the `last_adapters` lock.
async fn refresh_locked(
    state: &AppState,
    last: &mut Vec<NetworkAdapter>,
    mut discovered: Vec<NetworkAdapter>,
    policy: AddPolicy,
) -> Vec<NetworkAdapter> {
    if let Ok(mut defaults) = state.adapter_defaults.lock() {
        // Discovery's verdict, recorded before overrides change `enabled`.
        *defaults = discovered
            .iter()
            .map(|a| (a.id.clone(), a.enabled))
            .collect();
    }
    let apply_added = {
        let settings = state.settings.read().await;
        for a in discovered.iter_mut() {
            crate::adapters::apply_overrides(a, &settings.adapter_overrides);
        }
        policy == AddPolicy::Always || settings.auto_aggregate_adapters
    };
    let (added, removed) = conflux_core::diff_adapters(last, &discovered);
    *last = discovered.clone();
    let added = if apply_added { added } else { Vec::new() };
    if !added.is_empty() || !removed.is_empty() {
        handle_network_change(state, &added, &removed).await;
    }
    discovered
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
        .map_err(|e| redact_error(&format!("{e:#}")))?;
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
) -> Result<DownloadTaskState, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("Download URL is empty".to_string());
    }
    let dir = validate_save_dir(&save_dir)?;
    let settings = state.settings.read().await.clone();
    let adapters = select_adapters(&settings)?;
    let engine = engine_from_settings(&settings)?;

    // Probe exactly once; the same probe is handed to the engine.
    let probe = engine
        .probe(&url)
        .await
        .map_err(|e| format!("Probe failed: {}", redact_error(&format!("{e:#}"))))?;

    let requested_name = filename
        .map(|f| sanitize_filename(&f))
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| probe.suggested_filename.clone());
    let output_path = reserve_output_path(&state.reserved_paths, &dir, &requested_name).await?;
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
        adapter_ids: vec![],
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
        crate::tray::update_tray_tooltip(&app, &tasks);
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

pub async fn resume_task_internal(
    app: &AppHandle,
    state: &AppState,
    task_id: &str,
) -> Result<DownloadTaskState, String> {
    let task = state
        .tasks
        .read()
        .await
        .get(task_id)
        .cloned()
        .ok_or_else(|| format!("Unknown task: {task_id}"))?;
    match task.status {
        TaskStatus::Paused | TaskStatus::Error => {}
        TaskStatus::Downloading => return Ok(task),
        TaskStatus::Completed => return Err("Download is already complete".to_string()),
    }
    if state.handles.lock().await.contains_key(task_id) {
        return Err("Download is still stopping; try again in a moment".to_string());
    }

    let dir = validate_save_dir(&task.save_dir)?;
    let settings = state.settings.read().await.clone();
    let adapters = select_adapters(&settings)?;
    let engine = engine_from_settings(&settings)?;
    let probe = engine
        .probe(&task.url)
        .await
        .map_err(|e| format!("Probe failed: {}", redact_error(&format!("{e:#}"))))?;

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
    let shared = Shared::new(app, state);
    let total_bytes = probe.total_bytes;
    let supports_ranges = probe.supports_ranges;
    let snapshot = shared
        .update(task_id, |t| {
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
    spawn_task(
        shared,
        task_id.to_string(),
        engine,
        probe,
        output_path,
        adapters,
        true,
    )
    .await;
    Ok(snapshot)
}

pub async fn pause_all_internal(app: &AppHandle, state: &AppState) {
    let running_ids: Vec<String> = {
        let tasks = state.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.status == TaskStatus::Downloading)
            .map(|t| t.id.clone())
            .collect()
    };
    let shared = Shared::new(app, state);
    // Stop concurrently: each stop may wait up to `STOP_TIMEOUT`, and quitting waits for this,
    // so the total wait is bounded by one timeout rather than one per task.
    let mut stops = tokio::task::JoinSet::new();
    for id in running_ids {
        let shared = shared.clone();
        stops.spawn(async move {
            stop_task(&shared, &id).await;
        });
    }
    while let Some(result) = stops.join_next().await {
        if let Err(e) = result {
            warn!("Stopping a task failed: {e}");
        }
    }
    shared.save_history().await;
}

pub async fn resume_all_internal(app: &AppHandle, state: &AppState) {
    let paused_ids: Vec<String> = {
        let tasks = state.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.status == TaskStatus::Paused || t.status == TaskStatus::Error)
            .map(|t| t.id.clone())
            .collect()
    };
    for id in paused_ids {
        if let Err(e) = resume_task_internal(app, state, &id).await {
            warn!(task_id = %id, "Failed to resume task in resume_all: {e}");
        }
    }
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
    resume_task_internal(&app, &state, &task_id).await
}

#[tauri::command]
pub async fn pause_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    pause_all_internal(&app, &state).await;
    Ok(())
}

#[tauri::command]
pub async fn resume_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    resume_all_internal(&app, &state).await;
    Ok(())
}

// ─── 6. Remove ──────────────────────────────────────────────
/// Stops the task if running and removes it from the list. An incomplete download's resume
/// data is deleted, and its partial file too when it is evidently Conflux's (see
/// `partial_file_is_ours`); a completed file is deleted only if `delete_file`.
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
    {
        let tasks = state.tasks.read().await;
        crate::tray::update_tray_tooltip(&app, &tasks);
    }
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

    if !completed {
        let sidecar_exists = tokio::fs::try_exists(resume_sidecar_path(&path))
            .await
            .unwrap_or(false);
        let file_len = tokio::fs::metadata(&path).await.ok().map(|m| m.len());
        let ours = partial_file_is_ours(&task, file_len, sidecar_exists);
        if let Err(e) = remove_resume_sidecar(&path) {
            warn!(task_id = %task_id, path = %task.save_path, "Failed to delete resume data: {e}");
        }
        if !ours {
            if file_len.is_some() {
                info!(
                    task_id = %task_id,
                    path = %task.save_path,
                    ?file_len,
                    total_bytes = task.total_bytes,
                    "File does not look like this download's partial file; not deleting"
                );
            }
            return Ok(());
        }
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

/// Whether the file at an incomplete task's `save_path` is evidently the one Conflux wrote,
/// so removing the task may delete it. The path may since have been taken by something else
/// (the user deleted the partial file and saved another file under the same name), so a
/// file is only deleted with positive evidence:
/// - its resume sidecar exists (written next to the file by the chunked engine), or
/// - it is a chunked download and the file length equals the pre-allocated `total_bytes`, or
/// - it is empty (the placeholder claimed when the download started; nothing to lose).
///
/// `file_len` is `None` when there is no file.
fn partial_file_is_ours(task: &DownloadTaskState, file_len: Option<u64>, sidecar: bool) -> bool {
    let Some(len) = file_len else {
        return false;
    };
    let chunked = task.supports_ranges && task.total_bytes > 0;
    sidecar || len == 0 || (chunked && len == task.total_bytes)
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
    // Holding the write lock across the save serialises settings writers, so neither this
    // nor `set_adapter_enabled` can overwrite the other's change.
    let mut current = state.settings.write().await;
    let settings = current.apply_update(settings)?;
    if let Some(path) = &state.settings_path {
        settings::save(path, &settings)?;
    }
    *current = settings.clone();
    info!(?settings, "Settings updated");
    Ok(settings)
}

// ─── 8b. Startup notices, folder check, diagnostics, logs ───
/// Messages produced during startup (e.g. a corrupt data file was reset). Each is returned
/// once, then cleared.
#[tauri::command]
pub fn take_startup_notices(state: State<'_, AppState>) -> Vec<String> {
    state.take_notices()
}

/// Whether `path` is an existing directory (the Settings page warns when the default
/// download folder, e.g. on an unplugged drive, is gone).
#[tauri::command]
pub async fn folder_exists(path: String) -> bool {
    let path = path.trim().to_string();
    tokio::task::spawn_blocking(move || Path::new(&path).is_dir())
        .await
        .unwrap_or(false)
}

/// Support information with no paths, user names, URLs or full IP addresses.
#[tauri::command]
pub async fn get_diagnostics(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::Diagnostics, String> {
    let settings = state.settings.read().await.clone();
    let adapters = state.last_adapters.read().await.clone();
    let infos = state.adapter_infos(adapters);
    let recent = match crate::logging::log_dir() {
        Some(dir) => {
            let dir = dir.to_path_buf();
            tokio::task::spawn_blocking(move || crate::logging::read_recent_errors(&dir, 20))
                .await
                .unwrap_or_default()
        }
        None => Vec::new(),
    };
    Ok(crate::diagnostics::build(
        &settings,
        &infos,
        tauri::webview_version().ok(),
        recent,
    ))
}

/// Opens the folder holding the log files in the file manager.
#[tauri::command]
pub fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    let dir = match crate::logging::log_dir() {
        Some(dir) => dir.to_path_buf(),
        None => tauri::Manager::path(&app)
            .app_log_dir()
            .map_err(|e| format!("No log folder: {e}"))?,
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create log folder: {e}"))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Failed to open log folder: {e}"))
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
    let (adapter_tx, adapter_rx) = if probe.supports_ranges && probe.total_bytes > 0 {
        let (tx, rx) = mpsc::channel(64);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    let names = Arc::new(RwLock::new(adapter_names(&adapters)));
    // Hold the handles lock across spawn + insert: the task removes its own handle when it
    // finishes, and that removal must not run before the handle has been inserted.
    let mut handles = shared.handles.lock().await;
    // A pause or remove between marking the task Downloading and this point found no handle,
    // and (under this same lock) paused or removed it. Starting the engine now would leave
    // a download running that nothing can stop.
    let still_wanted = shared
        .tasks
        .read()
        .await
        .get(&task_id)
        .is_some_and(|t| t.status == TaskStatus::Downloading);
    if !still_wanted {
        info!(task_id = %task_id, "Task was paused or removed before it started; not starting the engine");
        let removed = !shared.tasks.read().await.contains_key(&task_id);
        drop(handles);
        // `remove_download` skipped the file because it was still reserved by this task.
        // A new download's placeholder is still empty and nobody else's: delete it.
        if removed && !resume {
            remove_empty_placeholder(&output_path).await;
        }
        shared.reserved_paths.lock().await.remove(&output_path);
        return;
    }
    let join_handle = tokio::spawn(run_download(
        shared.clone(),
        task_id.clone(),
        engine,
        probe,
        output_path.clone(),
        adapters,
        adapter_rx,
        names.clone(),
        cancel_rx,
        resume,
    ));
    handles.insert(
        task_id,
        TaskHandle {
            cancel_tx,
            adapter_tx,
            names,
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
    adapter_rx: Option<mpsc::Receiver<AdapterUpdate>>,
    names: Arc<RwLock<HashMap<std::net::IpAddr, (String, String)>>>,
    cancel_rx: watch::Receiver<bool>,
    resume: bool,
) {
    let (tx, mut rx) = mpsc::channel::<ProgressUpdate>(100);

    let mut forwarder = {
        let shared = shared.clone();
        let id = task_id.clone();
        let names = names.clone();
        tokio::spawn(async move {
            let mut last_save = Instant::now();
            while let Some(p) = rx.recv().await {
                let current_names = names.read().await;
                let stats = p
                    .adapters
                    .iter()
                    .map(|a| adapter_stat(a, &current_names))
                    .collect();
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
            .resume_with_updates(
                &probe,
                &output_path,
                &adapters,
                adapter_rx,
                Some(tx),
                cancel_rx.clone(),
            )
            .await
    } else {
        engine
            .download_with_updates(
                &probe,
                &output_path,
                &adapters,
                adapter_rx,
                Some(tx),
                cancel_rx.clone(),
            )
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
            #[cfg(windows)]
            write_mark_of_the_web(&output_path, &probe.url).await;
            (TaskStatus::Completed, Some(hash), None)
        }
        // Also treat any error after a stop request as a pause: tearing down sockets
        // mid-transfer can surface as an I/O error rather than `DownloadCancelled`.
        Err(e) if e.downcast_ref::<DownloadCancelled>().is_some() || stop_requested => {
            info!(task_id = %task_id, "Download paused by user");
            (TaskStatus::Paused, None, None)
        }
        Err(e) => {
            // The one place an engine error becomes task text (UI, downloads.json, toast):
            // reqwest embeds the full request URL, credentials and query included.
            let msg = redact_error(&format!("{e:#}"));
            error!(task_id = %task_id, "Download failed: {msg}");
            (TaskStatus::Error, None, Some(msg))
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
                redact_error(task.error.as_deref().unwrap_or("unknown error"))
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
    let mut handles = shared.handles.lock().await;
    let Some(TaskHandle {
        cancel_tx,
        mut join_handle,
        output_path,
        ..
    }) = handles.remove(task_id)
    else {
        // No engine yet: a start or resume may be about to spawn one. Pausing while still
        // holding `handles` guarantees `spawn_task` sees the new status and stands down.
        let snapshot = pause_if_downloading(shared, task_id).await;
        drop(handles);
        return snapshot;
    };
    // Release the lock before awaiting: the task itself locks `handles` during cleanup.
    drop(handles);

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

    // The task records its own final status unless it was aborted.
    pause_if_downloading(shared, task_id).await
}

/// Marks a `Downloading` task `Paused` and returns its snapshot; `None` if the id is unknown.
async fn pause_if_downloading(shared: &Shared, task_id: &str) -> Option<DownloadTaskState> {
    let current = shared.tasks.read().await.get(task_id).cloned()?;
    if current.status != TaskStatus::Downloading {
        return Some(current);
    }
    shared
        .update(task_id, |task| {
            if task.status == TaskStatus::Downloading {
                task.status = TaskStatus::Paused;
                task.clear_runtime();
            }
        })
        .await
}

/// Dynamically aggregates newly joined network adapters into active downloads,
/// and retires workers for disconnected adapters.
pub async fn handle_network_change(
    state: &AppState,
    added: &[NetworkAdapter],
    removed: &[NetworkAdapter],
) {
    let targets = {
        let handles = state.handles.lock().await;
        let tasks = state.tasks.read().await;
        handles
            .iter()
            .filter_map(|(task_id, handle)| {
                let task = tasks.get(task_id)?;
                if task.status != TaskStatus::Downloading {
                    return None;
                }
                Some((
                    task_id.clone(),
                    handle.adapter_tx.clone(),
                    handle.names.clone(),
                ))
            })
            .collect::<Vec<_>>()
    };

    let updates = crate::adapter_updates::adapter_updates(added, removed);
    for (task_id, tx, names) in targets {
        let Some(tx) = tx else { continue };
        for new_adapter in added {
            names.write().await.insert(
                new_adapter.ip,
                (
                    new_adapter.id.clone(),
                    crate::adapters::adapter_name(new_adapter),
                ),
            );
            info!(
                task_id = %task_id,
                adapter = %new_adapter.id,
                "Auto-aggregating active adapter into download"
            );
        }
        for drop_adapter in removed {
            info!(
                task_id = %task_id,
                adapter = %drop_adapter.id,
                "Retiring disabled/disconnected adapter from download"
            );
        }
        // `refresh_adapters` has already recorded the new adapter list, so nothing would ever
        // re-send a dropped update: delivery must not be lossy. A full queue (engine busy)
        // is waited out, but off this task, so the watcher / UI call holding the
        // `last_adapters` lock never blocks. One task per download keeps add/remove order.
        tokio::spawn(crate::adapter_updates::deliver(
            task_id,
            tx,
            updates.clone(),
            crate::adapter_updates::SEND_TIMEOUT,
        ));
    }
}

/// Marks a network adapter active or disabled globally for downloads.
#[tauri::command]
pub async fn set_adapter_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<Vec<AdapterInfo>, String> {
    // Same lock as `refresh_adapters`, taken before discovering, so this snapshot cannot be
    // overwritten by (or overwrite) a concurrent refresh's. Order: last_adapters -> settings.
    let mut last = state.last_adapters.write().await;
    let discovered = core_discover().map_err(|e| redact_error(&format!("{e:#}")))?;
    // Overrides are keyed by interface name so they survive DHCP address changes.
    let key = crate::adapters::override_key(&id, &discovered);
    {
        let mut current = state.settings.write().await;
        let mut next = current.clone();
        set_override(&mut next.adapter_overrides, &key, enabled);
        if let Some(path) = &state.settings_path {
            if let Err(e) = settings::save(path, &next) {
                warn!("Failed to save settings with updated adapter override: {e:#}");
            }
        }
        *current = next;
    }
    info!(id = %id, name = %key, enabled, "Adapter override set");

    // OPEN DESIGN QUESTION: disabling the LAST enabled adapter here is accepted, and
    // `handle_network_change` then retires its workers from running downloads. Per
    // `check_adapter_selection`, start/resume would refuse in that state, but a download that
    // is already running instead continues over the OS default route (the engine falls back
    // to unbound sockets) and so ignores the user's choice. Behaviour intentionally
    // unchanged; whether to pause such downloads or refuse the toggle is the user's call.
    let adapters = refresh_locked(&state, &mut last, discovered, AddPolicy::Always).await;
    drop(last);
    let infos: Vec<AdapterInfo> = state.adapter_infos(adapters);
    let _ = app.emit("network-adapters-changed", &infos);
    Ok(infos)
}

// ─── Helpers ────────────────────────────────────────────────
/// Stores `enabled` under the interface name `key`, dropping any legacy `"<key>:<ip>"` keys
/// so they cannot linger and disagree.
fn set_override(overrides: &mut HashMap<String, bool>, key: &str, enabled: bool) {
    overrides
        .retain(|k, _| crate::adapters::split_adapter_id(k).is_none_or(|(name, _)| name != key));
    overrides.insert(key.to_string(), enabled);
}

fn engine_from_settings(settings: &Settings) -> Result<DownloadEngine, String> {
    DownloadEngine::new(
        settings.chunk_size_bytes(),
        settings.connections_per_adapter as usize,
    )
    .map_err(|e| format!("Invalid engine settings: {e:#}"))
}

pub const NO_ADAPTERS_ENABLED: &str =
    "No network adapters are enabled. Enable one on the Network page.";

/// Discovers adapters and applies app-level user overrides from settings.
fn select_adapters(settings: &Settings) -> Result<Vec<NetworkAdapter>, String> {
    let mut adapters = core_discover().map_err(|e| format!("Adapter discovery failed: {e:#}"))?;
    for adapter in adapters.iter_mut() {
        crate::adapters::apply_overrides(adapter, &settings.adapter_overrides);
    }
    check_adapter_selection(&adapters)?;
    Ok(adapters)
}

/// Refuses to start when the user has disabled every usable adapter: silently downloading
/// over the OS default route would ignore their choice. With no usable adapter at all (e.g.
/// an IPv6-only host) there is nothing to choose, so the engine's default route is allowed.
///
/// OPEN DESIGN QUESTION: this only guards start/resume. Disabling the last enabled adapter
/// while a download is running (`set_adapter_enabled`) is not refused and leaves that download
/// running over the default route; see the note there.
fn check_adapter_selection(adapters: &[NetworkAdapter]) -> Result<(), String> {
    let any_usable = adapters.iter().any(crate::adapters::is_usable);
    let any_enabled = adapters.iter().any(|a| a.enabled);
    if any_usable && !any_enabled {
        return Err(NO_ADAPTERS_ENABLED.to_string());
    }
    Ok(())
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

/// Picks `dir/filename` or the first free `dir/"name (n).ext"`, claims it on disk by creating
/// an empty placeholder file, and reserves it for this process.
///
/// The claim uses `create_new`, so it is atomic across processes (e.g. the desktop app and
/// the CLI can never pick the same file). This mirrors `conflux_core::claim_unique_path`,
/// but additionally skips paths reserved by running tasks: a resumed task's file may be
/// missing on disk while its engine is about to recreate it.
async fn reserve_output_path(
    reserved: &Mutex<HashSet<PathBuf>>,
    dir: &Path,
    filename: &str,
) -> Result<PathBuf, String> {
    let mut reserved = reserved.lock().await;
    let mut n: u32 = 0;
    loop {
        let candidate = if n == 0 {
            dir.join(filename)
        } else {
            dir.join(numbered_filename(filename, n))
        };
        n += 1;
        if reserved.contains(&candidate) {
            continue;
        }
        let created = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate);
        match created {
            Ok(_) => {
                reserved.insert(candidate.clone());
                return Ok(candidate);
            }
            // Taken on disk (possibly a moment ago by another process): try the next name.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!("Cannot create {}: {e}", candidate.display()));
            }
        }
    }
}

/// Contents of the `Zone.Identifier` stream marking a file as downloaded from the Internet
/// (zone 3), so SmartScreen and Office Protected View treat it as untrusted. Credentials in
/// the URL are dropped and CR/LF removed so the URL cannot inject extra lines.
#[cfg_attr(not(windows), allow(dead_code))]
fn zone_identifier_contents(url: &str) -> String {
    let mut host_url: String = url.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    if let Some(scheme_end) = host_url.find("://") {
        let authority_start = scheme_end + 3;
        let authority_end = host_url[authority_start..]
            .find(['/', '?', '#'])
            .map_or(host_url.len(), |i| authority_start + i);
        if let Some(at) = host_url[authority_start..authority_end].rfind('@') {
            host_url.replace_range(authority_start..authority_start + at + 1, "");
        }
    }
    format!("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={host_url}\r\n")
}

/// Writes Mark-of-the-Web (`<path>:Zone.Identifier` NTFS alternate data stream). Best effort:
/// fails on non-NTFS volumes (e.g. FAT32 USB drives), which is only logged.
#[cfg(windows)]
async fn write_mark_of_the_web(path: &Path, url: &str) {
    let mut stream = path.as_os_str().to_os_string();
    stream.push(":Zone.Identifier");
    match tokio::fs::write(&stream, zone_identifier_contents(url)).await {
        Ok(()) => info!(path = %path.display(), "Wrote Mark-of-the-Web"),
        Err(e) => warn!(path = %path.display(), "Failed to write Mark-of-the-Web: {e}"),
    }
}

/// Deletes the file at `path` only if it is empty (an unused placeholder from
/// `reserve_output_path`). Best effort.
async fn remove_empty_placeholder(path: &Path) {
    let is_empty = tokio::fs::metadata(path)
        .await
        .is_ok_and(|m| m.is_file() && m.len() == 0);
    if !is_empty {
        return;
    }
    if let Err(e) = tokio::fs::remove_file(path).await {
        warn!(path = %path.display(), "Failed to delete unused placeholder file: {e}");
    }
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

        let a = reserve_output_path(&reserved, &dir, "file.bin")
            .await
            .unwrap();
        let b = reserve_output_path(&reserved, &dir, "file.bin")
            .await
            .unwrap();
        let c = reserve_output_path(&reserved, &dir, "file.bin")
            .await
            .unwrap();

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
        let d = reserve_output_path(&reserved, &dir, "other.bin")
            .await
            .unwrap();
        assert_eq!(d, dir.join("other (1).bin"));
        assert_eq!(std::fs::read(dir.join("other.bin")).unwrap(), b"keep");

        // A reserved path missing on disk (resumed task) is skipped too.
        let resumed = dir.join("resumed.bin");
        reserved.lock().await.insert(resumed.clone());
        let e = reserve_output_path(&reserved, &dir, "resumed.bin")
            .await
            .unwrap();
        assert_eq!(e, dir.join("resumed (1).bin"));
        assert!(!resumed.exists());

        assert!(
            reserve_output_path(&reserved, &dir.join("missing"), "x.bin")
                .await
                .is_err()
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn task(supports_ranges: bool, total_bytes: u64) -> DownloadTaskState {
        DownloadTaskState {
            id: "t".into(),
            url: "https://example.com/f".into(),
            filename: "f".into(),
            save_dir: "d".into(),
            save_path: "d/f".into(),
            adapter_ids: vec![],
            total_bytes,
            supports_ranges,
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
        }
    }

    #[test]
    fn partial_file_deleted_only_with_evidence() {
        let chunked = task(true, 1000);
        // No file: nothing to delete.
        assert!(!partial_file_is_ours(&chunked, None, true));
        // Resume sidecar present.
        assert!(partial_file_is_ours(&chunked, Some(123), true));
        // Pre-allocated to exactly the download size.
        assert!(partial_file_is_ours(&chunked, Some(1000), false));
        // Empty placeholder.
        assert!(partial_file_is_ours(&chunked, Some(0), false));
        // Some other file now sits at the path.
        assert!(!partial_file_is_ours(&chunked, Some(999), false));
        assert!(!partial_file_is_ours(&chunked, Some(5000), false));

        // Streaming downloads are not pre-allocated: length alone is no evidence.
        let streaming = task(false, 1000);
        assert!(!partial_file_is_ours(&streaming, Some(1000), false));
        assert!(partial_file_is_ours(&streaming, Some(1000), true));
        let unknown_size = task(true, 0);
        assert!(!partial_file_is_ours(&unknown_size, Some(42), false));
        assert!(partial_file_is_ours(&unknown_size, Some(0), false));
    }

    fn adapter(name: &str, ip: &str, enabled: bool) -> NetworkAdapter {
        let ip: std::net::IpAddr = ip.parse().unwrap();
        NetworkAdapter {
            id: format!("{name}:{ip}"),
            name: name.into(),
            ip,
            is_ipv4: ip.is_ipv4(),
            is_loopback: false,
            enabled,
        }
    }

    #[test]
    fn refuses_when_every_usable_adapter_is_disabled() {
        let wifi_off = adapter("Wi-Fi", "192.168.1.5", false);
        let eth_on = adapter("Ethernet", "10.0.0.2", true);
        let v6 = adapter("Ethernet", "fe80::1", false);
        assert_eq!(
            check_adapter_selection(&[wifi_off.clone(), v6.clone()]),
            Err(NO_ADAPTERS_ENABLED.to_string())
        );
        assert!(check_adapter_selection(&[wifi_off, eth_on]).is_ok());
        // Nothing usable at all: the engine's default route is allowed.
        assert!(check_adapter_selection(&[v6]).is_ok());
        assert!(check_adapter_selection(&[]).is_ok());
    }

    #[test]
    fn set_override_replaces_legacy_keys_for_the_name() {
        let mut overrides: HashMap<String, bool> = [
            ("Wi-Fi:192.168.1.5", true),
            ("Wi-Fi:fe80::1", true),
            ("Wi-Fi 2:10.0.0.1", true),
            ("Ethernet", false),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        set_override(&mut overrides, "Wi-Fi", false);
        let mut keys: Vec<&str> = overrides.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["Ethernet", "Wi-Fi", "Wi-Fi 2:10.0.0.1"]);
        assert_eq!(overrides.get("Wi-Fi"), Some(&false));
    }

    #[test]
    fn zone_identifier_marks_internet_zone() {
        assert_eq!(
            zone_identifier_contents("https://example.com/a.exe"),
            "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/a.exe\r\n"
        );
        assert_eq!(
            zone_identifier_contents("https://user:pw@example.com/a.exe?x=a@b"),
            "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/a.exe?x=a@b\r\n"
        );
        assert_eq!(
            zone_identifier_contents("https://example.com/a\r\nZoneId=0"),
            "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/aZoneId=0\r\n"
        );
    }
}
