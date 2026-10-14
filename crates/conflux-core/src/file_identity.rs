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
