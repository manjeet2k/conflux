//! Resume sidecar: `<output>.conflux.json` records which chunks of a chunked download are
//! durably on disk, so a stopped or failed download can continue instead of starting over.
//!
//! Durability rule: the data file is synced *before* a sidecar listing new chunks is
//! written, and the sidecar is replaced atomically (write temp file, sync, rename). So a
//! chunk listed as completed is always really on disk, even after a crash or power loss.

use crate::engine::DownloadProbe;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SIDECAR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeState {
    pub version: u32,
    pub total_bytes: u64,
    pub chunk_size: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Completed chunk ids as ascending ranges, e.g. `"0-5,7,9-12"` (empty = none).
    pub completed: String,
}

/// `<dir>/file.iso` -> `<dir>/file.iso.conflux.json`.
pub fn resume_sidecar_path(output_path: &Path) -> PathBuf {
    let mut name: OsString = output_path
        .file_name()
        .map(OsString::from)
        .unwrap_or_default();
    name.push(".conflux.json");
    output_path.with_file_name(name)
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

/// Atomically replaces the sidecar at `path` (temp file + sync + rename).
pub(crate) fn write_sidecar(path: &Path, state: &ResumeState) -> Result<()> {
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
    let sidecar = resume_sidecar_path(output_path);
    let mut tmp_name: OsString = sidecar.file_name().map(OsString::from).unwrap_or_default();
    tmp_name.push(".tmp");
    let _ = std::fs::remove_file(sidecar.with_file_name(tmp_name));
    match std::fs::remove_file(&sidecar) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Checks that `state` describes the same remote resource as `probe`.
/// Validators (ETag / Last-Modified) must be equal, including being absent on both sides:
/// one that appeared or disappeared means the server can no longer vouch for the old bytes.
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
    if state.etag != probe.etag {
        bail!(
            "ETag changed: sidecar {:?}, server {:?}",
            state.etag,
            probe.etag
        );
    }
    if state.last_modified != probe.last_modified {
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
        }
    }

    fn state(total: u64, etag: Option<&str>) -> ResumeState {
        ResumeState {
            version: SIDECAR_VERSION,
            total_bytes: total,
            chunk_size: 10,
            etag: etag.map(str::to_string),
            last_modified: None,
            completed: String::new(),
        }
    }

    #[test]
    fn test_sidecar_path_appends_suffix() {
        assert_eq!(
            resume_sidecar_path(Path::new("/tmp/a/file.iso")),
            PathBuf::from("/tmp/a/file.iso.conflux.json")
        );
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
    fn test_validate() {
        assert!(validate(&state(100, None), &probe(100, None)).is_ok());
        assert!(validate(&state(100, Some("a")), &probe(100, Some("a"))).is_ok());
        assert!(validate(&state(100, Some("a")), &probe(100, Some("b"))).is_err());
        // A validator present on only one side is a change too.
        assert!(validate(&state(100, Some("a")), &probe(100, None)).is_err());
        assert!(validate(&state(100, None), &probe(100, Some("a"))).is_err());
        let mut with_lm = state(100, None);
        with_lm.last_modified = Some("Mon".into());
        assert!(validate(&with_lm, &probe(100, None)).is_err());
        let mut probe_lm = probe(100, None);
        probe_lm.last_modified = Some("Mon".into());
        assert!(validate(&with_lm, &probe_lm).is_ok());
        assert!(validate(&state(100, None), &probe_lm).is_err());
        assert!(validate(&state(100, None), &probe(101, None)).is_err());
        let mut no_ranges = probe(100, None);
        no_ranges.supports_ranges = false;
        assert!(validate(&state(100, None), &no_ranges).is_err());
        let mut old = state(100, None);
        old.version = 0;
        assert!(validate(&old, &probe(100, None)).is_err());
    }

    #[test]
    fn test_write_read_remove_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("f.bin");
        let path = resume_sidecar_path(&output);
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
}
