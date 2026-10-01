mod adapters;
mod commands;
mod diagnostics;
mod fileutil;
mod history;
mod logging;
mod redact;
mod settings;
mod state;
mod tray;
mod updater;

use history::HistoryStore;
use state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};
use tracing::{info, warn};

/// Set once a graceful quit has started, so repeated close/quit requests do not race it.
static QUITTING: AtomicBool = AtomicBool::new(false);

/// Hides the window, pauses every running download (concurrently, bounded by one stop
/// timeout), saves history and exits. Further calls while quitting are ignored.
pub(crate) fn quit_gracefully(app: &tauri::AppHandle) {
    if QUITTING.swap(true, Ordering::SeqCst) {
        return;
    }
    info!("Quitting: pausing downloads and saving history");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        commands::pause_all_internal(&app, &state).await;
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::install_panic_hook();

    tauri::Builder::default()
        // Registered first so a second launch exits before touching shared data files.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Args are logged redacted (they may be a download URL) and forwarded to the UI
            // for later protocol-handler / "open with" support.
            info!(
                args = %redact::redact_urls(&format!("{:?}", args.iter().skip(1).collect::<Vec<_>>())),
                "Second instance launched; focusing the main window"
            );
            tray::show_main_window(app);
            let _ = app.emit("second-instance", args);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let log_dir = app.path().app_log_dir().ok();
            if let Some(problem) = logging::init(log_dir.as_deref()) {
                eprintln!("{problem}");
            }
            info!("{} starting", logging::build_info());
            if log_dir.is_none() {
                warn!("No log dir; logs go to stderr only");
            }
            let settings_path = match app.path().app_config_dir() {
                Ok(dir) => Some(dir.join("settings.json")),
                Err(e) => {
                    warn!("No config dir; settings will not be saved: {e}");
                    None
                }
            };
            let history_path = match app.path().app_data_dir() {
                Ok(dir) => Some(dir.join("downloads.json")),
                Err(e) => {
                    warn!("No data dir; download history will not be saved: {e}");
                    None
                }
            };
            info!(?settings_path, ?history_path, "Resolved storage paths");

            let (settings, settings_notice) = settings_path
                .as_deref()
                .map(settings::load)
                .unwrap_or_default();
            let history = HistoryStore::new(history_path);
            let (tasks, history_notice) = history.load();
            info!(restored = tasks.len(), "Loaded download history");
            app.manage(AppState::new(settings, settings_path, history, tasks));
            app.manage(updater::PendingUpdate::default());
            for notice in [settings_notice, history_notice].into_iter().flatten() {
                app.state::<AppState>().push_notice(notice);
            }

            if let Err(e) = tray::setup_tray(app.handle()) {
                warn!("Failed to setup system tray: {e}");
            } else {
                let state = app.state::<AppState>();
                if let Ok(tasks) = state.tasks.try_read() {
                    tray::update_tray_tooltip(app.handle(), &tasks);
                };
            }

            // Start background non-polling network adapter watcher inside Tokio runtime context
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match conflux_core::NetworkWatcher::start(conflux_core::DEFAULT_DEBOUNCE) {
                    Ok(watcher) => {
                        let mut rx = watcher.receiver();
                        // Keep watcher alive inside this task
                        let _watcher = watcher;
                        let state = app_handle.state::<AppState>();
                        let initial = rx.borrow().clone();
                        commands::refresh_adapters(
                            &state,
                            initial,
                            commands::AddPolicy::IfAutoAggregate,
                        )
                        .await;

                        while rx.changed().await.is_ok() {
                            let discovered = rx.borrow().clone();
                            // Applies overrides, updates `last_adapters` and hot-plugs the
                            // difference into active downloads, all under one lock.
                            let current_adapters = commands::refresh_adapters(
                                &state,
                                discovered,
                                commands::AddPolicy::IfAutoAggregate,
                            )
                            .await;

                            let infos = state.adapter_infos(current_adapters);
                            if let Err(e) = app_handle.emit("network-adapters-changed", &infos) {
                                warn!("Failed to emit network-adapters-changed: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Could not start NetworkWatcher: {e:#}");
                    }
                }
            });

            // After an in-app update: resume the downloads that were running, then do the
            // quiet update check (never installs by itself).
            let updater_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                updater::resume_after_update(&updater_handle).await;
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                updater::startup_check(&updater_handle).await;
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if QUITTING.load(Ordering::SeqCst) {
                    // A graceful quit already paused everything; let the window go.
                    return;
                }
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let close_to_tray = state
                    .settings
                    .try_read()
                    .map(|s| s.close_to_tray)
                    .unwrap_or(true);
                // Never let the window close directly: downloads must be paused (resume data
                // flushed) and history saved before the process exits.
                api.prevent_close();
                if close_to_tray {
                    let _ = window.hide();
                } else {
                    quit_gracefully(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::discover_adapters,
            commands::set_adapter_enabled,
            commands::probe_url,
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
            commands::pause_all,
            commands::resume_all,
            commands::remove_download,
            commands::list_tasks,
            commands::get_settings,
            commands::update_settings,
            commands::open_file,
            commands::reveal_file,
            commands::apply_window_theme,
            commands::take_startup_notices,
            commands::folder_exists,
            commands::get_diagnostics,
            commands::open_logs_folder,
            updater::check_for_update,
            updater::install_update,
            updater::open_about_link,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Conflux desktop");
}
