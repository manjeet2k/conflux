//! Persistent logging: daily-rotating files in the app log dir (release builds have no
//! console), URL credentials/queries scrubbed on the way out, and a panic hook that leaves
//! a timestamped report next to the logs.

use crate::fileutil::utc_stamp_now;
use crate::redact::{redact_urls, redact_user_paths, scrub_diagnostic_line};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Number of daily log files kept.
const MAX_LOG_FILES: usize = 7;
const LOG_PREFIX: &str = "conflux";
const LOG_SUFFIX: &str = "log";

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Keeps the background log writer flushing for the life of the process.
static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// The directory log files and panic reports are written to, once logging is initialised.
pub fn log_dir() -> Option<&'static Path> {
    LOG_DIR.get().map(PathBuf::as_path)
}

/// A writer that removes URL credentials and query strings from everything written through
/// it. `tracing_subscriber` hands the formatted event to the writer in one `write_all`, so
/// a URL is never split across calls.
struct RedactingWriter<W: Write>(W);

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match std::str::from_utf8(buf) {
            Ok(text) => self.0.write_all(redact_urls(text).as_bytes())?,
            Err(_) => self.0.write_all(buf)?,
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

#[derive(Clone)]
struct RedactingMakeWriter<M>(M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for RedactingMakeWriter<M> {
    type Writer = RedactingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter(self.0.make_writer())
    }
}

fn env_filter() -> EnvFilter {
    EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into())
}

fn file_writer(dir: &Path) -> Result<NonBlocking, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .filename_suffix(LOG_SUFFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .map_err(|e| e.to_string())?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let _ = GUARD.set(guard);
    Ok(writer)
}

/// Starts logging to stderr (dev consoles) and, when `dir` is usable, to daily log files.
/// Call once; later calls are no-ops. Returns a warning if the file log could not start.
pub fn init(dir: Option<&Path>) -> Option<String> {
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(|| RedactingWriter(std::io::stderr()))
        .with_filter(env_filter());
    let mut problem = None;
    let file_layer = dir.and_then(|dir| match file_writer(dir) {
        Ok(writer) => {
            let _ = LOG_DIR.set(dir.to_path_buf());
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(RedactingMakeWriter(writer))
                    .with_filter(env_filter()),
            )
        }
        Err(e) => {
            problem = Some(format!("Cannot write logs to {}: {e}", dir.display()));
            None
        }
    });
    let _ = tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .try_init();
    problem
}

/// One-line description of the running build for log headers and panic reports.
pub fn build_info() -> String {
    format!(
        "Conflux {} ({} {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Release builds have no console, so a panic would vanish. Persist it as
/// `panic-<UTC timestamp>.txt` in the log dir (the OS temp dir if logging is not up yet).
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let report = format!(
            "{}\nTime (UTC): {}\n\nPANIC: {}\n\nBacktrace:\n{}\n",
            build_info(),
            utc_stamp_now(),
            info,
            std::backtrace::Backtrace::force_capture()
        );
        // Backtrace frames carry absolute paths (C:\Users\<name>\...): drop the account name
        // and the literal profile dir so a shared report does not identify the user.
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .ok();
        let report = redact_user_paths(&redact_urls(&report), home.as_deref());
        eprintln!("{report}");
        let dir = log_dir().map_or_else(std::env::temp_dir, Path::to_path_buf);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(format!("panic-{}.txt", utc_stamp_now())), report);
    }));
}

/// The last `max` WARN/ERROR lines of `text`, scrubbed of paths, URLs and IP addresses.
pub fn recent_error_lines(text: &str, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.contains(" WARN ") || l.contains(" ERROR "))
        .map(scrub_diagnostic_line)
        .collect();
    let excess = lines.len().saturating_sub(max);
    lines.drain(..excess);
    lines
}

/// Reads the newest log file(s) in `dir` and returns up to `max` scrubbed error lines.
pub fn read_recent_errors(dir: &Path, max: usize) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(LOG_PREFIX) && n.ends_with(LOG_SUFFIX))
        })
        .collect();
    files.sort();
    // Newest file first; stop once enough lines were found, then restore chronological order.
    let mut collected: Vec<String> = Vec::new();
    for file in files.iter().rev() {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let mut lines = recent_error_lines(&text, max);
        lines.extend(collected);
        collected = lines;
        if collected.len() >= max {
            break;
        }
    }
    let excess = collected.len().saturating_sub(max);
    collected.drain(..excess);
    collected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_redacts_urls_but_reports_full_length() {
        let mut sink = Vec::new();
        let input = b"INFO probe url=https://u:p@h/f?token=SECRET done\n";
        let n = RedactingWriter(&mut sink).write(input).unwrap();
        assert_eq!(n, input.len());
        let out = String::from_utf8(sink).unwrap();
        assert_eq!(out, "INFO probe url=https://h/f?<redacted> done\n");
    }

    #[test]
    fn recent_errors_keeps_last_n_and_scrubs() {
        let log = "\
2026-01-01T00:00:00Z  INFO a: started
2026-01-01T00:00:01Z  WARN a: first path=C:\\Users\\bob\\f.bin
2026-01-01T00:00:02Z ERROR a: second from 192.168.1.57
2026-01-01T00:00:03Z  WARN a: third
";
        let lines = recent_error_lines(log, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("second") && lines[0].contains("192.168.1.x"));
        assert!(lines[1].contains("third"));
        let all = recent_error_lines(log, 20).join("\n");
        assert!(!all.contains("bob") && !all.contains("57"));
    }
}
