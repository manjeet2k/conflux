//! Resume metadata in Conflux app data records which chunks are durably on disk, so a
//! stopped or failed download can continue instead of starting over. Legacy sidecars next
//! to the output are read for compatibility and removed after app-data persistence.
//!
//! Durability rule: the data file is synced *before* a sidecar listing new chunks is
//! written, and the sidecar is replaced atomically (write temp file, sync, rename). So a
//! chunk listed as completed is always really on disk, even after a crash or power loss.

use crate::engine::{DownloadProbe, RequestHeaders};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SIDECAR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeState {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_identity: Option<crate::FileIdentity>,
    pub total_bytes: u64,
    pub chunk_size: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<RequestHeaders>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_origin: Option<String>,
    /// Completed chunk ids as ascending ranges, e.g. `"0-5,7,9-12"` (empty = none).
    pub completed: String,
}

pub(crate) fn resource_id(url: &str) -> String {
    format!("{:x}", Sha256::digest(url.as_bytes()))
}

/// Resume files share the desktop app's roaming data directory on Windows.
fn resume_dir() -> std::io::Result<PathBuf> {
    #[cfg(windows)]
    {
        let app_data = std::env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Windows roaming app data is unavailable; cannot store download resume data",
                )
            })?;
        Ok(app_data.join("com.conflux.desktop").join("resume"))
    }
    // Linux exists only for the WSL core tests. Keep their metadata out of download dirs
    // and separate test processes without introducing a supported Linux app-data location.
    #[cfg(not(windows))]
    {
        Ok(std::env::temp_dir().join(format!("conflux-resume-tests-{}", std::process::id())))
    }
}

/// Returns an app-data filename keyed by the full output path, not just its basename.
/// Resolving the parent keeps the key stable before creation and after file deletion.
pub fn resume_sidecar_path(output_path: &Path) -> std::io::Result<PathBuf> {
    let absolute = std::path::absolute(output_path)?;
    let normalized = absolute
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .and_then(|parent| absolute.file_name().map(|name| parent.join(name)))
        .unwrap_or(absolute);
    let key = normalized.as_os_str().as_encoded_bytes();
    #[cfg(windows)]
    let key = key.to_ascii_lowercase();
    let name = format!("{:x}.json", Sha256::digest(key));
    Ok(resume_dir()?.join(name))
}

fn legacy_sidecar_path(output_path: &Path) -> PathBuf {
    let mut name: OsString = output_path
        .file_name()
        .map(OsString::from)
        .unwrap_or_default();
    name.push(".conflux.json");
    output_path.with_file_name(name)
}

/// Both locations reserve an output name until its resume data has been removed.
pub fn has_resume_sidecar(output_path: &Path) -> bool {
    resume_sidecar_path(output_path).is_ok_and(|path| std::fs::symlink_metadata(path).is_ok())
        || std::fs::symlink_metadata(legacy_sidecar_path(output_path)).is_ok()
}

/// Prefer app data. Only fall back to a legacy sidecar if app-data metadata is absent.
pub(crate) fn read_resume_state(output_path: &Path) -> Result<ResumeState> {
    let path = resume_sidecar_path(output_path)?;
    match read_sidecar(&path) {
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            read_sidecar(&legacy_sidecar_path(output_path))
        }
        result => result,
    }
}

/// Persist before removing legacy metadata, so interruption never loses the saved chunks.
pub(crate) fn write_resume_state(output_path: &Path, state: &ResumeState) -> Result<()> {
    write_sidecar(&resume_sidecar_path(output_path)?, state)?;
    remove_sidecar_file(&legacy_sidecar_path(output_path))?;
    Ok(())
}

/// Encodes ids as ascending ranges. Input may be in any order and contain duplicates.
pub(crate) fn encode_ranges(ids: &[usize]) -> String {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < ids.len() {
        let start = ids[i];
        let mut end = start;
        while i + 1 < ids.len() && ids[i + 1] == end + 1 {
            i += 1;
            end = ids[i];
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{start}-{end}")
        });
        i += 1;
    }
    parts.join(",")
}

/// Decodes [`encode_ranges`] output. Fails on malformed input or any id `>= limit`.
pub(crate) fn decode_ranges(s: &str, limit: usize) -> Result<Vec<usize>> {
    let mut ids = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (start, end) = match part.split_once('-') {
            Some((a, b)) => (a.trim().parse::<usize>()?, b.trim().parse::<usize>()?),
            None => {
                let n = part.parse::<usize>()?;
                (n, n)
            }
        };
        if end < start || end >= limit {
            bail!("invalid chunk range {part:?} for {limit} chunks");
        }
        ids.extend(start..=end);
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

pub(crate) fn read_sidecar(path: &Path) -> Result<ResumeState> {
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {:?}", path))?;
    serde_json::from_str(&text).with_context(|| format!("cannot parse {:?}", path))
}

/// Restore only non-cookie headers and only the Referer for the saved resource origin.
pub fn read_resume_headers(output: &Path, url: &str) -> Result<Option<RequestHeaders>> {
    let state = read_resume_state(output)?;
    Ok(state.headers.map(|mut headers| {
        headers.cookie = None;
        let saved_origin = state.header_origin;
        let same_origin = reqwest::Url::parse(url).ok().is_some_and(|target| {
            saved_origin.as_deref() == Some(target.origin().ascii_serialization().as_str())
        });
        if !same_origin {
            headers.referer = None;
        }
        headers
    }))
}

/// Atomically replaces the sidecar at `path` (temp file + sync + rename).
pub(crate) fn write_sidecar(path: &Path, state: &ResumeState) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Cannot create resume data directory")?;
    }
    let mut tmp_name: OsString = path.file_name().map(OsString::from).unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    let json = serde_json::to_vec(state)?;
    {
        let mut f =
            std::fs::File::create(&tmp).with_context(|| format!("cannot create {:?}", tmp))?;
        f.write_all(&json)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).with_context(|| format!("cannot rename {:?} to {:?}", tmp, path))
}

/// Removes the sidecar (and a leftover temp file). Missing files are not an error.
pub fn remove_resume_sidecar(output_path: &Path) -> std::io::Result<()> {
    remove_sidecar_file(&resume_sidecar_path(output_path)?)?;
    remove_sidecar_file(&legacy_sidecar_path(output_path))
}

fn remove_sidecar_file(sidecar: &Path) -> std::io::Result<()> {
    let mut tmp_name: OsString = sidecar.file_name().map(OsString::from).unwrap_or_default();
    tmp_name.push(".tmp");
    let _ = std::fs::remove_file(sidecar.with_file_name(tmp_name));
    match std::fs::remove_file(sidecar) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Checks that `state` describes the same remote resource as `probe`.
/// A matching strong ETag is mandatory. Secondary validators are compared when present
/// on both sides; absence cannot override an established primary identity.
pub(crate) fn validate(state: &ResumeState, probe: &DownloadProbe) -> Result<()> {
    if state.version != SIDECAR_VERSION {
        bail!("unsupported sidecar version {}", state.version);
    }
    if !probe.supports_ranges {
        bail!("server no longer supports range requests");
    }
    if state.total_bytes != probe.total_bytes {
        bail!(
            "remote size changed: sidecar {} bytes, server {} bytes",
            state.total_bytes,
            probe.total_bytes
        );
    }
    if state.chunk_size == 0 {
        bail!("sidecar chunk size is 0");
    }
    if crate::engine::strong_etag(state.etag.as_deref()).is_none()
        || crate::engine::strong_etag(probe.etag.as_deref()).is_none()
    {
        bail!("resume requires a strong ETag; old completed blocks cannot be trusted");
    }
    if state
        .resource_id
        .as_deref()
        .is_some_and(|url| url != resource_id(&probe.url))
    {
        bail!("resume resource URL changed");
    }
    if state.etag != probe.etag {
        bail!(
            "ETag changed: sidecar {:?}, server {:?}",
            state.etag,
            probe.etag
        );
    }
    if state.last_modified.is_some()
        && probe.last_modified.is_some()
        && state.last_modified != probe.last_modified
    {
        bail!(
            "Last-Modified changed: sidecar {:?}, server {:?}",
            state.last_modified,
            probe.last_modified
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(total: u64, etag: Option<&str>) -> DownloadProbe {
        DownloadProbe {
            url: "http://x/y".into(),
            total_bytes: total,
            supports_ranges: true,
            suggested_filename: "y".into(),
            etag: etag.map(str::to_string),
            last_modified: None,
            headers: None,
            header_origin: Some("http://x".into()),
        }
    }

    fn state(total: u64, etag: Option<&str>) -> ResumeState {
        ResumeState {
            version: SIDECAR_VERSION,
            resource_id: Some(resource_id("http://x/y")),
            file_identity: None,
            total_bytes: total,
            chunk_size: 10,
            etag: etag.map(str::to_string),
            last_modified: None,
            headers: None,
            header_origin: Some("http://x".into()),
            completed: String::new(),
        }
    }

    #[test]
    fn resume_paths_are_in_app_data_and_distinguish_output_directories() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let path = first.path().join("file.iso");
        let metadata = resume_sidecar_path(&path).unwrap();
        assert_eq!(metadata.parent(), Some(resume_dir().unwrap().as_path()));
        assert!(!metadata.starts_with(first.path()));
        assert_ne!(
            metadata,
            resume_sidecar_path(&second.path().join("file.iso")).unwrap()
        );
        assert_eq!(
            metadata,
            resume_sidecar_path(&first.path().join(".").join("file.iso")).unwrap()
        );
        std::fs::write(&path, b"data").unwrap();
        assert_eq!(metadata, resume_sidecar_path(&path).unwrap());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(metadata, resume_sidecar_path(&path).unwrap());
        #[cfg(windows)]
        assert_eq!(
            metadata.parent().unwrap(),
            PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("com.conflux.desktop/resume")
        );
    }

    #[test]
    fn legacy_resume_migrates_without_losing_completed_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("file.bin");
        let legacy = legacy_sidecar_path(&output);
        let metadata = resume_sidecar_path(&output).unwrap();
        let mut saved = state(100, Some("\"a\""));
        saved.completed = "0-3".into();
        write_sidecar(&legacy, &saved).unwrap();
        assert!(has_resume_sidecar(&output));
        assert_eq!(read_resume_state(&output).unwrap(), saved);
        write_resume_state(&output, &saved).unwrap();
        assert!(!legacy.exists());
        assert_eq!(read_resume_state(&output).unwrap(), saved);
        assert!(has_resume_sidecar(&output));
        remove_resume_sidecar(&output).unwrap();
        assert!(!metadata.exists());
        assert!(!has_resume_sidecar(&output));
    }

    #[test]
    fn app_data_takes_precedence_and_cleanup_removes_both_locations_and_temps() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("file.bin");
        let legacy = legacy_sidecar_path(&output);
        let metadata = resume_sidecar_path(&output).unwrap();
        let old = state(100, Some("old"));
        let mut current = old.clone();
        current.completed = "0-3".into();
        write_sidecar(&legacy, &old).unwrap();
        write_sidecar(&metadata, &current).unwrap();
        assert_eq!(read_resume_state(&output).unwrap(), current);
        let temp_paths: Vec<_> = [&legacy, &metadata]
            .iter()
            .map(|path| {
                let mut name = path.file_name().unwrap().to_os_string();
                name.push(".tmp");
                path.with_file_name(name)
            })
            .collect();
        for temp in &temp_paths {
            std::fs::write(temp, b"partial").unwrap();
        }
        remove_resume_sidecar(&output).unwrap();
        for path in [&legacy, &metadata].into_iter().chain(temp_paths.iter()) {
            assert!(!path.exists());
        }
        remove_resume_sidecar(&output).unwrap();
    }

    #[test]
    fn failed_migration_preserves_legacy_resume_data() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("file.bin");
        let legacy = legacy_sidecar_path(&output);
        let metadata = resume_sidecar_path(&output).unwrap();
        let saved = state(100, Some("old"));
        write_sidecar(&legacy, &saved).unwrap();
        std::fs::create_dir_all(&metadata).unwrap();
        assert!(write_resume_state(&output, &saved).is_err());
        assert_eq!(read_sidecar(&legacy).unwrap(), saved);
        // An unreadable current record must not fall back to stale legacy progress.
        assert!(read_resume_state(&output).is_err());
        std::fs::remove_dir(&metadata).unwrap();
        assert_eq!(read_resume_state(&output).unwrap(), saved);
        remove_resume_sidecar(&output).unwrap();
    }

    #[test]
    fn test_ranges_roundtrip() {
        assert_eq!(encode_ranges(&[]), "");
        assert_eq!(encode_ranges(&[3]), "3");
        assert_eq!(encode_ranges(&[9, 0, 1, 2, 5, 7, 8, 2]), "0-2,5,7-9");
        for ids in [
            vec![],
            vec![0],
            vec![0, 1, 2, 5, 7, 8, 9],
            (0..1000).collect(),
        ] {
            assert_eq!(decode_ranges(&encode_ranges(&ids), 1000).unwrap(), ids);
        }
    }

    #[test]
    fn test_decode_rejects_bad_input() {
        assert!(decode_ranges("0-10", 10).is_err());
        assert!(decode_ranges("5-3", 10).is_err());
        assert!(decode_ranges("a", 10).is_err());
        assert!(decode_ranges("1-", 10).is_err());
    }

    #[test]
    fn test_validate_requires_strong_identity() {
        let strong = Some("\"a\"");
        assert!(validate(&state(100, strong), &probe(100, strong)).is_ok());
        for weak in [None, Some("W/\"a\""), Some("a"), Some("")] {
            assert!(validate(&state(100, weak), &probe(100, weak)).is_err());
        }
        assert!(validate(&state(100, strong), &probe(100, Some("\"b\""))).is_err());
        assert!(validate(&state(100, strong), &probe(101, strong)).is_err());
        let mut saved = state(100, strong);
        saved.last_modified = Some("Mon".into());
        // Missing secondary headers do not override a matching strong ETag.
        assert!(validate(&saved, &probe(100, strong)).is_ok());
        let mut fresh = probe(100, strong);
        fresh.last_modified = Some("Tue".into());
        assert!(validate(&saved, &fresh).is_err());
        fresh = probe(100, strong);
        fresh.url = "http://x/other".into();
        assert!(validate(&saved, &fresh).is_err());
        fresh = probe(100, strong);
        fresh.supports_ranges = false;
        assert!(validate(&saved, &fresh).is_err());
        saved.version = 0;
        assert!(validate(&saved, &probe(100, strong)).is_err());
    }

    #[test]
    fn restored_headers_never_include_cookies_and_keep_authorized_origin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("download.bin");
        let mut saved = state(100, Some("\"a\""));
        // A cross-origin redirect changed the final resource URL, not the original header authorization.
        saved.resource_id = Some(resource_id("http://cdn/target"));
        saved.headers = Some(RequestHeaders {
            cookie: Some("secret".into()),
            referer: Some("http://portal/page".into()),
            user_agent: Some("Browser".into()),
        });
        write_sidecar(&resume_sidecar_path(&path).unwrap(), &saved).unwrap();
        let same = read_resume_headers(&path, "http://x/original")
            .unwrap()
            .unwrap();
        assert_eq!(same.cookie, None);
        assert_eq!(same.referer.as_deref(), Some("http://portal/page"));
        let other = read_resume_headers(&path, "http://other/download")
            .unwrap()
            .unwrap();
        assert_eq!(other.cookie, None);
        assert_eq!(other.referer, None);
        assert_eq!(other.user_agent.as_deref(), Some("Browser"));
    }

    #[test]
    fn test_write_read_remove_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("f.bin");
        let path = resume_sidecar_path(&output).unwrap();
        let mut s = state(100, Some("e"));
        s.completed = "0-3".into();
        write_sidecar(&path, &s).unwrap();
        assert_eq!(read_sidecar(&path).unwrap(), s);
        s.completed = "0-4".into();
        write_sidecar(&path, &s).unwrap();
        assert_eq!(read_sidecar(&path).unwrap(), s);
        remove_resume_sidecar(&output).unwrap();
        assert!(!path.exists());
        remove_resume_sidecar(&output).unwrap();
    }

    #[test]
    fn test_resume_state_headers_serde_backwards_compatible() {
        // Old sidecar JSON without headers field must deserialize with headers = None
        let legacy_json = r#"{
            "version": 1,
            "total_bytes": 1024,
            "chunk_size": 256,
            "etag": "\"123\"",
            "last_modified": null,
            "completed": "0-2"
        }"#;
        let decoded: ResumeState = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(decoded.headers, None);
        assert_eq!(decoded.total_bytes, 1024);

        // New sidecar with headers must round-trip cleanly
        let mut state_with_headers = state(1024, Some("\"123\""));
        state_with_headers.headers = Some(RequestHeaders {
            cookie: Some("auth_token=xyz123".into()),
            referer: Some("https://example.com/download".into()),
            user_agent: Some("CustomAgent/1.0".into()),
        });
        let serialized = serde_json::to_string(&state_with_headers).unwrap();
        let deserialized: ResumeState = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, state_with_headers);
    }
}
