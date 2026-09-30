use crate::state::{AppState, DownloadTaskState, TaskHandle};
use conflux_core::{discover_adapters as core_discover, DownloadEngine};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{ipc::Channel, State};
use tokio::sync::{mpsc, watch};
use tracing::{error, info};
use uuid::Uuid;

// ─── 1. Adapter Discovery ───────────────────────────────────
#[derive(Debug, Clone, Serialize)]
pub struct AdapterInfo {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub is_ipv4: bool,
    pub is_loopback: bool,
    pub enabled: bool,
}

#[tauri::command]
pub fn discover_adapters() -> Result<Vec<AdapterInfo>, String> {
    let adapters = core_discover().map_err(|e| e.to_string())?;
    Ok(adapters
        .into_iter()
        .map(|a| AdapterInfo {
            name: a.id.split(':').next().unwrap_or(&a.id).to_string(),
            id: a.id,
            ip: a.ip.to_string(),
            is_ipv4: a.is_ipv4,
            is_loopback: a.is_loopback,
            enabled: a.enabled,
        })
        .collect())
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
    let engine = DownloadEngine::default();
    let probe = engine.probe(&url).await.map_err(|e| e.to_string())?;
    Ok(ProbeResult {
        url: probe.url,
        filename: probe.suggested_filename,
        total_bytes: probe.total_bytes,
        supports_ranges: probe.supports_ranges,
    })
}

// ─── 3. Start Download ─────────────────────────────────────
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub task_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_sec: f64,
    pub eta_seconds: u64,
    pub active_chunks: usize,
    pub completed_chunks: usize,
    pub total_chunks: usize,
    pub status: String,
    pub sha256: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    url: String,
    save_dir: String,
    on_progress: Channel<ProgressEvent>,
) -> Result<String, String> {
    let engine = DownloadEngine::default();
    let probe = engine.probe(&url).await.map_err(|e| e.to_string())?;

    let task_id = Uuid::new_v4().to_string();
    let output_path = PathBuf::from(&save_dir).join(&probe.suggested_filename);
    let adapters = core_discover().map_err(|e| e.to_string())?;

    let task_state = DownloadTaskState {
        id: task_id.clone(),
        url: url.clone(),
        mirrors: vec![],
        filename: probe.suggested_filename.clone(),
        save_path: output_path.to_string_lossy().to_string(),
        total_bytes: probe.total_bytes,
        downloaded_bytes: 0,
        status: "downloading".to_string(),
        speed_bytes_sec: 0.0,
        eta_seconds: 0,
        sha256: None,
        error: None,
    };

    state
        .tasks
        .write()
        .await
        .insert(task_id.clone(), task_state);

    let (cancel_tx, _cancel_rx) = watch::channel(false);
    let tid = task_id.clone();
    let tasks = state.tasks.clone();

    let join_handle = tokio::spawn(async move {
        let (tx, mut rx) = mpsc::channel::<conflux_core::ProgressUpdate>(100);

        let tid_fwd = tid.clone();
        let tasks_fwd = tasks.clone();
        let on_progress_clone = on_progress.clone();

        let progress_forwarder = tokio::spawn(async move {
            while let Some(p) = rx.recv().await {
                let event = ProgressEvent {
                    task_id: tid_fwd.clone(),
                    downloaded_bytes: p.downloaded_bytes,
                    total_bytes: p.total_bytes,
                    speed_bytes_sec: p.speed_bytes_sec,
                    eta_seconds: p.eta_seconds,
                    active_chunks: p.active_chunks,
                    completed_chunks: p.completed_chunks,
                    total_chunks: p.total_chunks,
                    status: "downloading".to_string(),
                    sha256: None,
                    error: None,
                };
                let _ = on_progress_clone.send(event);

                if let Some(task) = tasks_fwd.write().await.get_mut(&tid_fwd) {
                    task.downloaded_bytes = p.downloaded_bytes;
                    task.speed_bytes_sec = p.speed_bytes_sec;
                    task.eta_seconds = p.eta_seconds;
                }
            }
        });

        let result = engine
            .download(&url, &output_path, &adapters, Some(tx))
            .await;

        let _ = progress_forwarder.await;

        match result {
            Ok(sha256) => {
                info!("Download completed: {}", tid);
                if let Some(task) = tasks.write().await.get_mut(&tid) {
                    task.status = "completed".to_string();
                    task.sha256 = Some(sha256.clone());
                    task.downloaded_bytes = task.total_bytes;
                    task.speed_bytes_sec = 0.0;
                }
                let _ = on_progress.send(ProgressEvent {
                    task_id: tid,
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    speed_bytes_sec: 0.0,
                    eta_seconds: 0,
                    active_chunks: 0,
                    completed_chunks: 0,
                    total_chunks: 0,
                    status: "completed".to_string(),
                    sha256: Some(sha256),
                    error: None,
                });
            }
            Err(e) => {
                error!("Download failed for {}: {}", tid, e);
                if let Some(task) = tasks.write().await.get_mut(&tid) {
                    task.status = "error".to_string();
                    task.error = Some(e.to_string());
                    task.speed_bytes_sec = 0.0;
                }
                let _ = on_progress.send(ProgressEvent {
                    task_id: tid,
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    speed_bytes_sec: 0.0,
                    eta_seconds: 0,
                    active_chunks: 0,
                    completed_chunks: 0,
                    total_chunks: 0,
                    status: "error".to_string(),
                    sha256: None,
                    error: Some(e.to_string()),
                });
            }
        }
    });

    state.handles.lock().await.insert(
        task_id.clone(),
        TaskHandle {
            cancel_tx,
            join_handle,
        },
    );

    Ok(task_id)
}

// ─── 4. Pause Download ──────────────────────────────────────
#[tauri::command]
pub async fn pause_download(state: State<'_, AppState>, task_id: String) -> Result<(), String> {
    if let Some(handle) = state.handles.lock().await.remove(&task_id) {
        let _ = handle.cancel_tx.send(true);
        handle.join_handle.abort();
    }
    if let Some(task) = state.tasks.write().await.get_mut(&task_id) {
        task.status = "paused".to_string();
        task.speed_bytes_sec = 0.0;
    }
    Ok(())
}

// ─── 5. Resume Download ─────────────────────────────────────
#[tauri::command]
pub async fn resume_download(_state: State<'_, AppState>, _task_id: String) -> Result<(), String> {
    Err("Resume not yet implemented. Please restart the download.".to_string())
}

// ─── 6. Cancel Download ─────────────────────────────────────
#[tauri::command]
pub async fn cancel_download(state: State<'_, AppState>, task_id: String) -> Result<(), String> {
    if let Some(handle) = state.handles.lock().await.remove(&task_id) {
        let _ = handle.cancel_tx.send(true);
        handle.join_handle.abort();
    }
    state.tasks.write().await.remove(&task_id);
    Ok(())
}
