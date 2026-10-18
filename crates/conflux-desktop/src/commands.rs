use crate::adapters::{adapter_names, adapter_stat, AdapterInfo};
use crate::browser_bridge::ExternalDownloadPayload;
use crate::controller::{
    now_ms, remove_empty_placeholder, CommitError, ResumeStart, TaskController, TaskEvents,
    WorkerChannels, WorkerExit,
};
use crate::redact::redact_error;
use crate::settings::{self, Backdrop, Settings};
use crate::state::{AppState, DownloadTaskState, TaskStatus};
use conflux_core::{
    discover_adapters as core_discover, sanitize_filename, AdapterUpdate, DownloadCancelled,
    DownloadEngine, DownloadProbe, NetworkAdapter, ProgressUpdate, RequestHeaders,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Event carrying a full `DownloadTaskState` snapshot after every state change.
pub const PROGRESS_EVENT: &str = "download-progress";

/// Event carrying the download-history save failure (`null` once saving works again).
pub const HISTORY_STATUS_EVENT: &str = "history-save-status";

/// How long to wait for the progress forwarder to drain after the engine returns.
const FORWARDER_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Minimum interval between history save requests caused by one task's progress ticks.
/// The history writer additionally coalesces requests across all tasks.
const HISTORY_SAVE_INTERVAL: Duration = crate::history::PROGRESS_SAVE_INTERVAL;

/// Forwards controller events to the webview, the tray and notifications.
pub struct AppEvents(pub AppHandle);

impl TaskEvents for AppEvents {
    fn task_changed(&self, task: &DownloadTaskState, tasks: &HashMap<String, DownloadTaskState>) {
        emit(&self.0, task);
        crate::tray::update_tray_tooltip(&self.0, tasks);
    }

    fn task_removed(&self, tasks: &HashMap<String, DownloadTaskState>) {
        crate::tray::update_tray_tooltip(&self.0, tasks);
    }

    fn worker_finished(&self, task: &DownloadTaskState) {
        let app = self.0.clone();
        let task = task.clone();
        tauri::async_runtime::spawn(async move { notify_finished(&app, &task).await });
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
pub async fn probe_url(
    url: String,
    headers: Option<RequestHeaders>,
) -> Result<ProbeResult, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("Download URL is empty".to_string());
    }
    let probe = DownloadEngine::default()
        .probe_with_headers(&url, headers)
        .await
        .map_err(|e| redact_error(&format!("{e:#}")))?;
    let supports_ranges = probe.supports_parallel();
    Ok(ProbeResult {
        url: probe.url,
        filename: probe.suggested_filename,
        total_bytes: probe.total_bytes,
        supports_ranges,
    })
}

// ─── 3. Start Download ─────────────────────────────────────
#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    url: String,
    save_dir: String,
    filename: Option<String>,
    headers: Option<RequestHeaders>,
    handoff_id: Option<String>,
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
        .probe_with_headers(&url, headers)
        .await
        .map_err(|e| format!("Probe failed: {}", redact_error(&format!("{e:#}"))))?;

    let requested_name = filename
        .map(|f| sanitize_filename(&f))
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| probe.suggested_filename.clone());
    let (output_path, file_identity) = state
        .tasks
        .reserve_new_output(&dir, &requested_name)
        .await?;
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
        file_identity: file_identity.clone(),
        adapter_ids: vec![],
        total_bytes: probe.total_bytes,
        supports_ranges: probe.supports_parallel(),
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
        duration_ms: 0,
        avg_speed_bytes_sec: None,
        adapters: Vec::new(),
        chunk_map: None,
        lifecycle: Default::default(),
    };
    let generation = state.tasks.insert_new(task).await;
    state.tasks.save_history();
    let started = launch(
        &state,
        &task_id,
        generation,
        engine,
        probe,
        output_path,
        file_identity.clone(),
        file_identity,
        false,
    )
    .await;
    let task = state
        .tasks
        .get(&task_id)
        .await
        .filter(|task| task.status == TaskStatus::Completed || started)
        .ok_or_else(|| {
            "Download was not started; the browser transfer was preserved".to_string()
        })?;
    if let Some(id) = handoff_id {
        crate::browser_bridge::acknowledge_handoff(&id, true)
            .map_err(|_| "Download started, but browser acknowledgement failed".to_string())?;
    }
    Ok(task)
}

// ─── 4. Pause ───────────────────────────────────────────────
/// Stops the download, keeping its partial file and resume sidecar. Returns the resulting
/// snapshot: `paused`, `completed` if it finished before the stop took effect, or `stopping`
/// if the engine has not exited within the stop timeout.
#[tauri::command]
pub async fn pause_download(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<DownloadTaskState, String> {
    state
        .tasks
        .stop(&task_id)
        .await
        .ok_or_else(|| format!("Unknown task: {task_id}"))
}

/// Resumes a paused or failed task. Every await between checking the task and starting its
/// engine (the probe, file checks) is guarded by a [`crate::controller::ResumeTicket`]: if
/// anything else happened to the task meanwhile, this resume stands down.
pub async fn resume_task_internal(
    state: &AppState,
    task_id: &str,
) -> Result<DownloadTaskState, String> {
    let ticket = match state.tasks.begin_resume(task_id).await? {
        ResumeStart::Ready(ticket) => ticket,
        ResumeStart::AlreadyRunning(task) => return Ok(task),
    };
    let task = ticket.task.clone();

    let dir = validate_save_dir(&task.save_dir)?;
    let settings = state.settings.read().await.clone();
    select_adapters(&settings)?;
    let engine = engine_from_settings(&settings)?;
    let output_path = PathBuf::from(&task.save_path);
    if output_path.exists()
        && task.file_identity.as_ref().is_none_or(|identity| {
            conflux_core::FileIdentity::at(&output_path).as_ref().ok() != Some(identity)
        })
    {
        return Err("The saved file's ownership cannot be verified. It was preserved; start a new download to a new filename.".into());
    }
    let headers = conflux_core::read_resume_headers(&output_path, &task.url)
        .ok()
        .flatten();
    let probe = engine
        .probe_with_headers(&task.url, headers)
        .await
        .map_err(|e| format!("Probe failed: {}", redact_error(&format!("{e:#}"))))?;

    if output_path.parent() != Some(dir.as_path()) {
        warn!(task_id = %task_id, "Saved path is outside the saved folder; resuming anyway");
    }
    if !state.tasks.reserve_path(&output_path).await {
        return Err(format!(
            "{} is being written by another download",
            task.save_path
        ));
    }
    let (resume_existing, file_identity) =
        match prepare_resume_output(&output_path, task.file_identity.as_ref()) {
            Ok(output) => output,
            Err(error) => {
                state.tasks.release_path(&output_path).await;
                return Err(error);
            }
        };
    // A placeholder this resume created for a missing file; deleted if we stand down.
    let placeholder = (!resume_existing).then(|| file_identity.clone()).flatten();
    let total_bytes = probe.total_bytes;
    let supports_ranges = probe.supports_parallel();
    let committed = state
        .tasks
        .commit_resume(ticket, |t| {
            t.total_bytes = total_bytes;
            t.supports_ranges = supports_ranges;
            t.file_identity = file_identity.clone();
            t.error = None;
            t.sha256 = None;
            t.completed_at_ms = None;
        })
        .await;
    let (snapshot, generation) = match committed {
        Ok(committed) => committed,
        Err(stale) => {
            remove_empty_placeholder(&output_path, placeholder).await;
            state.tasks.release_path(&output_path).await;
            return match stale {
                CommitError::Removed => Err(format!("Unknown task: {task_id}")),
                // Paused, resumed or finished by someone else meanwhile: report that state.
                CommitError::Superseded(current) => {
                    info!(task_id = %task_id, status = ?current.status, "Resume superseded by a newer change; not starting");
                    Ok(*current)
                }
            };
        }
    };
    info!(task_id = %task_id, url = %probe.url, output = %task.save_path, "Resuming download");
    state.tasks.save_history();
    launch(
        state,
        task_id,
        generation,
        engine,
        probe,
        output_path,
        snapshot.file_identity.clone(),
        placeholder,
        resume_existing,
    )
    .await;
    Ok(state.tasks.get(task_id).await.unwrap_or(snapshot))
}

/// Pauses every running download (concurrently, bounded by one stop timeout in total) and
/// waits for the history write.
pub async fn pause_all_internal(state: &AppState) {
    state.tasks.stop_all().await;
}

pub async fn resume_all_internal(state: &AppState) {
    let paused_ids = state
        .tasks
        .ids_with_status(&[TaskStatus::Paused, TaskStatus::Error])
        .await;
    for id in paused_ids {
        if let Err(e) = resume_task_internal(state, &id).await {
            warn!(task_id = %id, "Failed to resume task in resume_all: {e}");
        }
    }
}

// ─── 5. Resume ──────────────────────────────────────────────
/// Continues a paused or failed download from its completed chunks (or from zero if the
/// engine finds its resume data unusable).
#[tauri::command]
pub async fn resume_download(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<DownloadTaskState, String> {
    resume_task_internal(&state, &task_id).await
}

#[tauri::command]
pub async fn pause_all(state: State<'_, AppState>) -> Result<(), String> {
    pause_all_internal(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn resume_all(state: State<'_, AppState>) -> Result<(), String> {
    resume_all_internal(&state).await;
    Ok(())
}

// ─── 6. Remove ──────────────────────────────────────────────
/// Stops the task if running and removes it from the list. An incomplete download's resume
/// data and partial file are deleted only after verifying file identity. A completed
/// file is deleted only if `delete_file`, with the same ownership check. Fails (keeping the
/// task) while its engine is still stopping.
#[tauri::command]
pub async fn remove_download(
    state: State<'_, AppState>,
    task_id: String,
    delete_file: bool,
) -> Result<(), String> {
    state.tasks.remove(&task_id, delete_file).await
}

// ─── 7. List Tasks ──────────────────────────────────────────
/// All known tasks, newest first.
#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<DownloadTaskState>, String> {
    Ok(state.tasks.list().await)
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

/// Returns any pending external download request from the browser extension or CLI (on cold start).
#[tauri::command]
pub fn take_pending_download(state: State<'_, AppState>) -> Option<ExternalDownloadPayload> {
    state.take_pending_download()
}

#[tauri::command]
pub fn dismiss_external_download(handoff_id: String) -> Result<(), String> {
    crate::browser_bridge::acknowledge_handoff(&handoff_id, false)
        .map(|_| ())
        .map_err(|_| "Could not dismiss browser handoff".into())
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct BrowserIntegrationStatus {
    pub registered: bool,
}

/// Checks whether Conflux is registered as a native messaging host in the OS.
#[tauri::command]
pub async fn get_browser_integration_status() -> BrowserIntegrationStatus {
    tokio::task::spawn_blocking(move || BrowserIntegrationStatus {
        registered: crate::browser_bridge::is_browser_integration_registered(),
    })
    .await
    .unwrap_or(BrowserIntegrationStatus { registered: false })
}

/// Registers the native messaging hosts and conflux:// protocol handler in HKCU.
#[tauri::command]
pub async fn register_browser_extension() -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        crate::browser_bridge::register_browser_integration().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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

// ─── 10. Window Theme / Backdrop ────────────────────────────
#[derive(Debug, Clone, Serialize)]
pub struct WindowBackdrop {
    /// The material actually applied; `None` means the UI must paint opaque surfaces.
    pub backdrop: Backdrop,
}

/// Sets the window frame to `dark` and applies the `requested` material, falling back to
/// weaker ones where Windows lacks it: Acrylic and Mica Alt need Windows 11 22H2+, Mica
/// needs Windows 11, and Windows 10 gets no material.
#[tauri::command]
pub fn apply_window_theme(window: WebviewWindow, dark: bool, backdrop: Backdrop) -> WindowBackdrop {
    let theme = if dark {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    if let Err(e) = window.set_theme(Some(theme)) {
        warn!("Failed to set window theme: {e}");
    }
    let applied = apply_backdrop(&window, dark, backdrop);
    info!("Window backdrop: requested {backdrop:?}, applied {applied:?}");
    WindowBackdrop { backdrop: applied }
}

fn apply_backdrop(window: &WebviewWindow, dark: bool, requested: Backdrop) -> Backdrop {
    // Reset to no material first. `clear_tabbed` only succeeds where the system backdrop
    // API (DWMWA_SYSTEMBACKDROP_TYPE, Windows 11 22H2+) exists, so it doubles as the probe.
    // Without that API `apply_acrylic` would use the legacy blur that lags while dragging.
    let system_backdrop = window_vibrancy::clear_tabbed(window).is_ok();
    let _ = window_vibrancy::clear_mica(window);

    if system_backdrop {
        let result = match requested {
            Backdrop::None => return Backdrop::None,
            Backdrop::Acrylic => window_vibrancy::apply_acrylic(window, None),
            Backdrop::MicaAlt => window_vibrancy::apply_tabbed(window, Some(dark)),
            Backdrop::Mica => window_vibrancy::apply_mica(window, Some(dark)),
        };
        match result {
            Ok(()) => return requested,
            Err(e) => info!("{requested:?} backdrop unavailable: {e}"),
        }
    }
    if requested == Backdrop::None {
        return Backdrop::None;
    }
    // Windows 11 21H2 has only the original Mica; Windows 10 has none.
    match window_vibrancy::apply_mica(window, Some(dark)) {
        Ok(()) => Backdrop::Mica,
        Err(e) => {
            info!("Mica backdrop unavailable: {e}");
            let _ = window_vibrancy::clear_mica(window);
            Backdrop::None
        }
    }
}

// ─── Running a task ─────────────────────────────────────────
/// Chooses the adapters and hands the engine to the controller, which starts it only if the
/// task is still in `generation`. `output_path` must already be reserved. `expected_identity`
/// is the file the engine may write (checked on its handle before any truncation);
/// `placeholder` is an empty file this start created, deleted if the task stands down.
#[allow(clippy::too_many_arguments)]
async fn launch(
    state: &AppState,
    task_id: &str,
    generation: u64,
    engine: DownloadEngine,
    probe: DownloadProbe,
    output_path: PathBuf,
    expected_identity: Option<conflux_core::FileIdentity>,
    placeholder: Option<conflux_core::FileIdentity>,
    resume: bool,
) -> bool {
    let controller = &state.tasks;
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (adapter_tx, adapter_rx) = if probe.supports_parallel() {
        let (tx, rx) = mpsc::channel(64);
        (
            Some(crate::adapter_updates::ordered_sender(
                task_id.to_string(),
                tx,
            )),
            Some(rx),
        )
    } else {
        (None, None)
    };
    // Held until the handle is registered, so an adapter change in between is not missed.
    let _adapter_snapshot = state.last_adapters.read().await;
    let adapters = match select_adapters(&*state.settings.read().await) {
        Ok(adapters) => adapters,
        Err(message) => {
            // Not started: record why through the normal exit path.
            let names = Arc::new(RwLock::new(HashMap::new()));
            let channels = WorkerChannels {
                cancel_tx,
                adapter_tx,
                names,
            };
            let exit = WorkerExit {
                status: TaskStatus::Error,
                error: Some(message),
                ..WorkerExit::paused()
            };
            controller
                .spawn_worker(
                    task_id,
                    generation,
                    output_path,
                    placeholder,
                    channels,
                    async { exit },
                )
                .await;
            return false;
        }
    };
    let names = Arc::new(RwLock::new(adapter_names(&adapters)));
    let worker = run_download(
        Arc::clone(controller),
        task_id.to_string(),
        engine,
        probe,
        output_path.clone(),
        expected_identity,
        adapters,
        adapter_rx,
        names.clone(),
        cancel_rx,
        resume,
    );
    let channels = WorkerChannels {
        cancel_tx,
        adapter_tx,
        names,
    };
    controller
        .spawn_worker(
            task_id,
            generation,
            output_path,
            placeholder,
            channels,
            worker,
        )
        .await
}

/// The engine run of one task, with its progress forwarded to the controller. Returns how
/// it ended; the controller records that and releases the task's handle and path.
#[allow(clippy::too_many_arguments)]
async fn run_download(
    controller: Arc<TaskController>,
    task_id: String,
    engine: DownloadEngine,
    probe: DownloadProbe,
    output_path: PathBuf,
    expected_identity: Option<conflux_core::FileIdentity>,
    adapters: Vec<NetworkAdapter>,
    adapter_rx: Option<mpsc::Receiver<AdapterUpdate>>,
    names: Arc<RwLock<HashMap<std::net::IpAddr, (String, String)>>>,
    cancel_rx: watch::Receiver<bool>,
    resume: bool,
) -> WorkerExit {
    let (tx, mut rx) = mpsc::channel::<ProgressUpdate>(100);
    let session_start = Instant::now();
    let initial_duration_ms = controller
        .get(&task_id)
        .await
        .map(|t| t.duration_ms)
        .unwrap_or(0);

    let mut forwarder = {
        let controller = Arc::clone(&controller);
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
                drop(current_names);
                let elapsed_ms = initial_duration_ms + session_start.elapsed().as_millis() as u64;
                let exists = controller
                    .update_running(&id, |task| {
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
                        task.duration_ms = elapsed_ms;
                        if elapsed_ms > 0 && task.downloaded_bytes > 0 {
                            task.avg_speed_bytes_sec =
                                Some((task.downloaded_bytes as f64) / (elapsed_ms as f64 / 1000.0));
                        }
                        if let Some(map) = p.chunk_map {
                            task.chunk_map = Some(map);
                        }
                    })
                    .await;
                if exists && last_save.elapsed() >= HISTORY_SAVE_INTERVAL {
                    last_save = Instant::now();
                    controller.save_progress();
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
                expected_identity,
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
                expected_identity,
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
    let (status, sha256, error) = match result {
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

    let completed_length = if status == TaskStatus::Completed {
        tokio::fs::metadata(&output_path)
            .await
            .ok()
            .map(|metadata| metadata.len())
    } else {
        None
    };
    WorkerExit {
        status,
        sha256,
        error,
        completed_length,
        duration_ms: Some(initial_duration_ms + session_start.elapsed().as_millis() as u64),
    }
}

async fn notify_finished(app: &AppHandle, task: &DownloadTaskState) {
    let state = app.state::<AppState>();
    if !state.settings.read().await.notify_on_complete {
        return;
    }
    let (title, body) = match task.status {
        TaskStatus::Completed => {
            let detail = match (task.duration_ms, task.avg_speed_bytes_sec) {
                (ms, Some(speed)) if ms > 0 => {
                    let secs = ms as f64 / 1000.0;
                    let speed_mb = speed / (1024.0 * 1024.0);
                    format!("{} ({:.1}s, avg {:.2} MB/s)", task.filename, secs, speed_mb)
                }
                _ => task.filename.clone(),
            };
            ("Download complete", detail)
        }
        TaskStatus::Error => (
            "Download failed",
            format!(
                "{}: {}",
                task.filename,
                redact_error(task.error.as_deref().unwrap_or("unknown error"))
            ),
        ),
        TaskStatus::Paused | TaskStatus::Downloading | TaskStatus::Stopping => return,
    };
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        warn!(task_id = %task.id, "Failed to show notification: {e}");
    }
}

/// Dynamically aggregates newly joined network adapters into active downloads,
/// and retires workers for disconnected adapters.
pub async fn handle_network_change(
    state: &AppState,
    added: &[NetworkAdapter],
    removed: &[NetworkAdapter],
) {
    let targets = state.tasks.adapter_targets().await;

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
        let _ = tx.send(updates.clone());
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
    DownloadEngine::new(settings.connections_per_adapter as usize)
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

fn prepare_resume_output(
    path: &Path,
    expected: Option<&conflux_core::FileIdentity>,
) -> Result<(bool, Option<conflux_core::FileIdentity>), String> {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
    {
        Ok(file) => {
            let actual = conflux_core::FileIdentity::of(&file)
                .map_err(|_| "Cannot verify the saved file's ownership".to_string())?;
            if expected != Some(&actual) {
                return Err(
                    "The saved file was replaced or ownership is unknown; it was preserved".into(),
                );
            }
            Ok((true, Some(actual)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| format!("Cannot claim the missing output: {error}"))?;
            Ok((false, conflux_core::FileIdentity::of(&file).ok()))
        }
        Err(error) => Err(format!("Cannot open the saved output: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_claims_missing_files_and_preserves_replacements() {
        let directory = std::env::temp_dir().join(format!("conflux-resume-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("file.bin");
        let (existing, identity) = prepare_resume_output(&path, None).unwrap();
        assert!(!existing);
        assert!(identity.is_some());
        std::fs::write(&path, b"partial").unwrap();
        assert!(prepare_resume_output(&path, identity.as_ref()).unwrap().0);
        std::fs::rename(&path, directory.join("old.bin")).unwrap();
        std::fs::write(&path, b"replace").unwrap();
        assert!(prepare_resume_output(&path, identity.as_ref()).is_err());
        assert!(prepare_resume_output(&path, None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"replace");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn validate_save_dir_rejects_empty_and_relative() {
        assert!(validate_save_dir("").is_err());
        assert!(validate_save_dir("   ").is_err());
        assert!(validate_save_dir("Downloads").is_err());
        let tmp = std::env::temp_dir();
        assert_eq!(validate_save_dir(tmp.to_str().unwrap()).unwrap(), tmp);
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

    #[tokio::test]
    async fn test_browser_integration_status_smoke() {
        let status = get_browser_integration_status().await;
        // Just verify it returns a valid boolean without panicking
        let _ = status.registered;
    }
}
