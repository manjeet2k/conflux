use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Pre-allocated output file that accepts positional writes from many concurrent workers.
///
/// Each write is a single positional `pwrite`-style call (`write_all_at` on unix,
/// `seek_write` loop on Windows) executed on the blocking thread pool. There is no shared
/// cursor, so no mutex is needed and every I/O error is returned to the exact caller
/// (and therefore chunk) that issued the write.
#[derive(Clone)]
pub struct SparseFileWriter {
    path: PathBuf,
    total_bytes: u64,
    file: Arc<File>,
}

impl SparseFileWriter {
    /// Creates (truncating) a file at `path`, immediately pre-allocating its total size to `total_bytes`.
    pub async fn create<P: AsRef<Path>>(path: P, total_bytes: u64) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        if let Some(parent) = path_buf.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await.with_context(|| {
                    format!("Failed to create parent directory for {:?}", path_buf)
                })?;
            }
        }

        let open_path = path_buf.clone();
        let file = tokio::task::spawn_blocking(move || -> Result<File> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&open_path)
                .with_context(|| format!("Failed to create output file {:?}", open_path))?;
            // Pre-allocate the complete file size. On Windows this sets EndOfFile; on Linux it calls ftruncate.
            file.set_len(total_bytes).with_context(|| {
                format!(
                    "Failed to pre-allocate {} bytes for {:?}",
                    total_bytes, open_path
                )
            })?;
            Ok(file)
        })
        .await
        .context("File creation task panicked")??;

        Ok(Self {
            path: path_buf,
            total_bytes,
            file: Arc::new(file),
        })
    }

    /// Opens an existing file for resuming, without truncating it.
    ///
    /// Fails if the file does not exist or its length is not exactly `total_bytes`
    /// (a different length means it is not the pre-allocated file of this download).
    pub async fn open_existing<P: AsRef<Path>>(path: P, total_bytes: u64) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let open_path = path_buf.clone();
        let file = tokio::task::spawn_blocking(move || -> Result<File> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&open_path)
                .with_context(|| format!("Failed to open existing file {:?}", open_path))?;
            let len = file
                .metadata()
                .with_context(|| format!("Failed to read metadata of {:?}", open_path))?
                .len();
            if len != total_bytes {
                bail!(
                    "Existing file {:?} is {} bytes, expected {}",
                    open_path,
                    len,
                    total_bytes
                );
            }
            Ok(file)
        })
        .await
        .context("File open task panicked")??;

        Ok(Self {
            path: path_buf,
            total_bytes,
            file: Arc::new(file),
        })
    }

    /// Writes `data` at `offset` without altering other regions.
    ///
    /// Fails (without writing) if the write would extend past `total_bytes`.
    pub async fn write_at(&self, offset: u64, data: impl Into<Vec<u8>>) -> Result<()> {
        let data: Vec<u8> = data.into();
        let len = data.len() as u64;
        let end = offset.checked_add(len);
        if end.is_none_or(|end| end > self.total_bytes) {
            bail!(
                "Refusing to write {} bytes at offset {}: exceeds file size {} of {:?}",
                len,
                offset,
                self.total_bytes,
                self.path
            );
        }

        let file = Arc::clone(&self.file);
        tokio::task::spawn_blocking(move || write_all_at(&file, &data, offset))
            .await
            .context("Positional write task panicked")?
            .with_context(|| {
                format!(
                    "Failed to write {} bytes at offset {} in {:?}",
                    len, offset, self.path
                )
            })
    }

    /// Flushes and syncs OS buffers to disk.
    pub async fn sync(&self) -> Result<()> {
        let file = Arc::clone(&self.file);
        tokio::task::spawn_blocking(move || file.sync_all())
            .await
            .context("Sync task panicked")?
            .context("Failed to sync file to disk")
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(buf, offset)
}

#[cfg(windows)]
fn write_all_at(file: &File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_write(buf, offset) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ))
            }
            Ok(n) => {
                buf = &buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sparse_file_writer_out_of_order() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_sparse.bin");
        let total_size = 12;

        let writer = SparseFileWriter::create(&file_path, total_size).await?;
        assert_eq!(tokio::fs::metadata(&file_path).await?.len(), total_size);

        writer.write_at(4, b"EFGH").await?;
        writer.write_at(8, b"IJKL").await?;
        writer.write_at(0, b"ABCD").await?;
        writer.sync().await?;

        let contents = tokio::fs::read(&file_path).await?;
        assert_eq!(contents, b"ABCDEFGHIJKL");
        Ok(())
    }

    #[tokio::test]
    async fn test_sparse_file_writer_rejects_overrun() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("bounded.bin");
        let writer = SparseFileWriter::create(&file_path, 8).await?;
        assert!(writer.write_at(6, b"XYZ").await.is_err());
        assert!(writer.write_at(u64::MAX, b"X").await.is_err());
        writer.write_at(5, b"XYZ").await?;
        assert_eq!(tokio::fs::metadata(&file_path).await?.len(), 8);
        Ok(())
    }

    #[tokio::test]
    async fn test_sparse_file_writer_truncates_existing() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("existing.bin");
        std::fs::write(&file_path, vec![0xAA; 100])?;
        let _writer = SparseFileWriter::create(&file_path, 10).await?;
        assert_eq!(std::fs::read(&file_path)?, vec![0u8; 10]);
        Ok(())
    }

    /// Many concurrent tasks writing interleaved small pieces at distinct offsets must
    /// produce exactly the expected byte pattern (no lost or misplaced writes).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_sparse_file_writer_concurrent_positional_writes() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("concurrent.bin");
        let pieces: u64 = 64;
        let piece_len: u64 = 4099; // deliberately not a power of two
        let total = pieces * piece_len;
        let writer = SparseFileWriter::create(&file_path, total).await?;

        let expected: Vec<u8> = (0..total)
            .map(|i| (i.wrapping_mul(31) % 251) as u8)
            .collect();
        let expected = Arc::new(expected);

        let mut handles = Vec::new();
        for task in 0..8u64 {
            let writer = writer.clone();
            let expected = Arc::clone(&expected);
            handles.push(tokio::spawn(async move {
                // Each task writes every 8th piece, in reverse order, split in two halves.
                let mut piece = pieces - 1 - task;
                loop {
                    let start = piece * piece_len;
                    let mid = start + piece_len / 2;
                    let end = start + piece_len;
                    writer
                        .write_at(mid, &expected[mid as usize..end as usize])
                        .await?;
                    writer
                        .write_at(start, &expected[start as usize..mid as usize])
                        .await?;
                    if piece < 8 {
                        break;
                    }
                    piece -= 8;
                }
                anyhow::Ok(())
            }));
        }
        for h in handles {
            h.await??;
        }
        writer.sync().await?;

        let contents = tokio::fs::read(&file_path).await?;
        assert_eq!(contents.len() as u64, total);
        assert!(contents == *expected, "file contents differ from expected");
        Ok(())
    }

    #[tokio::test]
    async fn test_open_existing_keeps_contents_and_checks_length() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("resume.bin");
        std::fs::write(&file_path, b"ABCDEFGH")?;

        let writer = SparseFileWriter::open_existing(&file_path, 8).await?;
        writer.write_at(2, b"xy").await?;
        writer.sync().await?;
        assert_eq!(std::fs::read(&file_path)?, b"ABxyEFGH");

        assert!(SparseFileWriter::open_existing(&file_path, 9)
            .await
            .is_err());
        assert!(
            SparseFileWriter::open_existing(dir.path().join("missing.bin"), 8)
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(&file_path)?,
            b"ABxyEFGH",
            "failed opens must not modify"
        );
        Ok(())
    }
}
