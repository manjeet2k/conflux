use crate::commands;
use crate::state::{AppState, DownloadTaskState, TaskStatus};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::menu::{MenuBuilder, MenuEvent};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tracing::info;

pub const TRAY_ID: &str = "conflux-tray";

static LAST_TOOLTIP: Mutex<Option<String>> = Mutex::new(None);

pub fn setup_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let menu = MenuBuilder::new(app)
        .text("show", "Show Conflux")
        .separator()
        .text("add", "Add Download...")
        .separator()
        .text("pause_all", "Pause All")
        .text("resume_all", "Resume All")
        .separator()
        .text("settings", "Settings")
        .separator()
        .text("quit", "Quit Conflux")
        .build()?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Conflux")
        .show_menu_on_left_click(false)
        .menu(&menu)
        .on_menu_event(|app, event| {
            handle_menu_event(app, event);
        })
        .on_tray_icon_event(|tray, event| {
            handle_tray_event(tray, event);
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let tray = builder.build(app)?;
    info!("Conflux system tray icon initialized");
    Ok(tray)
}

pub fn format_tray_tooltip(tasks: &HashMap<String, DownloadTaskState>) -> String {
    let active: Vec<&DownloadTaskState> = tasks
        .values()
        .filter(|t| t.status == TaskStatus::Downloading)
        .collect();

    if !active.is_empty() {
        let total_downloaded: u64 = active.iter().map(|t| t.downloaded_bytes).sum();
        let total_bytes: u64 = active.iter().map(|t| t.total_bytes).sum();
        if total_bytes > 0 {
            let percent =
                ((total_downloaded as f64 / total_bytes as f64) * 100.0).clamp(0.0, 100.0) as u32;
            if active.len() == 1 {
                format!("Conflux - {percent}%")
            } else {
                format!("Conflux - {percent}% ({} active)", active.len())
            }
        } else if active.len() == 1 {
            "Conflux - Downloading".to_string()
        } else {
            format!("Conflux - Downloading ({} active)", active.len())
        }
    } else if tasks.values().any(|t| t.status == TaskStatus::Paused) {
        "Conflux - Paused".to_string()
    } else {
        "Conflux".to_string()
    }
}

pub fn update_tray_tooltip(app: &AppHandle, tasks: &HashMap<String, DownloadTaskState>) {
    let text = format_tray_tooltip(tasks);
    if let Ok(mut last) = LAST_TOOLTIP.lock() {
        if last.as_deref() == Some(&text) {
            return;
        }
        *last = Some(text.clone());
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(&text));
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn handle_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
        | TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            let app = tray.app_handle();
            if let Some(window) = app.get_webview_window("main") {
                if window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) {
                    if window.is_focused().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.set_focus();
                    }
                } else {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
        }
        _ => {}
    }
}

fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "show" => {
            show_main_window(app);
        }
        "add" => {
            show_main_window(app);
            let _ = app.emit("open-add-dialog", ());
        }
        "pause_all" => {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app_clone.state::<AppState>();
                commands::pause_all_internal(&app_clone, &state).await;
            });
        }
        "resume_all" => {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app_clone.state::<AppState>();
                commands::resume_all_internal(&app_clone, &state).await;
            });
        }
        "settings" => {
            show_main_window(app);
            let _ = app.emit("open-settings", ());
        }
        "quit" => {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app_clone.state::<AppState>();
                commands::pause_all_internal(&app_clone, &state).await;
                app_clone.exit(0);
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, status: TaskStatus, downloaded: u64, total: u64) -> DownloadTaskState {
        DownloadTaskState {
            id: id.into(),
            url: "https://example.com/file".into(),
            filename: format!("{id}.bin"),
            save_dir: ".".into(),
            save_path: format!("./{id}.bin"),
            adapter_ids: vec![],
            total_bytes: total,
            supports_ranges: true,
            downloaded_bytes: downloaded,
            status,
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
    fn test_format_tray_tooltip_empty() {
        let tasks = HashMap::new();
        assert_eq!(format_tray_tooltip(&tasks), "Conflux");
    }

    #[test]
    fn test_format_tray_tooltip_completed_only() {
        let mut tasks = HashMap::new();
        tasks.insert("1".into(), make_task("1", TaskStatus::Completed, 100, 100));
        assert_eq!(format_tray_tooltip(&tasks), "Conflux");
    }

    #[test]
    fn test_format_tray_tooltip_paused() {
        let mut tasks = HashMap::new();
        tasks.insert("1".into(), make_task("1", TaskStatus::Paused, 50, 100));
        assert_eq!(format_tray_tooltip(&tasks), "Conflux - Paused");
    }

    #[test]
    fn test_format_tray_tooltip_active_single() {
        let mut tasks = HashMap::new();
        tasks.insert("1".into(), make_task("1", TaskStatus::Downloading, 45, 100));
        assert_eq!(format_tray_tooltip(&tasks), "Conflux - 45%");
    }

    #[test]
    fn test_format_tray_tooltip_active_multiple() {
        let mut tasks = HashMap::new();
        tasks.insert("1".into(), make_task("1", TaskStatus::Downloading, 30, 100));
        tasks.insert("2".into(), make_task("2", TaskStatus::Downloading, 70, 100));
        assert_eq!(format_tray_tooltip(&tasks), "Conflux - 50% (2 active)");
    }

    #[test]
    fn test_format_tray_tooltip_unknown_size() {
        let mut tasks = HashMap::new();
        tasks.insert("1".into(), make_task("1", TaskStatus::Downloading, 5000, 0));
        assert_eq!(format_tray_tooltip(&tasks), "Conflux - Downloading");
    }
}
