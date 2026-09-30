mod adapters;
mod commands;
mod history;
mod settings;
mod state;

use history::HistoryStore;
use state::AppState;
use tauri::Manager;
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::discover_adapters,
            commands::probe_url,
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
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
