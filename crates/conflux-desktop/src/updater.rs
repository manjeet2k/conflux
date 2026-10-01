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

    /// Reads the resume file without deleting it: it must survive until every id has been
    /// resumed or has permanently failed (see [`resume_with_retry`]). A missing file is
    /// "nothing to resume"; a corrupt one is deleted and also yields nothing, so it can never
    /// wedge startup.
    pub fn read_resume_file(path: &Path) -> Vec<String> {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Vec<String>>(&bytes) {
                Ok(ids) => ids,
                Err(_) => {
                    let _ = std::fs::remove_file(path);
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        }
    }

    /// Persists the ids still to resume; deletes the file when none are left.
    pub fn save_remaining(path: &Path, ids: &[String]) {
        let result = if ids.is_empty() {
            std::fs::remove_file(path)
        } else {
            write_resume_file(path, ids)
        };
        if let Err(e) = result {
            if e.kind() != io::ErrorKind::NotFound {
                tracing::warn!("Could not update the resume-after-update file: {e}");
            }
        }
    }

    /// Delays between resume rounds: 3 attempts over ~30 s, enough for the network to come up
    /// right after the installer relaunches the app.
    pub const RESUME_BACKOFF: [std::time::Duration; 2] = [
        std::time::Duration::from_secs(10),
        std::time::Duration::from_secs(20),
    ];

    /// Errors no retry can fix: the task is gone, finished, or the user disabled every adapter.
    pub fn is_permanent_resume_error(err: &str) -> bool {
        err.starts_with("Unknown task")
            || err.contains("already complete")
            || err.starts_with("No network adapters are enabled")
    }

    pub trait Resumer {
        fn resume(&mut self, id: &str) -> impl Future<Output = Result<(), String>>;
        fn sleep(&mut self, d: std::time::Duration) -> impl Future<Output = ()>;
    }

    /// Resumes `ids` in order, in rounds: ids that fail transiently are retried after each
    /// `backoff` delay. After each round the file is rewritten with the ids still outstanding
    /// (and deleted once none are left), so a crash mid-way never forgets a download. Returns the ids given up on.
    pub async fn resume_with_retry<R: Resumer>(
        path: &Path,
        ids: Vec<String>,
        backoff: &[std::time::Duration],
        r: &mut R,
    ) -> Vec<String> {
        let mut pending = ids;
        let mut abandoned = Vec::new();
        let mut round = 0;
        while !pending.is_empty() {
            let mut failed = Vec::new();
            for id in std::mem::take(&mut pending) {
                match r.resume(&id).await {
                    Ok(()) => {
                        tracing::info!(task_id = %id, "Resumed after the update");
                    }
                    Err(e) if is_permanent_resume_error(&e) => {
                        tracing::warn!(task_id = %id, "Not resuming after the update: {e}");
                        abandoned.push(id.clone());
                    }
                    Err(e) => {
                        tracing::warn!(task_id = %id, attempt = round + 1, "Could not resume after the update: {e}");
                        failed.push(id);
                        continue;
                    }
                }
            }
            pending = failed;
            // Persist what is still outstanding. Done once per round: a crash mid-round
            // leaves the whole round in the file, and re-resuming a running task is a no-op.
            save_remaining(path, &pending);
            let Some(delay) = backoff.get(round).copied().filter(|_| !pending.is_empty()) else {
                break;
            };
            r.sleep(delay).await;
            round += 1;
        }
        for id in &pending {
            tracing::warn!(task_id = %id, "Giving up resuming after the update; it stays paused");
        }
        // Out of attempts counts as settled: the user can resume these by hand, and a stale
        // file must not re-trigger resumes on every later launch.
        save_remaining(path, &[]);
        abandoned.extend(pending);
        abandoned
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

        fn temp_path(tag: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join(format!("conflux-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            dir.join(RESUME_FILE)
        }

        #[test]
        fn resume_file_round_trips_and_is_kept_until_saved_empty() {
            let path = temp_path("resume");
            let ids = vec!["a".to_string(), "b".to_string()];
            write_resume_file(&path, &ids).unwrap();
            assert_eq!(read_resume_file(&path), ids);
            assert!(path.exists(), "reading must not delete the file");
            save_remaining(&path, &["b".to_string()]);
            assert_eq!(read_resume_file(&path), ["b"]);
            save_remaining(&path, &[]);
            assert!(!path.exists());
            assert!(read_resume_file(&path).is_empty());
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }

        #[test]
        fn corrupt_resume_file_yields_nothing_and_is_removed() {
            let path = temp_path("resume-bad");
            std::fs::write(&path, b"{not json").unwrap();
            assert!(read_resume_file(&path).is_empty());
            assert!(!path.exists());
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }

        #[test]
        fn permanent_errors_are_classified() {
            assert!(is_permanent_resume_error("Unknown task: x"));
            assert!(is_permanent_resume_error("Download is already complete"));
            assert!(is_permanent_resume_error(
                "No network adapters are enabled. Enable one on the Network page."
            ));
            assert!(!is_permanent_resume_error("Probe failed: dns error"));
        }

        /// Fails each id a scripted number of times, recording calls and sleeps.
        struct Scripted {
            fail_times: std::collections::HashMap<String, (u32, String)>,
            log: Vec<String>,
            path: std::path::PathBuf,
            file_at_sleep: Vec<Vec<String>>,
        }

        impl Resumer for Scripted {
            async fn resume(&mut self, id: &str) -> Result<(), String> {
                self.log.push(format!("try:{id}"));
                match self.fail_times.get_mut(id) {
                    Some((n, msg)) if *n > 0 => {
                        *n -= 1;
                        Err(msg.clone())
                    }
                    _ => Ok(()),
                }
            }
            async fn sleep(&mut self, d: std::time::Duration) {
                self.log.push(format!("sleep:{}", d.as_secs()));
                self.file_at_sleep.push(read_resume_file(&self.path));
            }
        }

        fn run_retry(
            tag: &str,
            fails: &[(&str, u32, &str)],
        ) -> (Vec<String>, Vec<String>, Vec<Vec<String>>, bool) {
            let path = temp_path(tag);
            let ids: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
            write_resume_file(&path, &ids).unwrap();
            let mut r = Scripted {
                fail_times: fails
                    .iter()
                    .map(|(i, n, m)| (i.to_string(), (*n, m.to_string())))
                    .collect(),
                log: Vec::new(),
                path: path.clone(),
                file_at_sleep: Vec::new(),
            };
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let abandoned = rt.block_on(resume_with_retry(&path, ids, &RESUME_BACKOFF, &mut r));
            let exists = path.exists();
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
            (abandoned, r.log, r.file_at_sleep, exists)
        }

        #[test]
        fn all_resumed_first_try_deletes_file_without_sleeping() {
            let (abandoned, log, _, exists) = run_retry("retry-ok", &[]);
            assert!(abandoned.is_empty());
            assert_eq!(log, ["try:a", "try:b", "try:c"]);
            assert!(!exists);
        }

        #[test]
        fn transient_failure_is_retried_in_order_with_file_kept() {
            let (abandoned, log, file_at_sleep, exists) =
                run_retry("retry-transient", &[("b", 2, "Probe failed: no network")]);
            assert!(abandoned.is_empty());
            assert_eq!(
                log,
                ["try:a", "try:b", "try:c", "sleep:10", "try:b", "sleep:20", "try:b"]
            );
            // While waiting, the file holds exactly the id still outstanding.
            assert_eq!(
                file_at_sleep,
                [vec!["b".to_string()], vec!["b".to_string()]]
            );
            assert!(!exists);
        }

        #[test]
        fn gives_up_after_three_attempts_and_permanent_errors_are_not_retried() {
            let (abandoned, log, _, exists) = run_retry(
                "retry-giveup",
                &[
                    ("a", 99, "Probe failed: no network"),
                    ("c", 99, "Unknown task: c"),
                ],
            );
            assert_eq!(abandoned, ["c", "a"]);
            let tries = |id: &str| log.iter().filter(|l| **l == format!("try:{id}")).count();
            assert_eq!((tries("a"), tries("b"), tries("c")), (3, 1, 1));
            // Nothing outstanding after giving up: the file is not kept forever.
            assert!(!exists);
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
use logic::{
    install_flow, is_newer, read_resume_file, resume_with_retry, write_resume_file, InstallSteps,
    Resumer, RESUME_BACKOFF,
};

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
    let ids = read_resume_file(&path);
    if ids.is_empty() {
        return;
    }
    info!(
        count = ids.len(),
        "Resuming downloads paused for the update"
    );
    let state = app.state::<AppState>();
    struct Live<'a> {
        app: &'a AppHandle,
        state: &'a AppState,
    }
    impl Resumer for Live<'_> {
        async fn resume(&mut self, id: &str) -> Result<(), String> {
            commands::resume_task_internal(self.app, self.state, id)
                .await
                .map(|_| ())
        }
        async fn sleep(&mut self, d: std::time::Duration) {
            tokio::time::sleep(d).await;
        }
    }
    // The file stays on disk until each id is resumed or given up on.
    let given_up = resume_with_retry(
        &path,
        ids,
        &RESUME_BACKOFF,
        &mut Live { app, state: &state },
    )
    .await;
    if !given_up.is_empty() {
        warn!(
            count = given_up.len(),
            "Downloads left paused after the update: {given_up:?}"
        );
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
