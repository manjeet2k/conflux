use crate::commands;
use crate::state::AppState;
use tauri::menu::{MenuBuilder, MenuEvent};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tracing::info;

pub const TRAY_ID: &str = "conflux-tray";

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
        .tooltip("Conflux - Multi-Interface Download Accelerator")
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
