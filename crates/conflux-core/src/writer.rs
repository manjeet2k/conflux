use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SparseFileWriter {
    path: PathBuf,
    total_bytes: u64,
    file: Arc<Mutex<File>>,
}

impl SparseFileWriter {
    /// Creates or opens a file at `path`, immediately pre-allocating its total size to `total_bytes`.
    pub async fn create<P: AsRef<Path>>(path: P, total_bytes: u64) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        if let Some(parent) = path_buf.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create parent directory for {:?}", path_buf))?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path_buf)
            .await
            .with_context(|| format!("Failed to create output file {:?}", path_buf))?;

        // Pre-allocate the complete file size. On Windows this sets EndOfFile; on Linux it calls ftruncate.
        file.set_len(total_bytes).await.with_context(|| {
            format!(
                "Failed to pre-allocate {} bytes for {:?}",
                total_bytes, path_buf
            )
        })?;

        Ok(Self {
            path: path_buf,
            total_bytes,
            file: Arc::new(Mutex::new(file)),
        })
    }

    /// Writes `data` directly at the specified `offset` without altering other regions.
    pub async fn write_at(&self, offset: u64, data: &[u8]) -> Result<()> {
        let mut handle = self.file.lock().await;
        handle
            .seek(SeekFrom::Start(offset))
            .await
            .with_context(|| format!("Failed to seek to offset {} in {:?}", offset, self.path))?;
        handle.write_all(data).await.with_context(|| {
            format!(
                "Failed to write {} bytes at offset {} in {:?}",
                data.len(),
                offset,
                self.path
            )
        })?;
        Ok(())
    }

    /// Flushes and syncs OS buffers to disk.
    pub async fn sync(&self) -> Result<()> {
        let handle = self.file.lock().await;
        handle
            .sync_all()
            .await
            .context("Failed to sync file to disk")?;
        Ok(())
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
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

        // Write chunk 2 (middle): "EFGH" at offset 4
        writer.write_at(4, b"EFGH").await?;

        // Write chunk 3 (end): "IJKL" at offset 8
        writer.write_at(8, b"IJKL").await?;

        // Write chunk 1 (start): "ABCD" at offset 0
        writer.write_at(0, b"ABCD").await?;

        writer.sync().await?;

        let contents = tokio::fs::read(&file_path).await?;
        assert_eq!(contents, b"ABCDEFGHIJKL");

        Ok(())
    }
}
