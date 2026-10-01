mod adapters;
mod commands;
mod history;
mod settings;
mod state;
mod tray;

use history::HistoryStore;
use state::AppState;
use tauri::{Emitter, Manager};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
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

            let settings = settings_path
                .as_deref()
                .map(settings::load)
                .unwrap_or_default();
            let history = HistoryStore::new(history_path);
            let tasks = history.load();
            info!(restored = tasks.len(), "Loaded download history");
            app.manage(AppState::new(settings, settings_path, history, tasks));

            if let Err(e) = tray::setup_tray(app.handle()) {
                warn!("Failed to setup system tray: {e}");
            } else {
                let state = app.state::<AppState>();
                if let Ok(tasks) = state.tasks.try_read() {
                    tray::update_tray_tooltip(app.handle(), &tasks);
                };
            }

            // Start background non-polling network adapter watcher
            match conflux_core::NetworkWatcher::start(conflux_core::DEFAULT_DEBOUNCE) {
                Ok(watcher) => {
                    let app_handle = app.handle().clone();
                    let mut rx = watcher.receiver();

                    tauri::async_runtime::spawn(async move {
                        // Keep watcher alive inside this task
                        let _watcher = watcher;
                        let state = app_handle.state::<AppState>();
                        {
                            let mut initial_adapters = rx.borrow().clone();
                            let overrides = state.settings.read().await.adapter_overrides.clone();
                            for a in initial_adapters.iter_mut() {
                                adapters::apply_overrides(a, &overrides);
                            }
                            *state.last_adapters.write().await = initial_adapters;
                        }

                        while rx.changed().await.is_ok() {
                            let mut current_adapters = rx.borrow().clone();
                            let settings = state.settings.read().await.clone();
                            for a in current_adapters.iter_mut() {
                                adapters::apply_overrides(a, &settings.adapter_overrides);
                            }
                            let (added, removed) = {
                                let mut last = state.last_adapters.write().await;
                                let diff = conflux_core::diff_adapters(&last, &current_adapters);
                                *last = current_adapters.clone();
                                diff
                            };

                            // 1. Emit updated adapter list to UI
                            let infos: Vec<adapters::AdapterInfo> = current_adapters
                                .into_iter()
                                .map(adapters::to_info)
                                .collect();
                            if let Err(e) = app_handle.emit("network-adapters-changed", &infos) {
                                warn!("Failed to emit network-adapters-changed: {e}");
                            }

                            // 2. Hot-plug into active downloads
                            let added_to_apply = if settings.auto_aggregate_adapters {
                                added
                            } else {
                                Vec::new()
                            };
                            if !added_to_apply.is_empty() || !removed.is_empty() {
                                commands::handle_network_change(&state, &added_to_apply, &removed)
                                    .await;
                            }
                        }
                    });
                }
                Err(e) => {
                    warn!("Could not start NetworkWatcher: {e:#}");
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let close_to_tray = state
                    .settings
                    .try_read()
                    .map(|s| s.close_to_tray)
                    .unwrap_or(true);
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Conflux desktop");
}
