# Conflux Browser Extension

The official browser companion for **Conflux Download Accelerator**. Seamlessly hand off download links and files from Google Chrome, Microsoft Edge, Brave, Mozilla Firefox, and other Chromium browsers to Conflux Desktop.

## Features

- **Context Menu Integration**: Right-click any link, video, or audio file and choose **"Download with Conflux"**.
- **Request Context Preservation**: Forwards the page `Referer` and `User-Agent` so servers that validate them accept the hand-off.
- **Automatic Interception (Opt-in)**: Intercepts browser downloads matching configurable file extensions (`.zip`, `.iso`, `.exe`, `.tar.gz`, `.mp4`, etc.) and optional minimum file size thresholds, routing them to Conflux.
- **Zero Background Daemons**: Uses the Windows Native Messaging API and Conflux's single-instance IPC bridge (`conflux-desktop.exe --from-browser <payload>`) to wake the desktop app in under 20ms with zero persistent background servers or listening ports.
- **Zero-Extension Fallback**: Works alongside `conflux://` protocol links for environments without extension support.

## Installation / Loading in Developer Mode

### Chromium Browsers (Google Chrome, Microsoft Edge, Brave, Vivaldi, Opera)

1. Open your browser's extensions page:
   - Chrome: `chrome://extensions`
   - Edge: `edge://extensions`
   - Brave: `brave://extensions`
2. Enable **"Developer mode"** (top right toggle).
3. Click **"Load unpacked"**.
4. Select the `extensions/conflux-browser` directory in this repository.
5. The extension ID is deterministically set to `ddnnilfjdnicfekflgpibapcoakighmf` via the public key in `manifest.json`.

### Mozilla Firefox

1. Open `about:debugging#/runtime/this-firefox`.
2. Click **"Load Temporary Add-on..."**.
3. Select `extensions/conflux-browser/manifest.json`.

## Native Messaging Host Registration

When Conflux is installed via the Windows NSIS installer, the native messaging manifests are automatically registered in the Windows registry:

- Google Chrome / Chromium / Opera / Vivaldi: `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop`
- Microsoft Edge: `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.conflux.desktop`
- Brave Browser: `HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.conflux.desktop`
- Mozilla Firefox: `HKCU\Software\Mozilla\NativeMessagingHosts\com.conflux.desktop`

### Manual / Development Registration

During development or when running a portable build, you can register or unregister the native messaging host directly from the Conflux CLI:

```cmd
conflux-desktop.exe --register-browser
```

To remove registry entries:

```cmd
conflux-desktop.exe --unregister-browser
```

## Security & Privacy

- **No Remote Telemetry**: The extension contains no analytics, ads, or external scripts.
- **Localhost Only**: Communication occurs strictly over OS-level stdio pipes directly with the local `conflux-desktop.exe` binary.
- **Minimal Permissions**: No host permissions, no cookie access, and no tab access. Only the download URL, referrer, and User-Agent are handed to Conflux.
- **Privacy Policy**: Read our complete [Browser Extension Privacy Policy](https://manjeet2k.github.io/conflux/privacy.html).
