# Privacy

Short version: **Conflux has no telemetry, no analytics, no crash reporting and no accounts.**
It talks to the servers you tell it to download from, and checks GitHub for updates. Everything
else stays on your PC.

## What the app contacts

1. **The URLs you add.** For each download the engine sends HTTP requests (a probe, then ranged
   `GET`s) to that URL and any redirect targets, from each network adapter you enabled. The
   server you download from can see your IP address for each adapter (that is how bonding works).
2. **GitHub Releases, for update checks.** The in-app updater asks the project's GitHub
   releases (`github.com/manjeet2k/conflux`, a small `latest.json` file) whether a newer beta exists and downloads it if you
   accept. GitHub sees your IP address and the request, as with any download from GitHub. No
   identifier or usage data is added by the app.

That is all. There are no other outbound connections.

### How this was verified

The statement above is checked against the code, not assumed. On 2026-10-01 we searched the
Rust crates and the UI for network use (`reqwest`, `http(s)://` literals, sockets, `fetch`,
`XMLHttpRequest`, `WebSocket`, analytics/crash-reporting crates):

- The only HTTP client is `reqwest` in `conflux-core`, used by the engine for probes and downloads
  of user-supplied URLs.
- The desktop app has no other HTTP client. Its plugins are single-instance, dialog (folder
  picker), notification (local toasts) and opener (opens a file, folder or the logs folder
  locally).
- The UI makes no network calls (no `fetch`, XHR or WebSocket); it only talks to the Rust side
  over Tauri IPC. The webview's Content Security Policy is `connect-src ipc: http://ipc.localhost`,
  so even a bug could not make the page contact the internet.
- No analytics or crash-reporting dependency exists.
- The updater (`tauri-plugin-updater`, in `crates/conflux-desktop/src/updater.rs`) is configured with
  a single endpoint, `https://github.com/manjeet2k/conflux/releases/download/updater-beta/latest.json`,
  and downloads the installer from GitHub Releases. It is the only call added beyond the engine.
  If you find any other call, report it as a bug. Re-verify this section whenever dependencies change.

## What is stored on your PC

| Data | Where | Notes |
|------|-------|-------|
| Settings | app config folder (`%APPDATA%\com.conflux.desktop`) | theme, chunk size, adapter toggles, folders |
| Download history | app data folder (`%APPDATA%\com.conflux.desktop`) | URLs, file names and paths of your downloads |
| Resume file | next to the partial download, `<file>.conflux.json` | which pieces are done, server validators |
| Logs | app log folder (typically `%LOCALAPPDATA%\com.conflux.desktop\logs`) | URL credentials and query strings are redacted; URL **paths** are not (see [known issues](docs/KNOWN_ISSUES.md)) |

Nothing here is uploaded. Delete the folders to remove it; **Settings > Open logs folder** opens the logs.
Uninstalling never deletes your downloaded files.

## Diagnostics

**Copy diagnostics** puts a text report on your clipboard **only when you click it**. It is not
sent anywhere; you decide whether to paste it into a bug report. It is built from an allow-list:
app and OS version, WebView version, settings values, adapter names/types/state, the **subnet**
of each adapter address with the host part masked (for example `192.168.1.x`), and the last
warning/error log lines with URLs, file paths and IP addresses scrubbed. Adapter names appear
as Windows reports them, so glance at the text before posting it.

## Zone.Identifier

Downloaded files are marked with the standard Windows "downloaded from the internet" marker
(`Zone.Identifier`) containing the source URL without credentials, so Windows can warn you before
you open an executable. That marker lives with the file on your disk.

## Contact

Questions: open an issue, or email Manjeet Singh <manjeetsgh11@gmail.com>.
