use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// Computes the SHA-256 hex digest of a file at `path`.
pub async fn compute_sha256<P: AsRef<Path>>(path: P) -> Result<String> {
    let mut file = File::open(path.as_ref()).await.with_context(|| {
        format!(
            "Failed to open file for SHA-256 computation: {:?}",
            path.as_ref()
        )
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024]; // 64 KB buffer

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .await
            .context("Failed reading file chunk")?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_compute_sha256() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let mut file = File::create(temp.path()).await?;
        file.write_all(b"Hello Conflux!").await?;
        file.flush().await?;

        let hash = compute_sha256(temp.path()).await?;
        // echo -n "Hello Conflux!" | sha256sum
        // 03417769e6b509f6f69165d496a7935be0ddff175d27d6ef625b591b65e08b68
        let mut hasher = Sha256::new();
        hasher.update(b"Hello Conflux!");
        let expected = format!("{:x}", hasher.finalize());

        assert_eq!(hash, expected);
        Ok(())
    }
}
