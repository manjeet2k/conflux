//! Browser integration and native messaging bridge for Conflux.
//!
//! Enables seamless download handoff from web browsers (Chrome, Edge, Firefox)
//! via standard WebExtensions Native Messaging API, custom `conflux://` protocol handler,
//! and CLI URL arguments.

use conflux_core::RequestHeaders;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// External download payload received from a browser extension, `conflux://` link,
/// or command line invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalDownloadPayload {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<RequestHeaders>,
}

/// Raw message payload received from the browser extension via stdio native messaging.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionMessage {
    pub action: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, alias = "suggested_filename")]
    pub filename: Option<String>,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
}

/// Converts bytes to a lowercase hex string.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Parses a hex string back to bytes.
pub fn from_hex(hex: &str) -> Option<Vec<u8>> {
    let hex = hex.trim();
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        out.push(byte);
    }
    Some(out)
}

/// Parses command-line arguments to find external download requests.
///
/// Supports:
/// 1. `--from-browser <hex>` containing JSON-encoded [`ExternalDownloadPayload`].
/// 2. `--from-browser-file <path>` containing JSON-encoded [`ExternalDownloadPayload`].
/// 3. `conflux://https://...` or `conflux://http://...` custom protocol handler URLs.
/// 4. Direct `https://...` or `http://...` download URLs.
pub fn parse_cli_download_args(args: &[String]) -> Option<ExternalDownloadPayload> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        let trimmed = arg.trim();

        // 1. Browser payload flag
        if trimmed == "--from-browser" {
            if let Some(hex_val) = iter.next() {
                if let Some(bytes) = from_hex(hex_val) {
                    if let Ok(payload) = serde_json::from_slice::<ExternalDownloadPayload>(&bytes) {
                        if is_valid_url(&payload.url) {
                            return Some(payload);
                        }
                    }
                }
            }
            continue;
        }

        // 1b. Browser payload file (for large payloads exceeding Windows command line length limits)
        if trimmed == "--from-browser-file" {
            if let Some(file_path_str) = iter.next() {
                let path = std::path::Path::new(file_path_str);
                // Security: only read and delete files that we created ourselves.
                // The handoff file must live in the OS temp directory and match our
                // naming convention (conflux-handoff-<uuid>.json) to prevent an
                // attacker from abusing argument splitting to delete arbitrary files.
                let safe = path.parent().is_some_and(|p| p == std::env::temp_dir())
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("conflux-handoff-") && n.ends_with(".json"));
                if safe {
                    if let Ok(bytes) = std::fs::read(path) {
                        let _ = std::fs::remove_file(path);
                        if let Ok(payload) =
                            serde_json::from_slice::<ExternalDownloadPayload>(&bytes)
                        {
                            if is_valid_url(&payload.url) {
                                return Some(payload);
                            }
                        }
                    }
                }
            }
            continue;
        }

        // 2. Custom protocol handler: conflux://https://... or conflux:https://...
        if let Some(rest) = trimmed
            .strip_prefix("conflux://")
            .or_else(|| trimmed.strip_prefix("conflux:"))
        {
            let target_url = rest
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches('/');
            if is_valid_url(target_url) {
                return Some(ExternalDownloadPayload {
                    url: target_url.to_string(),
                    filename: None,
                    headers: None,
                });
            }
        }

        // 3. Direct HTTP(S) URL
        if is_valid_url(trimmed) {
            return Some(ExternalDownloadPayload {
                url: trimmed.to_string(),
                filename: None,
                headers: None,
            });
        }
    }
    None
}

fn is_valid_url(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("http://") || s.starts_with("https://")) && tauri::Url::parse(s).is_ok()
}

/// Runs the Native Messaging Host stdio protocol loop for Chrome, Edge, and Firefox.
///
/// Reads 32-bit length-prefixed JSON from `stdin`, forwards the download to the
/// Conflux desktop instance via `current_exe() --from-browser <hex>`, and responds
/// with `{"status":"ok"}` on `stdout`.
pub fn run_native_host() {
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    let mut len_buf = [0u8; 4];
    while stdin.read_exact(&mut len_buf).is_ok() {
        let msg_len = u32::from_ne_bytes(len_buf) as usize;
        if msg_len == 0 || msg_len > 1024 * 1024 {
            // Chrome sends EOF or illegal length on close; exit cleanly.
            break;
        }

        let mut msg_buf = vec![0u8; msg_len];
        if stdin.read_exact(&mut msg_buf).is_err() {
            break;
        }

        let Ok(msg) = serde_json::from_slice::<ExtensionMessage>(&msg_buf) else {
            let _ = send_response(
                &mut stdout,
                serde_json::json!({ "status": "error", "message": "invalid JSON" }),
            );
            continue;
        };

        match msg.action.as_str() {
            "ping" => {
                let _ = send_response(
                    &mut stdout,
                    serde_json::json!({
                        "status": "ok",
                        "app": "Conflux",
                        "version": env!("CARGO_PKG_VERSION")
                    }),
                );
            }
            "download" | "start_download" => {
                if let Some(raw_url) = msg.url {
                    if is_valid_url(&raw_url) {
                        let payload = ExternalDownloadPayload {
                            url: raw_url,
                            filename: msg.filename,
                            headers: Some(RequestHeaders {
                                cookie: msg.cookies.filter(|c| !c.trim().is_empty()),
                                referer: msg.referer.filter(|r| !r.trim().is_empty()),
                                user_agent: msg.user_agent.filter(|ua| !ua.trim().is_empty()),
                            }),
                        };

                        if let Ok(json_bytes) = serde_json::to_vec(&payload) {
                            if let Ok(exe_path) = std::env::current_exe() {
                                let mut cmd = std::process::Command::new(exe_path);
                                // If the payload is large (e.g. enterprise cookies > 3KB),
                                // write to a temporary handoff file to stay well clear of
                                // the Windows 8191 character command-line limit.
                                if json_bytes.len() > 3000 {
                                    let temp_path = std::env::temp_dir().join(format!(
                                        "conflux-handoff-{}.json",
                                        uuid::Uuid::new_v4()
                                    ));
                                    if std::fs::write(&temp_path, &json_bytes).is_ok() {
                                        cmd.arg("--from-browser-file").arg(&temp_path);
                                    } else {
                                        let hex_str = to_hex(&json_bytes);
                                        cmd.arg("--from-browser").arg(hex_str);
                                    }
                                } else {
                                    let hex_str = to_hex(&json_bytes);
                                    cmd.arg("--from-browser").arg(hex_str);
                                }
                                let _ = cmd.spawn();
                            }
                        }

                        let _ = send_response(&mut stdout, serde_json::json!({ "status": "ok" }));
                    } else {
                        let _ = send_response(
                            &mut stdout,
                            serde_json::json!({ "status": "error", "message": "invalid URL" }),
                        );
                    }
                } else {
                    let _ = send_response(
                        &mut stdout,
                        serde_json::json!({ "status": "error", "message": "missing url" }),
                    );
                }
            }
            other => {
                let _ = send_response(
                    &mut stdout,
                    serde_json::json!({ "status": "error", "message": format!("unknown action: {other}") }),
                );
            }
        }
    }
}

fn send_response<W: Write>(writer: &mut W, value: serde_json::Value) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(&value)?;
    let len = (bytes.len() as u32).to_ne_bytes();
    writer.write_all(&len)?;
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(windows)]
pub fn register_browser_integration() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_string_lossy();
    let dir = exe
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No parent directory"))?;

    let chrome_manifest_path = dir.join("com.conflux.desktop.json");
    if !chrome_manifest_path.exists() {
        let manifest_content = serde_json::json!({
            "name": "com.conflux.desktop",
            "description": "Conflux Download Accelerator Native Messaging Host",
            "path": exe_str,
            "type": "stdio",
            "allowed_origins": [
                "chrome-extension://ddnnilfjdnicfekflgpibapcoakighmf/"
            ]
        });
        let _ = std::fs::write(
            &chrome_manifest_path,
            serde_json::to_string_pretty(&manifest_content)?,
        );
    }

    let ff_manifest_path = dir.join("com.conflux.desktop.firefox.json");
    if !ff_manifest_path.exists() {
        let manifest_content = serde_json::json!({
            "name": "com.conflux.desktop",
            "description": "Conflux Download Accelerator Native Messaging Host",
            "path": exe_str,
            "type": "stdio",
            "allowed_extensions": [
                "conflux@conflux.app"
            ]
        });
        let _ = std::fs::write(
            &ff_manifest_path,
            serde_json::to_string_pretty(&manifest_content)?,
        );
    }

    let chrome_manifest_str = chrome_manifest_path.to_string_lossy();
    let ff_manifest_str = ff_manifest_path.to_string_lossy();
    let icon_arg = format!("\"{exe_str}\",0");
    let cmd_arg = format!("\"{exe_str}\" \"%1\"");

    let reg_cmds = [
        vec![
            "add",
            r"HKCU\Software\Classes\conflux",
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            "URL:Conflux Protocol",
            "/f",
        ],
        vec![
            "add",
            r"HKCU\Software\Classes\conflux",
            "/v",
            "URL Protocol",
            "/t",
            "REG_SZ",
            "/d",
            "",
            "/f",
        ],
        vec![
            "add",
            r"HKCU\Software\Classes\conflux\DefaultIcon",
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &icon_arg,
            "/f",
        ],
        vec![
            "add",
            r"HKCU\Software\Classes\conflux\shell\open\command",
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &cmd_arg,
            "/f",
        ],
        vec![
            "add",
            r"HKCU\Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop",
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &chrome_manifest_str,
            "/f",
        ],
        vec![
            "add",
            r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.conflux.desktop",
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &chrome_manifest_str,
            "/f",
        ],
        vec![
            "add",
            r"HKCU\Software\Mozilla\NativeMessagingHosts\com.conflux.desktop",
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &ff_manifest_str,
            "/f",
        ],
    ];

    for cmd_args in reg_cmds {
        let _ = std::process::Command::new("reg").args(&cmd_args).output();
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn register_browser_integration() -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
pub fn unregister_browser_integration() -> std::io::Result<()> {
    let reg_keys = [
        r"HKCU\Software\Classes\conflux",
        r"HKCU\Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop",
        r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.conflux.desktop",
        r"HKCU\Software\Mozilla\NativeMessagingHosts\com.conflux.desktop",
    ];

    for key in reg_keys {
        let _ = std::process::Command::new("reg")
            .args(["delete", key, "/f"])
            .output();
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn unregister_browser_integration() -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
pub fn is_browser_integration_registered() -> bool {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop",
        ])
        .output();
    output.map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(not(windows))]
pub fn is_browser_integration_registered() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_roundtrip() {
        let data = b"Hello, Conflux Browser Bridge! 12345";
        let hex = to_hex(data);
        let back = from_hex(&hex).unwrap();
        assert_eq!(back, data);
        assert_eq!(from_hex("invalid_odd"), None);
    }

    #[test]
    fn test_parse_cli_download_args_direct_url() {
        let args = vec![
            "conflux-desktop.exe".into(),
            "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz".into(),
        ];
        let parsed = parse_cli_download_args(&args).expect("must parse URL");
        assert_eq!(
            parsed.url,
            "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz"
        );
        assert_eq!(parsed.filename, None);
        assert_eq!(parsed.headers, None);
    }

    #[test]
    fn test_parse_cli_download_args_protocol_scheme() {
        let args = vec![
            "conflux-desktop.exe".into(),
            "conflux://https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz".into(),
        ];
        let parsed = parse_cli_download_args(&args).expect("must parse conflux://");
        assert_eq!(
            parsed.url,
            "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz"
        );

        let args_colon = vec![
            "conflux-desktop.exe".into(),
            "conflux:https://example.com/test.zip".into(),
        ];
        let parsed_colon = parse_cli_download_args(&args_colon).expect("must parse conflux:");
        assert_eq!(parsed_colon.url, "https://example.com/test.zip");
    }

    #[test]
    fn test_parse_cli_download_args_from_browser_payload() {
        let original = ExternalDownloadPayload {
            url: "https://secure.example.com/archive.zip".into(),
            filename: Some("archive.zip".into()),
            headers: Some(RequestHeaders {
                cookie: Some("session_token=secret_987".into()),
                referer: Some("https://example.com/downloads".into()),
                user_agent: Some("Mozilla/5.0 Chrome/120".into()),
            }),
        };
        let json = serde_json::to_vec(&original).unwrap();
        let hex = to_hex(&json);

        let args = vec!["conflux-desktop.exe".into(), "--from-browser".into(), hex];
        let parsed = parse_cli_download_args(&args).expect("must parse browser payload");
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_parse_cli_download_args_none_when_empty() {
        let args = vec!["conflux-desktop.exe".into()];
        assert_eq!(parse_cli_download_args(&args), None);

        let bad_args = vec!["conflux-desktop.exe".into(), "--some-flag".into()];
        assert_eq!(parse_cli_download_args(&bad_args), None);
    }

    #[test]
    fn test_send_response_writes_length_prefixed_json() {
        let mut buf = Vec::new();
        let val = serde_json::json!({ "status": "ok" });
        send_response(&mut buf, val).unwrap();

        assert!(buf.len() >= 4);
        let len = u32::from_ne_bytes(buf[0..4].try_into().unwrap()) as usize;
        assert_eq!(len, buf.len() - 4);
        let json: serde_json::Value = serde_json::from_slice(&buf[4..]).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[test]
    fn test_parse_cli_download_args_from_browser_file() {
        let original = ExternalDownloadPayload {
            url: "https://secure.example.com/large-archive.iso".into(),
            filename: Some("large-archive.iso".into()),
            headers: Some(RequestHeaders {
                cookie: Some("auth_token=super_long_cookie".into()),
                referer: Some("https://example.com/iso".into()),
                user_agent: None,
            }),
        };
        let temp_file =
            std::env::temp_dir().join(format!("conflux-handoff-{}.json", uuid::Uuid::new_v4()));
        let json = serde_json::to_vec(&original).unwrap();
        std::fs::write(&temp_file, &json).unwrap();

        let args = vec![
            "conflux-desktop.exe".into(),
            "--from-browser-file".into(),
            temp_file.to_string_lossy().to_string(),
        ];
        let parsed = parse_cli_download_args(&args).expect("must parse payload from file");
        assert_eq!(parsed, original);
        // Assert temp file was cleaned up
        assert!(!temp_file.exists());
    }

    #[test]
    fn test_from_browser_file_rejects_unsafe_path() {
        // A file outside the expected naming convention must be rejected to prevent
        // arbitrary file deletion via crafted CLI arguments.
        let dangerous_file =
            std::env::temp_dir().join(format!("evil-{}.json", uuid::Uuid::new_v4()));
        let payload = ExternalDownloadPayload {
            url: "https://example.com/file.zip".into(),
            filename: None,
            headers: None,
        };
        let json = serde_json::to_vec(&payload).unwrap();
        std::fs::write(&dangerous_file, &json).unwrap();

        let args = vec![
            "conflux-desktop.exe".into(),
            "--from-browser-file".into(),
            dangerous_file.to_string_lossy().to_string(),
        ];
        // Must return None — the path doesn't match conflux-handoff-*.json
        assert_eq!(parse_cli_download_args(&args), None);
        // The file must NOT have been deleted
        assert!(dangerous_file.exists());
        let _ = std::fs::remove_file(&dangerous_file);
    }

    #[test]
    fn test_extension_message_suggested_filename_alias() {
        let raw_json = serde_json::json!({
            "action": "download",
            "url": "https://example.com/test.bin",
            "suggested_filename": "test.bin"
        });
        let msg: ExtensionMessage = serde_json::from_value(raw_json).unwrap();
        assert_eq!(msg.filename, Some("test.bin".into()));
    }
}
