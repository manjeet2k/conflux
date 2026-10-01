//! Std-only helpers for data files: UTC timestamps and quarantining of corrupt files.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many `<file>.corrupt-<timestamp>` backups are kept per data file.
pub const MAX_CORRUPT_BACKUPS: usize = 3;

/// `YYYYMMDD-HHMMSS` in UTC for `unix_secs` (civil-from-days, no time-zone database needed).
pub fn utc_stamp_from_secs(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// Current UTC time as `YYYYMMDD-HHMMSS`.
pub fn utc_stamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    utc_stamp_from_secs(secs)
}

/// `downloads.json` + `20260101-000000` -> `downloads.json.corrupt-20260101-000000`.
pub fn corrupt_backup_name(file_name: &str, stamp: &str) -> String {
    format!("{file_name}.corrupt-{stamp}")
}

/// Deletes the oldest `<file_name>.corrupt-*` siblings of `path` beyond `keep`. Timestamps
/// sort lexicographically, so name order is age order. Best effort.
pub fn prune_corrupt_backups(path: &Path, keep: usize) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return;
    };
    let prefix = format!("{name}.corrupt-");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut backups: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(&prefix))
        .collect();
    backups.sort();
    let excess = backups.len().saturating_sub(keep);
    for old in backups.into_iter().take(excess) {
        let _ = std::fs::remove_file(dir.join(old));
    }
}

/// Renames the unreadable data file at `path` to `<file>.corrupt-<stamp>` (never overwriting
/// an existing backup) and prunes old backups. Returns the backup path.
pub fn quarantine_corrupt(path: &Path, stamp: &str) -> std::io::Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| std::io::Error::other("data file has no name"))?;
    let base = corrupt_backup_name(name, stamp);
    let mut target = path.with_file_name(&base);
    let mut n = 1;
    while target.exists() {
        target = path.with_file_name(format!("{base}-{n}"));
        n += 1;
    }
    std::fs::rename(path, &target)?;
    prune_corrupt_backups(path, MAX_CORRUPT_BACKUPS);
    Ok(target)
}

/// Quarantines the unreadable data file `path` and returns a one-line, path-free notice for
/// the UI ("<what> was unreadable ... kept as <backup name>").
pub fn quarantine_with_notice(path: &Path, what: &str) -> String {
    match quarantine_corrupt(path, &utc_stamp_now()) {
        Ok(backup) => {
            let name = backup
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            format!("Your {what} could not be read and was reset. The damaged file was kept as {name} in the app data folder.")
        }
        Err(_) => format!("Your {what} could not be read and was reset."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "conflux-fileutil-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn stamp_known_values() {
        assert_eq!(utc_stamp_from_secs(0), "19700101-000000");
        assert_eq!(utc_stamp_from_secs(951_782_400), "20000229-000000");
        assert_eq!(utc_stamp_from_secs(1_700_000_000), "20231114-221320");
    }

    #[test]
    fn backup_name_format() {
        assert_eq!(
            corrupt_backup_name("downloads.json", "20260101-000000"),
            "downloads.json.corrupt-20260101-000000"
        );
    }

    #[test]
    fn quarantine_moves_file_and_keeps_three() {
        let dir = tmp();
        let path = dir.join("settings.json");
        for i in 0..5 {
            std::fs::write(&path, format!("garbage {i}")).unwrap();
            let moved = quarantine_corrupt(&path, &format!("2026010{i}-000000")).unwrap();
            assert!(moved.exists());
            assert!(!path.exists(), "original must be moved away");
        }
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "settings.json.corrupt-20260102-000000",
                "settings.json.corrupt-20260103-000000",
                "settings.json.corrupt-20260104-000000",
            ]
        );
        // The newest backup holds the newest content.
        let newest = std::fs::read_to_string(dir.join(&names[2])).unwrap();
        assert_eq!(newest, "garbage 4");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn notice_names_backup_without_directory() {
        let dir = tmp();
        let path = dir.join("downloads.json");
        std::fs::write(&path, "x").unwrap();
        let notice = quarantine_with_notice(&path, "download history");
        assert!(notice.contains("download history"));
        assert!(notice.contains("downloads.json.corrupt-"));
        assert!(!notice.contains(dir.to_string_lossy().as_ref()));
        assert!(!path.exists());
        // Nothing to move: still a notice, never a panic.
        assert!(quarantine_with_notice(&path, "download history").contains("reset"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn same_second_collision_does_not_overwrite() {
        let dir = tmp();
        let path = dir.join("a.json");
        std::fs::write(&path, "one").unwrap();
        let first = quarantine_corrupt(&path, "20260101-000000").unwrap();
        std::fs::write(&path, "two").unwrap();
        let second = quarantine_corrupt(&path, "20260101-000000").unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(first).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(second).unwrap(), "two");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
