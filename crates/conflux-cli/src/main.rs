use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use conflux_core::{
    claim_unique_path, discover_adapters, sanitize_filename, DownloadCancelled, DownloadEngine,
    ProgressUpdate,
};
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, watch};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "conflux")]
#[command(about = "Next-Gen Multi-Interface & Channel-Bonding Download Accelerator", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all discovered network adapters and their IP bindings
    Adapters,

    /// Probe a remote URL to inspect file size and Range support
    Probe {
        /// Remote URL to inspect
        url: String,
    },

    /// Download a file aggregating across all active network interfaces
    Download {
        /// URL of the file to download
        url: String,

        /// Destination file path (overwritten if it exists), or a directory: an existing one,
        /// or any path ending in a separator (e.g. `newdir/`), which is created if missing.
        /// Without it, or with a directory, the file is saved there under the server-suggested
        /// name, auto-renamed to "name (1).ext" etc. if that name is taken.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Chunk size in megabytes (1-1024, default: 4 MB)
        #[arg(short = 's', long, default_value_t = 4, value_parser = clap::value_parser!(u64).range(1..=1024))]
        chunk_size_mb: u64,

        /// Number of concurrent connection workers per network adapter (1-64)
        #[arg(short, long, default_value_t = 4, value_parser = clap::value_parser!(u16).range(1..=64))]
        connections: u16,

        /// Only use adapters matching this name or IP substring (e.g. "WiFi" or "Ethernet")
        #[arg(short = 'a', long)]
        adapter: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Adapters => {
            println!("🔍 Inspecting available network adapters...");
            let adapters = discover_adapters()?;
            if adapters.is_empty() {
                println!("No network adapters discovered.");
            } else {
                println!(
                    "\n{:<30} {:<20} {:<10} {:<10}",
                    "ADAPTER ID", "IP ADDRESS", "IPV4", "ACTIVE"
                );
                println!("{:-<75}", "");
                for a in adapters {
                    println!(
                        "{:<30} {:<20} {:<10} {:<10}",
                        a.id,
                        a.ip,
                        if a.is_ipv4 { "Yes" } else { "No" },
                        if a.enabled {
                            "🟢 Ready"
                        } else {
                            "⚪ Ignored"
                        }
                    );
                }
            }
        }

        Commands::Probe { url } => {
            println!("🔍 Probing remote endpoint: {}", url);
            validate_url(&url)?;
            let engine = DownloadEngine::default();
            let probe = engine.probe(&url).await?;

            println!("\n📦 Probe Results:");
            println!("  Filename:        {}", probe.suggested_filename);
            println!(
                "  Size:            {:.2} MB ({} bytes)",
                probe.total_bytes as f64 / (1024.0 * 1024.0),
                probe.total_bytes
            );
            println!(
                "  Range support:   {}",
                if probe.supports_ranges {
                    "✅ Supported"
                } else {
                    "❌ Not supported"
                }
            );
        }

        Commands::Download {
            url,
            output,
            chunk_size_mb,
            connections,
            adapter,
        } => {
            validate_url(&url)?;
            let chunk_size = chunk_size_mb
                .checked_mul(1024 * 1024)
                .context("chunk size overflow")?;
            let engine = DownloadEngine::new(chunk_size, usize::from(connections))?;
            let probe = engine.probe(&url).await?;

            // Fallible setup first: resolve_output_path leaves an empty placeholder file.
            let mut adapters = discover_adapters()?;
            if let Some(ref filter) = adapter {
                let needle = filter.to_lowercase();
                adapters.retain(|a| {
                    a.id.to_lowercase().contains(&needle) || a.ip.to_string().contains(filter)
                });
                if adapters.is_empty() {
                    bail!("No network adapters matched filter {:?}", filter);
                }
            }
            let final_path = resolve_output_path(output.as_deref(), &probe.suggested_filename)?;

            println!("⚡ Conflux Download Starting");
            println!("  Target:     {}", probe.suggested_filename);
            if probe.total_bytes > 0 {
                println!(
                    "  Total Size: {:.2} MB",
                    probe.total_bytes as f64 / (1024.0 * 1024.0)
                );
            } else {
                println!("  Total Size: unknown or empty");
            }
            println!(
                "  Mode:       {}",
                if probe.supports_ranges && probe.total_bytes > 0 {
                    "multi-chunk"
                } else {
                    "single-stream"
                }
            );
            println!("  Output:     {:?}", final_path);

            let active_count = adapters
                .iter()
                .filter(|a| a.enabled && !a.is_loopback && a.is_ipv4)
                .count();
            println!(
                "  Bonding:    {} active network adapter(s)",
                active_count.max(1)
            );

            // Ctrl-C flips the cancel flag; the engine stops workers and returns DownloadCancelled.
            let (cancel_tx, cancel_rx) = watch::channel(false);
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    eprintln!("\n  Cancelling...");
                    let _ = cancel_tx.send(true);
                }
            });

            let (tx, mut rx) = mpsc::channel::<ProgressUpdate>(100);

            // Spawn progress printer task
            let progress_task = tokio::spawn(async move {
                while let Some(p) = rx.recv().await {
                    let speed_mb = p.speed_bytes_sec / (1024.0 * 1024.0);
                    let downloaded_mb = p.downloaded_bytes as f64 / (1024.0 * 1024.0);

                    if p.total_bytes > 0 {
                        let percent = (p.downloaded_bytes as f64 / p.total_bytes as f64) * 100.0;
                        let total_mb = p.total_bytes as f64 / (1024.0 * 1024.0);
                        eprint!(
                            "\r  [{:>5.1}%] {:.1}/{:.1} MB | Speed: {:>6.2} MB/s | Workers: {} | Chunks: {}/{} | ETA: {}s   ",
                            percent,
                            downloaded_mb,
                            total_mb,
                            speed_mb,
                            p.active_chunks,
                            p.completed_chunks,
                            p.total_chunks,
                            p.eta_seconds
                        );
                    } else {
                        eprint!(
                            "\r  {:.1} MB | Speed: {:>6.2} MB/s   ",
                            downloaded_mb, speed_mb
                        );
                    }
                }
                eprintln!();
            });

            let result = engine
                .download(&probe, &final_path, &adapters, Some(tx), cancel_rx)
                .await;
            let _ = progress_task.await;

            match result {
                Ok(sha256) => {
                    println!("\n✅ Download Finished Successfully!");
                    println!("  Saved to:   {:?}", final_path);
                    println!("  SHA-256:    {}", sha256);
                }
                Err(e) if e.downcast_ref::<DownloadCancelled>().is_some() => {
                    eprintln!("Download cancelled. Partial file left at {:?}", final_path);
                    std::process::exit(130);
                }
                Err(e) => return Err(e),
            }
        }
    }

    Ok(())
}

fn validate_url(url: &str) -> Result<()> {
    let lower = url.trim().to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        bail!("URL must start with http:// or https:// (got {:?})", url);
    }
    Ok(())
}

/// Decides where to save the download.
/// - no `-o`: current directory + suggested name, auto-renamed if taken
/// - `-o <existing dir>` or `-o <path ending in a separator>` (created if missing):
///   that directory + suggested name, auto-renamed if taken
/// - `-o <file>`: exactly that path (overwritten)
///
/// Auto-renamed names are claimed atomically ([`claim_unique_path`] creates the empty file),
/// so two concurrent runs can never pick the same file.
fn resolve_output_path(output: Option<&Path>, suggested_filename: &str) -> Result<PathBuf> {
    let name = sanitize_filename(suggested_filename);
    let dir = match output {
        None => std::env::current_dir().context("Cannot determine current directory")?,
        Some(path) if path.as_os_str().is_empty() => bail!("Output path must not be empty"),
        Some(path) if path.is_dir() => path.to_path_buf(),
        Some(path) if ends_with_separator(path) => {
            std::fs::create_dir_all(path)
                .with_context(|| format!("Cannot create output directory {:?}", path))?;
            path.to_path_buf()
        }
        Some(path) => return Ok(path.to_path_buf()),
    };
    claim_unique_path(&dir, &name)
        .with_context(|| format!("Cannot create output file for {:?} in {:?}", name, dir))
}

/// `true` if the path as typed ends in `/` (or `\` on Windows), i.e. names a directory.
fn ends_with_separator(path: &Path) -> bool {
    path.as_os_str()
        .to_string_lossy()
        .ends_with(std::path::is_separator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url() {
        assert!(validate_url("https://example.com/a.bin").is_ok());
        assert!(validate_url("HTTP://example.com").is_ok());
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("example.com").is_err());
    }

    #[test]
    fn test_resolve_output_path() {
        let dir = tempdir_path();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.bin"), b"x").unwrap();

        // Existing directory: auto-rename.
        assert_eq!(
            resolve_output_path(Some(&dir), "f.bin").unwrap(),
            dir.join("f (1).bin")
        );
        // The auto-renamed name is claimed: a second run cannot pick it again.
        assert!(dir.join("f (1).bin").exists());
        assert_eq!(
            resolve_output_path(Some(&dir), "f.bin").unwrap(),
            dir.join("f (2).bin")
        );
        // Explicit file: used as-is (overwrite).
        assert_eq!(
            resolve_output_path(Some(&dir.join("f.bin")), "other").unwrap(),
            dir.join("f.bin")
        );
        // Explicit non-existent file: used as-is, not created as a directory.
        assert_eq!(
            resolve_output_path(Some(&dir.join("new.bin")), "other").unwrap(),
            dir.join("new.bin")
        );
        assert!(!dir.join("new.bin").exists());
        assert!(resolve_output_path(Some(Path::new("")), "x").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_resolve_output_path_trailing_separator_is_directory() {
        let dir = tempdir_path().with_extension("sep");
        let _ = std::fs::remove_dir_all(&dir);

        // Non-existent path ending in a separator: created and used as a directory.
        let newdir = PathBuf::from(format!("{}/newdir/", dir.display()));
        let got = resolve_output_path(Some(&newdir), "a.iso").unwrap();
        assert!(dir.join("newdir").is_dir());
        assert_eq!(got, dir.join("newdir").join("a.iso"));
        assert!(got.is_file(), "name must be claimed");

        // Existing directory given with a trailing separator: auto-renamed inside it.
        let again = resolve_output_path(Some(&newdir), "a.iso").unwrap();
        assert_eq!(again, dir.join("newdir").join("a (1).iso"));

        assert!(ends_with_separator(Path::new("x/")));
        assert!(!ends_with_separator(Path::new("x")));
        #[cfg(windows)]
        assert!(ends_with_separator(Path::new("x\\")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn tempdir_path() -> PathBuf {
        std::env::temp_dir().join(format!("conflux-cli-test-{}", std::process::id()))
    }

    #[test]
    fn test_cli_rejects_zero_args() {
        assert!(Cli::try_parse_from(["conflux", "download", "http://x/y", "-c", "0"]).is_err());
        assert!(Cli::try_parse_from(["conflux", "download", "http://x/y", "-s", "0"]).is_err());
        assert!(
            Cli::try_parse_from(["conflux", "download", "http://x/y", "-c", "2", "-s", "1"])
                .is_ok()
        );
        assert!(
            Cli::try_parse_from(["conflux", "download", "http://x/y", "--adapter", "WiFi"]).is_ok()
        );
    }
}
