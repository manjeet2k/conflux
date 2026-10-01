//! In-app updates (`tauri-plugin-updater`), driven entirely from Rust so the webview needs no
//! updater or process capability.
//!
//! Flow of [`install_update`]: download the (signature-verified) installer first, because that
//! can fail and must not disturb downloads. Only then pause every running download, persist
//! their ids to `resume_after_update.json`, and hand over to the installer. On Windows the
//! plugin's `install` starts the NSIS installer in passive mode and exits the process; the
//! installer relaunches the new version, whose startup calls [`resume_after_update`].
//!
//! Endpoint: a fixed-tag GitHub release (`updater-beta`) whose `latest.json` is overwritten on
//! every published release, because `releases/latest/download/` ignores prereleases. The repo
//! must be public for installed copies to fetch it (see docs/RELEASING.md).

use crate::commands;
use crate::state::AppState;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tracing::{info, warn};

/// Emitted to the UI when the quiet startup check finds a newer version.
pub const UPDATE_AVAILABLE_EVENT: &str = "update-available";

// BEGIN-PURE-LOGIC (no tauri imports below this marker until END-PURE-LOGIC)
mod logic {
    use std::future::Future;
    use std::io;
    use std::path::Path;

    pub const RESUME_FILE: &str = "resume_after_update.json";

    /// True when `candidate` has higher semver precedence than `current`. Unparseable versions
    /// are never "newer", so a malformed manifest cannot trigger an update (or a downgrade).
    pub fn is_newer(current: &str, candidate: &str) -> bool {
        match (
            semver::Version::parse(current.trim_start_matches('v')),
            semver::Version::parse(candidate.trim_start_matches('v')),
        ) {
            (Ok(current), Ok(candidate)) => candidate > current,
            _ => false,
        }
    }

    /// Saves the ids of the downloads to resume after the update.
    pub fn write_resume_file(path: &Path, ids: &[String]) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec(ids).map_err(io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Reads and deletes the resume file. A missing file is "nothing to resume"; a corrupt one
    /// is deleted and also yields nothing, so it can never wedge startup.
    pub fn take_resume_file(path: &Path) -> Vec<String> {
        let ids = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Vec<String>>(&bytes).unwrap_or_default(),
            Err(_) => return Vec::new(),
        };
        let _ = std::fs::remove_file(path);
        ids
    }

    /// The side effects of installing an update, in the order [`install_flow`] runs them.
    pub trait InstallSteps {
        /// Pauses every running download; returns the ids that were running.
        fn pause_running(&mut self) -> impl Future<Output = Vec<String>>;
        fn persist(&mut self, ids: &[String]) -> io::Result<()>;
        /// Hands over to the installer. On success the process normally never returns.
        fn install(&mut self) -> Result<(), String>;
        /// Undo after a failed persist/install: resume the paused downloads.
        fn resume(&mut self, ids: &[String]) -> impl Future<Output = ()>;
    }

    /// pause -> persist -> install. Downloads are never left paused by a failed update.
    pub async fn install_flow<S: InstallSteps>(steps: &mut S) -> Result<(), String> {
        let ids = steps.pause_running().await;
        if let Err(e) = steps.persist(&ids) {
            steps.resume(&ids).await;
            return Err(format!("Could not save the downloads to resume: {e}"));
        }
        if let Err(e) = steps.install() {
            steps.resume(&ids).await;
            return Err(e);
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn newer_versions_follow_semver_precedence() {
            assert!(is_newer("0.2.0-beta.1", "0.2.0-beta.2"));
            assert!(is_newer("0.2.0-beta.2", "0.2.0-beta.10"));
            assert!(is_newer("0.2.0-beta.9", "0.2.0"));
            assert!(is_newer("0.1.5", "v0.2.0-beta.1"));
            assert!(!is_newer("0.2.0", "0.2.0-beta.3"));
            assert!(!is_newer("0.2.0", "0.2.0"));
            assert!(!is_newer("0.2.1", "0.2.0"));
            assert!(!is_newer("0.2.0", "garbage"));
            assert!(!is_newer("garbage", "0.2.0"));
        }

        #[test]
        fn resume_file_round_trips_and_is_deleted_once_read() {
            let dir = std::env::temp_dir().join(format!("conflux-resume-{}", std::process::id()));
            let path = dir.join(RESUME_FILE);
            let ids = vec!["a".to_string(), "b".to_string()];
            write_resume_file(&path, &ids).unwrap();
            assert_eq!(take_resume_file(&path), ids);
            assert!(!path.exists());
            assert!(take_resume_file(&path).is_empty());
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn corrupt_resume_file_yields_nothing_and_is_removed() {
            let dir =
                std::env::temp_dir().join(format!("conflux-resume-bad-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join(RESUME_FILE);
            std::fs::write(&path, b"{not json").unwrap();
            assert!(take_resume_file(&path).is_empty());
            assert!(!path.exists());
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[derive(Default)]
        struct Recorder {
            log: Vec<String>,
            fail_persist: bool,
            fail_install: bool,
        }

        impl InstallSteps for Recorder {
            async fn pause_running(&mut self) -> Vec<String> {
                self.log.push("pause".into());
                vec!["t1".into(), "t2".into()]
            }
            fn persist(&mut self, ids: &[String]) -> io::Result<()> {
                self.log.push(format!("persist:{}", ids.join(",")));
                if self.fail_persist {
                    Err(io::Error::other("disk full"))
                } else {
                    Ok(())
                }
            }
            fn install(&mut self) -> Result<(), String> {
                self.log.push("install".into());
                if self.fail_install {
                    Err("installer failed".into())
                } else {
                    Ok(())
                }
            }
            async fn resume(&mut self, ids: &[String]) {
                self.log.push(format!("resume:{}", ids.join(",")));
            }
        }

        fn run(mut r: Recorder) -> (Result<(), String>, Vec<String>) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let result = rt.block_on(install_flow(&mut r));
            (result, r.log)
        }

        #[test]
        fn pauses_then_persists_then_installs() {
            let (result, log) = run(Recorder::default());
            assert!(result.is_ok());
            assert_eq!(log, ["pause", "persist:t1,t2", "install"]);
        }

        #[test]
        fn failed_persist_never_installs_and_resumes_downloads() {
            let (result, log) = run(Recorder {
                fail_persist: true,
                ..Default::default()
            });
            assert!(result.is_err());
            assert_eq!(log, ["pause", "persist:t1,t2", "resume:t1,t2"]);
        }

        #[test]
        fn failed_install_resumes_downloads() {
            let (result, log) = run(Recorder {
                fail_install: true,
                ..Default::default()
            });
            assert_eq!(result.unwrap_err(), "installer failed");
            assert_eq!(log, ["pause", "persist:t1,t2", "install", "resume:t1,t2"]);
        }
    }
}
// END-PURE-LOGIC

pub use logic::RESUME_FILE;
use logic::{install_flow, is_newer, take_resume_file, write_resume_file, InstallSteps};

/// What the UI shows about an available update.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

impl UpdateInfo {
    fn from_update(update: &Update) -> Self {
        Self {
            version: update.version.clone(),
            notes: update.body.clone().filter(|n| !n.trim().is_empty()),
            date: update.date.map(|d| d.to_string()),
        }
    }
}

/// The update found by the last check, kept so `install_update` installs exactly what the user
/// was shown.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

fn resume_file_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(RESUME_FILE))
}

async fn check(app: &AppHandle) -> Result<Option<Update>, String> {
    let updater = app
        .updater_builder()
        .version_comparator(|current, remote| {
            is_newer(&current.to_string(), &remote.version.to_string())
        })
        .build()
        .map_err(|e| format!("Update check is not available: {e}"))?;
    updater
        .check()
        .await
        .map_err(|e| format!("Could not check for updates: {e}"))
}

/// Checks the update endpoint. `None` = already up to date.
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>, String> {
    let found = check(&app).await?;
    let info = found.as_ref().map(UpdateInfo::from_update);
    *pending.0.lock().unwrap_or_else(|e| e.into_inner()) = found;
    Ok(info)
}

struct RealSteps<'a> {
    app: &'a AppHandle,
    state: &'a AppState,
    update: &'a Update,
    bytes: &'a [u8],
    path: Option<std::path::PathBuf>,
}

impl InstallSteps for RealSteps<'_> {
    async fn pause_running(&mut self) -> Vec<String> {
        let running: Vec<String> = {
            let tasks = self.state.tasks.read().await;
            tasks
                .values()
                .filter(|t| t.status == crate::state::TaskStatus::Downloading)
                .map(|t| t.id.clone())
                .collect()
        };
        commands::pause_all_internal(self.app, self.state).await;
        running
    }

    fn persist(&mut self, ids: &[String]) -> std::io::Result<()> {
        match &self.path {
            Some(path) if !ids.is_empty() => write_resume_file(path, ids),
            _ => Ok(()),
        }
    }

    fn install(&mut self) -> Result<(), String> {
        self.update
            .install(self.bytes)
            .map_err(|e| format!("Installing the update failed: {e}"))
    }

    async fn resume(&mut self, ids: &[String]) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
        for id in ids {
            if let Err(e) = commands::resume_task_internal(self.app, self.state, id).await {
                warn!(task_id = %id, "Could not resume after a failed update: {e}");
            }
        }
    }
}

/// Downloads and installs the update found by the last check, pausing downloads first and
/// resuming them after the new version starts.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingUpdate>,
) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or("No update has been found; check for updates first")?;
    info!(version = %update.version, "Downloading update");
    // Download (and signature-verify) before touching any running download.
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Downloading the update failed: {e}"))?;
    let mut steps = RealSteps {
        app: &app,
        state: &state,
        update: &update,
        bytes: &bytes,
        path: resume_file_path(&app),
    };
    info!("Pausing downloads and installing update");
    install_flow(&mut steps).await
}

/// At startup: resumes exactly the downloads that were running when an update was installed.
pub async fn resume_after_update(app: &AppHandle) {
    let Some(path) = resume_file_path(app) else {
        return;
    };
    let ids = take_resume_file(&path);
    if ids.is_empty() {
        return;
    }
    info!(
        count = ids.len(),
        "Resuming downloads paused for the update"
    );
    let state = app.state::<AppState>();
    for id in ids {
        if let Err(e) = commands::resume_task_internal(app, &state, &id).await {
            warn!(task_id = %id, "Could not resume after the update: {e}");
        }
    }
}

/// Quiet startup check: only notifies (event for the UI), never downloads or installs.
pub async fn startup_check(app: &AppHandle) {
    let enabled = app
        .state::<AppState>()
        .settings
        .read()
        .await
        .check_updates_on_start;
    if !enabled {
        return;
    }
    match check(app).await {
        Ok(Some(update)) => {
            let info = UpdateInfo::from_update(&update);
            info!(version = %info.version, "Update available");
            *app.state::<PendingUpdate>()
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(update);
            if let Err(e) = app.emit(UPDATE_AVAILABLE_EVENT, &info) {
                warn!("Failed to emit {UPDATE_AVAILABLE_EVENT}: {e}");
            }
            use tauri_plugin_notification::NotificationExt;
            if let Err(e) = app
                .notification()
                .builder()
                .title("Conflux update available")
                .body(format!(
                    "Version {} is available. Open Settings > About & Updates to install it.",
                    info.version
                ))
                .show()
            {
                warn!("Failed to show the update notification: {e}");
            }
        }
        Ok(None) => info!("No update available"),
        Err(e) => warn!("Startup update check failed: {e}"),
    }
}

/// Opens one of a fixed set of About-page targets. The webview names the target only; it never
/// supplies a URL or path, so it needs no opener capability.
#[tauri::command]
pub fn open_about_link(app: AppHandle, target: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    match target.as_str() {
        "licenses" => {
            let path = app
                .path()
                .resource_dir()
                .map_err(|e| format!("Resource folder not found: {e}"))?
                .join("THIRD_PARTY_LICENSES.md");
            if !path.is_file() {
                return Err("The licenses file is not installed with this build".to_string());
            }
            app.opener()
                .open_path(path.to_string_lossy(), None::<&str>)
                .map_err(|e| format!("Could not open the licenses file: {e}"))
        }
        "repo" => app
            .opener()
            .open_url(REPO_URL, None::<&str>)
            .map_err(|e| format!("Could not open the browser: {e}")),
        "releases" => app
            .opener()
            .open_url(format!("{REPO_URL}/releases"), None::<&str>)
            .map_err(|e| format!("Could not open the browser: {e}")),
        other => Err(format!("Unknown link: {other}")),
    }
}

const REPO_URL: &str = "https://github.com/manjeet2k/conflux";
