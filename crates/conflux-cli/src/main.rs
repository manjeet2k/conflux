use anyhow::Result;
use clap::{Parser, Subcommand};
use conflux_core::{discover_adapters, DownloadEngine, ProgressUpdate};
use std::path::PathBuf;
use tokio::sync::mpsc;
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

        /// Destination output path or filename
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Chunk size in megabytes (default: 4 MB)
        #[arg(short = 's', long, default_value_t = 4)]
        chunk_size_mb: u64,

        /// Number of concurrent connection workers per network adapter
        #[arg(short, long, default_value_t = 4)]
        connections: usize,
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
                "  Accept-Ranges:   {}",
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
        } => {
            let engine = DownloadEngine::new(chunk_size_mb * 1024 * 1024, connections);
            let probe = engine.probe(&url).await?;

            let final_path = output.unwrap_or_else(|| PathBuf::from(&probe.suggested_filename));

            println!("⚡ Conflux Download Starting");
            println!("  Target:     {}", probe.suggested_filename);
            println!(
                "  Total Size: {:.2} MB",
                probe.total_bytes as f64 / (1024.0 * 1024.0)
            );
            println!("  Output:     {:?}", final_path);

            let adapters = discover_adapters()?;
            let active_count = adapters
                .iter()
                .filter(|a| a.enabled && !a.is_loopback && a.is_ipv4)
                .count();
            println!(
                "  Bonding:    {} active network adapter(s)",
                active_count.max(1)
            );

            let (tx, mut rx) = mpsc::channel::<ProgressUpdate>(100);

            // Spawn progress printer task
            let progress_task = tokio::spawn(async move {
                while let Some(p) = rx.recv().await {
                    let percent = if p.total_bytes > 0 {
                        (p.downloaded_bytes as f64 / p.total_bytes as f64) * 100.0
                    } else {
                        0.0
                    };

                    let speed_mb = p.speed_bytes_sec / (1024.0 * 1024.0);
                    let downloaded_mb = p.downloaded_bytes as f64 / (1024.0 * 1024.0);
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
                }
                eprintln!();
            });

            let sha256 = engine
                .download(&url, &final_path, &adapters, Some(tx))
                .await?;
            let _ = progress_task.await;

            println!("\n✅ Download Finished Successfully!");
            println!("  Saved to:   {:?}", final_path);
            println!("  SHA-256:    {}", sha256);
        }
    }

    Ok(())
}
