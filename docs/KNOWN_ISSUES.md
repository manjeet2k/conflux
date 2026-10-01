# Known issues

Honest list of limitations in the current beta. Fixed items are removed; planned work is in
[ROADMAP.md](ROADMAP.md). Report new ones using the issue templates.

## Installer and trust

- **The installer is unsigned.** Windows SmartScreen shows "Windows protected your PC / unknown
  publisher". Use More info > Run anyway and verify the file with `SHA256SUMS.txt`
  (see the [README](../README.md#windows-protected-your-pc--unknown-publisher)). Code signing is
  deferred until after the beta (decision D2).

## Not yet verified on real Windows hardware

Everything below compiles and is unit-tested where possible, but has not been exercised on a
real Windows machine with several adapters. The checklist is
[WINDOWS_TEST_PLAN.md](WINDOWS_TEST_PLAN.md).

- **Adapter discovery**: link-state and Duplicate Address Detection (tentative address)
  filtering, and the network-change watcher.
- **NTFS sparse files**: preallocation relies on NTFS sparse behaviour; not confirmed with
  `fsutil sparse queryflag`.
- **Mark-of-the-Web**: the `Zone.Identifier` stream written on completion has been tested only
  through its text format.
- **Strict CSP in the real webview**: the UI is built and linted, but rendering under the
  production Content Security Policy in WebView2 has not been confirmed.
- **Tray, notifications, single-instance focus, close-to-tray** and exit with active downloads.
- **Bonding gains**: no published real-world benchmark yet ([BENCHMARKS.md](BENCHMARKS.md)).

## Behaviour limits

- **Adapters without their own gateway may stall.** On Windows an adapter with no default
  gateway can accept a bind to its address yet fail to connect or carry no traffic, so it
  contributes no speed. This is partly mitigated: the UI now shows the last error or the reason
  an adapter was dropped next to it (roadmap V-3), but Conflux cannot fix the missing route and
  such an adapter still does not help the download.
- **Gains need independent links.** Two adapters behind the same router/ISP line share that
  line's capacity.
- **Strict 206 validation can fail on some CDNs.** Every ranged response must carry the same
  ETag/Last-Modified as the probe. CDNs that serve different edges with inconsistent ETags may
  make a multi-connection download fail or fall back (roadmap decision D7; observations being
  collected in [BENCHMARKS.md](BENCHMARKS.md)). The same strictness applies to mirror pools: if
  mirrors of one file send different ETag/Last-Modified values, the mismatching mirror is
  rejected even though the bytes may be identical.
- **Disabling the last enabled adapter mid-download does not stop the download.** It keeps going
  over the default route instead of pausing.
- **Paths longer than 260 characters** (folder plus file name, plus the `.conflux.json` sidecar)
  may fail on Windows unless long paths are enabled (`LongPathsEnabled` policy). Use a shorter
  download folder.
- **The Add dialog probes a URL automatically** a moment after you paste or edit it, to show the
  file name and size. The server sees that request before you click Download.
- **Servers without Range support** are downloaded as a single stream, without bonding.
- Only `http://` and `https://` URLs are supported.

## Privacy and logs

- **Task URLs are stored in plaintext in the history file**, including any credentials typed
  into the URL (`https://user:pass@host/...`) and query-string tokens. Logs redact them; the
  history file (`%APPDATA%\com.conflux.desktop`) does not. Do not put secrets in URLs, and
  clear the history entry or delete the file to remove them.

- **Tokens in URL paths are not redacted in logs.** Credentials and query strings are removed,
  but a secret embedded in the path (for example `/download/<token>/file.zip`) is logged as is.
- **Interface names appear verbatim in diagnostics.** "Copy diagnostics" masks the host part of
  IP addresses but includes adapter names exactly as Windows reports them (these can contain a
  machine or SSID-like name). Review the text before posting it publicly.

## Platform

- Windows 10 (1809+) and 11, 64-bit only. Linux and macOS are not supported.
