use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::watch;

/// Computes the SHA-256 hex digest of a file at `path`.
pub async fn compute_sha256<P: AsRef<Path>>(path: P) -> Result<String> {
    // The sender is kept alive so the receiver never reports "closed"; it never fires.
    let (_keep, mut never) = watch::channel(false);
    Ok(compute_sha256_cancellable(path, &mut never, Duration::ZERO)
        .await?
        .expect("an unfired cancel channel never cancels"))
}

/// Like [`compute_sha256`], but stops as soon as `cancel` holds `true` and returns
/// `Ok(None)`. `throttle` is slept after each 64 KiB read (test hook; use `Duration::ZERO`).
pub async fn compute_sha256_cancellable<P: AsRef<Path>>(
    path: P,
    cancel: &mut watch::Receiver<bool>,
    throttle: Duration,
) -> Result<Option<String>> {
    let mut file = File::open(path.as_ref()).await.with_context(|| {
        format!(
            "Failed to open file for SHA-256 computation: {:?}",
            path.as_ref()
        )
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024]; // 64 KB buffer

    loop {
        if *cancel.borrow() {
            return Ok(None);
        }
        let bytes_read = file
            .read(&mut buffer)
            .await
            .context("Failed reading file chunk")?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        if !throttle.is_zero() {
            tokio::time::sleep(throttle).await;
        }
    }

    let result = hasher.finalize();
    Ok(Some(format!("{:x}", result)))
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

    #[tokio::test]
    async fn test_compute_sha256_cancelled_returns_none() -> Result<()> {
        let temp = NamedTempFile::new()?;
        std::fs::write(temp.path(), vec![7u8; 300_000])?;
        let (tx, mut rx) = watch::channel(true);
        assert_eq!(
            compute_sha256_cancellable(temp.path(), &mut rx, Duration::ZERO).await?,
            None
        );
        tx.send(false)?;
        assert!(
            compute_sha256_cancellable(temp.path(), &mut rx, Duration::ZERO)
                .await?
                .is_some()
        );
        Ok(())
    }
}
