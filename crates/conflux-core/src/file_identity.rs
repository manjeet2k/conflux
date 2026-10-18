//! File ownership is identified by the open file, never by its name or length.
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub volume: u64,
    pub file: u64,
}

#[derive(Debug)]
pub struct FileOwnershipChanged;
impl std::fmt::Display for FileOwnershipChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("file ownership changed; output preserved")
    }
}
impl std::error::Error for FileOwnershipChanged {}

impl FileIdentity {
    pub fn of(file: &File) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            };
            let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
            // SAFETY: file owns the live handle and info is writable for the call.
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let info = unsafe { info.assume_init() };
            Ok(Self {
                volume: u64::from(info.dwVolumeSerialNumber),
                file: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            })
        }
        #[cfg(target_os = "linux")]
        {
            // Linux exists only for the core test host.
            use std::os::unix::fs::MetadataExt;
            let metadata = file.metadata()?;
            Ok(Self {
                volume: metadata.dev(),
                file: metadata.ino(),
            })
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = file;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "file identity unavailable",
            ))
        }
    }

    pub fn at(path: &Path) -> io::Result<Self> {
        Self::of(&File::open(path)?)
    }
}

/// Renames the verified source without replacing anything at the destination.
/// On Windows the rename uses the same handle as the ownership check, so replacing
/// the source path between a check and a path-based rename cannot publish a stranger's file.
pub fn rename_owned_file(source: &Path, target: &Path, expected: &FileIdentity) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
        };
        options
            .access_mode(DELETE | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ);
    }
    let file = options.open(source)?;
    if FileIdentity::of(&file)? != *expected {
        return Err(io::Error::other(FileOwnershipChanged));
    }
    #[cfg(windows)]
    {
        use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            FileRenameInfo, SetFileInformationByHandle, FILE_RENAME_INFO,
        };
        let name: Vec<u16> = std::path::absolute(target)?
            .as_os_str()
            .encode_wide()
            .collect();
        if name.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NUL in destination path",
            ));
        }
        let name_bytes = u32::try_from(name.len() * 2).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination path too long")
        })?;
        let size = std::mem::size_of::<FILE_RENAME_INFO>() + name_bytes as usize;
        // usize storage gives FILE_RENAME_INFO its required pointer alignment. The
        // zeroed union leaves ReplaceIfExists=false; the extra word holds a terminator.
        let mut storage = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
        let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        // SAFETY: aligned writable allocation includes the header and the complete UTF-16
        // name; the live handle has DELETE access. The API is synchronous.
        let ok = unsafe {
            (*info).FileNameLength = name_bytes;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                name.len(),
            );
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileRenameInfo,
                info.cast(),
                size as u32,
            )
        };
        if ok == 0 {
            let error = io::Error::last_os_error();
            // A directory collision may surface as ACCESS_DENIED rather than EXISTS.
            if matches!(error.raw_os_error(), Some(5 | 80 | 183))
                && std::fs::symlink_metadata(target).is_ok()
            {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, error));
            }
            return Err(error);
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let path = |p: &Path| {
            CString::new(p.as_os_str().as_bytes())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))
        };
        let source = path(source)?;
        let target = path(target)?;
        // Test-host equivalent of Windows' no-replace rename. No Linux packaging or UI.
        // SAFETY: both C strings are valid for the duration of the synchronous call.
        if unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = target;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "file publishing unavailable",
        ))
    }
}

/// Scanner handles can temporarily deny DELETE access after a completed file closes.
/// Retry those Windows sharing/access errors for about four seconds, retaining the
/// source on failure. Collisions return immediately for the caller to choose a new name.
pub async fn publish_owned_file(
    source: &Path,
    target: &Path,
    expected: &FileIdentity,
) -> io::Result<()> {
    for attempt in 0..10 {
        let source = source.to_path_buf();
        let target = target.to_path_buf();
        let expected = expected.clone();
        let result =
            tokio::task::spawn_blocking(move || rename_owned_file(&source, &target, &expected))
                .await
                .map_err(io::Error::other)?;
        match result {
            Err(ref error)
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && attempt < 9 =>
            {
                tokio::time::sleep(std::time::Duration::from_millis((100 << attempt).min(500)))
                    .await;
            }
            result => return result,
        }
    }
    unreachable!("the last attempt always returns")
}

/// Deletes only the file whose handle matches the recorded identity.
/// On Windows exclusive sharing prevents path replacement during validation/deletion.
pub fn remove_owned_file(path: &Path, expected: &FileIdentity) -> io::Result<bool> {
    remove_owned_file_with_length(path, expected, None)
}

pub fn remove_owned_empty_file(path: &Path, expected: &FileIdentity) -> io::Result<bool> {
    remove_owned_file_with_length(path, expected, Some(0))
}

fn remove_owned_file_with_length(
    path: &Path,
    expected: &FileIdentity,
    length: Option<u64>,
) -> io::Result<bool> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES};
        options
            .access_mode(DELETE | FILE_READ_ATTRIBUTES)
            .share_mode(0);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if FileIdentity::of(&file)? != *expected
        || length.is_some_and(|length| {
            file.metadata()
                .map_or(true, |metadata| metadata.len() != length)
        })
    {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
        };
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: valid exclusive handle with DELETE access and correctly sized input.
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of_val(&disposition) as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        drop(file);
    }
    #[cfg(not(windows))]
    {
        // Positional-write/ownership regression tests only; Windows is the shipped platform.
        std::fs::remove_file(path)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_preserves_identity_and_never_replaces_a_collision() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("résumé-資料.exe.part");
        let target = dir.path().join("résumé-資料.exe");
        std::fs::write(&source, b"complete").unwrap();
        let identity = FileIdentity::at(&source).unwrap();
        std::fs::write(&target, b"user's file").unwrap();
        assert_eq!(
            publish_owned_file(&source, &target, &identity)
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"user's file");
        assert_eq!(std::fs::read(&source).unwrap(), b"complete");
        std::fs::remove_file(&target).unwrap();
        publish_owned_file(&source, &target, &identity)
            .await
            .unwrap();
        assert_eq!(FileIdentity::at(&target).unwrap(), identity);
        assert!(!source.exists());
    }

    #[test]
    fn publish_refuses_a_replaced_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("file.bin.part");
        std::fs::write(&source, b"our file").unwrap();
        let identity = FileIdentity::at(&source).unwrap();
        std::fs::rename(&source, dir.path().join("original")).unwrap();
        std::fs::write(&source, b"replacement").unwrap();
        let target = dir.path().join("file.bin");
        assert!(rename_owned_file(&source, &target, &identity).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"replacement");
        assert!(!target.exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn publish_retries_until_scanner_releases_its_handle() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("scanned.exe.part");
        let target = dir.path().join("scanned.exe");
        std::fs::write(&source, b"complete").unwrap();
        let identity = FileIdentity::at(&source).unwrap();
        let scanner = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&source)
            .unwrap();
        let release = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            drop(scanner);
        });
        publish_owned_file(&source, &target, &identity)
            .await
            .unwrap();
        release.await.unwrap();
        assert_eq!(FileIdentity::at(&target).unwrap(), identity);
        assert!(!source.exists());
    }

    #[test]
    fn replacement_of_same_size_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("download.bin");
        std::fs::write(&path, b"partial").unwrap();
        let identity = FileIdentity::at(&path).unwrap();
        std::fs::rename(&path, dir.path().join("old.bin")).unwrap();
        std::fs::write(&path, b"replace").unwrap();
        assert!(!remove_owned_file(&path, &identity).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"replace");
    }

    #[test]
    fn owned_file_can_be_deleted_after_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("download.bin");
        std::fs::write(&path, []).unwrap();
        let identity = FileIdentity::at(&path).unwrap();
        std::fs::write(&path, b"partial").unwrap();
        assert_eq!(FileIdentity::at(&path).unwrap(), identity);
        assert!(remove_owned_file(&path, &identity).unwrap());
        assert!(!path.exists());
        assert!(!remove_owned_file(&path, &identity).unwrap());
    }
}
