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
    /// Correlates auto-interception with an accepted or dismissed desktop request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_id: Option<String>,
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
    pub wait_for_acceptance: bool,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub suggested_filename: Option<String>,
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
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Some(nibble(pair[0])? * 16 + nibble(pair[1])?))
        .collect()
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
            let target_url = rest.trim().trim_matches('"').trim_matches('\'');
            if is_valid_url(target_url) {
                return Some(ExternalDownloadPayload {
                    handoff_id: None,
                    url: target_url.to_string(),
                    filename: None,
                    headers: None,
                });
            }
        }

        // 3. Direct HTTP(S) URL
        if is_valid_url(trimmed) {
            return Some(ExternalDownloadPayload {
                handoff_id: None,
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

fn receipt_path(id: &str) -> std::io::Result<std::path::PathBuf> {
    let id = uuid::Uuid::parse_str(id)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid handoff id"))?;
    Ok(std::env::temp_dir().join(format!("conflux-receipt-{id}.txt")))
}

struct HandoffReceipt(std::path::PathBuf);
impl HandoffReceipt {
    fn prepare(id: &str) -> std::io::Result<Self> {
        let path = receipt_path(id)?;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.with_extension("pending"))?;
        Ok(Self(path))
    }
}
impl Drop for HandoffReceipt {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.with_extension("pending"));
        let _ = std::fs::remove_file(&self.0);
    }
}

/// First decision wins: a dismissed request must never later cancel its browser transfer.
pub fn acknowledge_handoff(id: &str, accepted: bool) -> std::io::Result<bool> {
    let path = receipt_path(id)?;
    if !path.with_extension("pending").try_exists()? {
        return Ok(false);
    }
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    file.write_all(if accepted { b"accepted" } else { b"cancelled" })?;
    Ok(true)
}

fn wait_for_handoff(id: &str, timeout: std::time::Duration) -> std::io::Result<&'static str> {
    let path = receipt_path(id)?;
    let started = std::time::Instant::now();
    loop {
        match std::fs::read(&path) {
            Ok(value) if value == b"accepted" => break Ok("accepted"),
            Ok(value) if value == b"cancelled" => break Ok("cancelled"),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => break Err(error),
        }
        if started.elapsed() >= timeout {
            break Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "handoff not accepted",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn launch_payload(payload: &ExternalDownloadPayload) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(payload).map_err(std::io::Error::other)?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    let temporary = if bytes.len() > 3000 {
        let path =
            std::env::temp_dir().join(format!("conflux-handoff-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, &bytes)?;
        command.arg("--from-browser-file").arg(&path);
        Some(path)
    } else {
        command.arg("--from-browser").arg(to_hex(&bytes));
        None
    };
    if let Err(error) = command.spawn() {
        if let Some(path) = temporary {
            let _ = std::fs::remove_file(path);
        }
        return Err(error);
    }
    Ok(())
}

/// Runs the Native Messaging Host stdio protocol loop for Chrome, Edge, and Firefox.
///
/// Reads 32-bit length-prefixed JSON from `stdin`, forwards the download to the
/// Conflux desktop instance via `current_exe() --from-browser <hex>`, and responds
/// with a queued, accepted, cancelled, or error status on `stdout`.
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
            "open" => {
                let result = std::env::current_exe()
                    .and_then(|path| std::process::Command::new(path).spawn());
                let response = match result {
                    Ok(_) => serde_json::json!({ "status": "ok" }),
                    Err(_) => {
                        serde_json::json!({ "status": "error", "message": "could not launch Conflux" })
                    }
                };
                let _ = send_response(&mut stdout, response);
            }
            "download" | "start_download" => {
                let Some(url) = msg.url.filter(|url| is_valid_url(url)) else {
                    let _ = send_response(
                        &mut stdout,
                        serde_json::json!({ "status": "error", "message": "missing or invalid URL" }),
                    );
                    continue;
                };
                let payload = ExternalDownloadPayload {
                    handoff_id: msg
                        .wait_for_acceptance
                        .then(|| uuid::Uuid::new_v4().to_string()),
                    url,
                    filename: msg.filename.or(msg.suggested_filename),
                    headers: Some(RequestHeaders {
                        cookie: msg.cookies.filter(|value| !value.trim().is_empty()),
                        referer: msg.referer.filter(|value| !value.trim().is_empty()),
                        user_agent: msg.user_agent.filter(|value| !value.trim().is_empty()),
                    }),
                };
                let result = (|| {
                    let _receipt = payload
                        .handoff_id
                        .as_deref()
                        .map(HandoffReceipt::prepare)
                        .transpose()?;
                    launch_payload(&payload)?;
                    match payload.handoff_id.as_deref() {
                        Some(id) => wait_for_handoff(id, std::time::Duration::from_secs(600)),
                        None => Ok("queued"),
                    }
                })();
                let response = match result {
                    Ok(status) => serde_json::json!({ "status": status }),
                    Err(_) => {
                        serde_json::json!({ "status": "error", "message": "Conflux did not accept the download; keep the browser transfer" })
                    }
                };
                let _ = send_response(&mut stdout, response);
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
                "chrome-extension://ddnnilfjdnicfekflgpibapcoakighmf/",
                "chrome-extension://ncapjgooomipdohlolfbbknhbjcfdjml/"
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

    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Protocol: conflux://
    let (conflux_key, _) = hkcu.create_subkey(r"Software\Classes\conflux")?;
    conflux_key.set_value("", &"URL:Conflux Protocol")?;
    conflux_key.set_value("URL Protocol", &"")?;

    let (icon_key, _) = hkcu.create_subkey(r"Software\Classes\conflux\DefaultIcon")?;
    icon_key.set_value("", &icon_arg)?;

    let (cmd_key, _) = hkcu.create_subkey(r"Software\Classes\conflux\shell\open\command")?;
    cmd_key.set_value("", &cmd_arg)?;

    // Native messaging hosts
    let chrome_hosts = [
        r"Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop",
        r"Software\Microsoft\Edge\NativeMessagingHosts\com.conflux.desktop",
        r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.conflux.desktop",
    ];
    let manifest_val: &str = &chrome_manifest_str;
    for host in chrome_hosts {
        let (key, _) = hkcu.create_subkey(host)?;
        key.set_value("", &manifest_val)?;
    }

    let (ff_key, _) =
        hkcu.create_subkey(r"Software\Mozilla\NativeMessagingHosts\com.conflux.desktop")?;
    let ff_val: &str = &ff_manifest_str;
    ff_key.set_value("", &ff_val)?;

    Ok(())
}

#[cfg(not(windows))]
pub fn register_browser_integration() -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
pub fn unregister_browser_integration() -> std::io::Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(r"Software\Classes\conflux");
    let _ =
        hkcu.delete_subkey_all(r"Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop");
    let _ =
        hkcu.delete_subkey_all(r"Software\Microsoft\Edge\NativeMessagingHosts\com.conflux.desktop");
    let _ = hkcu.delete_subkey_all(
        r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.conflux.desktop",
    );
    let _ = hkcu.delete_subkey_all(r"Software\Mozilla\NativeMessagingHosts\com.conflux.desktop");
    Ok(())
}

#[cfg(not(windows))]
pub fn unregister_browser_integration() -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
pub fn is_browser_integration_registered() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(r"Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop")
        .is_ok()
}

#[cfg(not(windows))]
pub fn is_browser_integration_registered() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_unicode_hex_is_rejected_without_panicking() {
        for value in ["€0", "😀", "１２", "0g", "a"] {
            assert_eq!(from_hex(value), None, "{value:?}");
        }
        assert_eq!(from_hex(" 00aF "), Some(vec![0, 175]));
    }

    #[test]
    fn handoff_requires_acceptance_and_keeps_the_first_decision() {
        let id = uuid::Uuid::new_v4().to_string();
        let receipt = HandoffReceipt::prepare(&id).unwrap();
        assert!(acknowledge_handoff(&id, false).unwrap());
        assert!(!acknowledge_handoff(&id, true).unwrap());
        assert_eq!(
            wait_for_handoff(&id, std::time::Duration::ZERO).unwrap(),
            "cancelled"
        );
        drop(receipt);
        assert!(!receipt_path(&id).unwrap().exists());
        assert!(!acknowledge_handoff(&id, true).unwrap());
        let id = uuid::Uuid::new_v4().to_string();
        let _receipt = HandoffReceipt::prepare(&id).unwrap();
        assert!(wait_for_handoff(&id, std::time::Duration::ZERO).is_err());
        assert!(acknowledge_handoff(&id, true).unwrap());
        assert_eq!(
            wait_for_handoff(&id, std::time::Duration::ZERO).unwrap(),
            "accepted"
        );
        assert!(acknowledge_handoff("../invalid", true).is_err());
    }

    #[test]
    fn protocol_targets_keep_their_exact_trailing_slashes() {
        for url in [
            "https://example.com/download/",
            "https://example.com/download//",
            "https://example.com/download?token=x/",
        ] {
            let parsed =
                parse_cli_download_args(&["conflux".into(), format!("conflux://{url}")]).unwrap();
            assert_eq!(parsed.url, url);
        }
    }

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
            handoff_id: None,
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
            handoff_id: None,
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
            handoff_id: None,
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
            "filename": "test.bin",
            "suggested_filename": "test.bin"
        });
        let msg: ExtensionMessage = serde_json::from_value(raw_json).unwrap();
        assert_eq!(
            msg.filename.or(msg.suggested_filename),
            Some("test.bin".into())
        );
    }

    #[test]
    fn test_extension_message_open_action() {
        let raw_json = serde_json::json!({
            "action": "open"
        });
        let msg: ExtensionMessage = serde_json::from_value(raw_json).unwrap();
        assert_eq!(msg.action, "open");
        assert_eq!(msg.url, None);
    }
}
