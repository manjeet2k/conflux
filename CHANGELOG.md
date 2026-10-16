# Changelog

All notable changes to Conflux are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

**Rule:** every user-visible change adds a line under `Unreleased`. On release, rename
`Unreleased` to the new version and date (`scripts/bump-version.mjs` keeps version files in sync).

## [Unreleased]

## [0.3.1] - 2026-10-16

### Changed
- Replaced the desktop, title bar, favicon, and browser extension app icons with the supplied bird/download v2 artwork, preserving transparency and size-specific small icons. Desktop version is now 0.3.1 and browser extension version is 0.2.3.

## [0.2.23] - 2026-10-16

### Added
- Native Windows Shell file and folder icons throughout download lists, details, dialogs and folder actions. Icons follow installed file associations, load without reading partial files, and are cached per extension.
- **Window material setting:** choose Mica Alt (default), Mica, Acrylic or None under Settings → Appearance. Falls back to a weaker material where Windows lacks the requested one.
- Context menus and dropdown lists use in-app acrylic like Windows 11 flyouts.
- Clicking a download opens the details pane if it is closed.
- **Speed Graph Sparklines & History Tracking:** Compact sparklines in SpeedGraph, independent per-task rolling speed histories, smooth sample animations, and real-time statistics.
- **Download Duration & Average Speed:** Track and display total active transfer duration and accurate overall average download speed in download items and details pane.

### Changed
- **Speed-Sized Chunks & Connection Reuse:** Dynamic chunk sizing adapts pieces to each adapter's measured throughput (~4s of data, 4 MiB to 256 MiB) with full keep-alive connection reuse and zero-copy write buffer handoffs.
- **Adaptive Rate-Limit & Backoff Handling:** HTTP 429 and 503 (`Retry-After`) pause new requests across workers without consuming retry budgets, transitioning to paced requests and fair-share chunks.
- Buttons throughout the app share quick shading feedback and a smooth release, including toolbar, navigation, icon and dialog controls; text stays at its original size, and reduced-motion settings suppress transitions.
- Push buttons use the reference's quiet filled surface, normal-weight text, and compact rounded sizing; dropdown triggers disappear while their popup is open and return on dismissal or selection.
- Settings dropdowns follow the recorded Task Manager behavior: content-sized acrylic popups, device-pixel-rounded item spacing, a surface reveal around the current choice, and immediate dismissal.
- Redesigned download progress bars: status-colored fill on a visible track, a moving sheen while running, and a one-decimal percentage.
- Windows 11-style rounded row highlight in the download list; toolbar, column headers, rows and status bar now share one left edge.
- Parallel downloads and block resume require verified ranges, a known size and a strong ETag. Other transfers restart as a complete single stream, including when a server loses range support.
- Missing secondary Last-Modified headers on a ranged response are accepted when its strong ETag matches; contradictory supplied validators still fail.
- Partial-file deletion and resume verify recorded Windows file identity. Legacy files with unknown ownership and replacement files are preserved.
- Browser extension 0.2.2 waits for a desktop task to be accepted before cancelling an intercepted transfer. Dismissal, timeout and host failure retain the browser download.

### Fixed
- Restored standard Fluent UI vector icons (`FolderOpen20Regular` and `Folder20Regular`) for folder action buttons, menus, and settings cards, and added a `Folder20Regular` fallback to `FolderIcon` when shell icons are unavailable.
- Prevented Fluent's opaque pressed fill from turning neutral buttons black; button presses retain a subtle translucent fill without an inset shadow.
- Retrieve native 256-pixel Shell icon frames for sharp file and folder icons at high display scaling, avoiding enlargement of 48-pixel icons.
- The window material was never visible: the UI root painted an opaque background over it.
- Blurry text and edges in the download list caused by rows sitting on half-pixel offsets.
- Enabled Hyper-V external uplinks by requesting gateway information during Windows discovery.
- Prevented signed URL queries from being generated as redirect Referer headers; explicit browser context is retained.
- Rejected partial/short single-stream responses and delayed excess bytes on unsplit ranged responses.
- Rejected explicit CLI adapter selections that would otherwise fall back to an unintended default route.
- Serialized adapter changes, reconciled selection after probing, and made concurrent stop/remove wait for shutdown.
- Restored permitted non-cookie headers before protected resume probes, retaining their original authorized origin across redirects.
- Kept Add dialog edits/submission state while links queue, prevented stale submissions advancing another request, and presented links queued behind Remove.
- Recorded actual completed sizes for unknown-length downloads, rejected malformed Unicode hex without panicking, and preserved exact protocol URL trailing slashes.

## [0.2.4] - 2026-10-14

### Added
- **Dynamic Range Splitting & Adaptive Chunking:** Real-time throughput-based segment stealing and dynamic range splitting across bonded network adapters. Faster connections automatically claim and split remaining byte segments from slower or stalled interfaces to sustain maximum aggregate bandwidth.
- **Brave Browser Support:** Full support for Brave alongside Google Chrome, Microsoft Edge, and Mozilla Firefox for browser integration and download interception.
- **Download Filter Preferences:** Configurable minimum file size threshold for automatic browser download interception.
- **Direct Store & Add-on Links:** Direct action buttons in Settings to install browser extensions from the Chrome Web Store and Firefox Add-ons (AMO).

### Changed
- Streamlined browser extension permissions for accelerated store verification and Firefox AMO compliance.
- Upgraded `rustls` to 0.23.46 resolving TLS key share prediction advisory.
- Updated third-party licenses inventory.

### Fixed
- Resolved Settings page freeze caused by PowerShell registry lookups by implementing native direct winreg IPC.
- Resolved notification dismissal behavior: interactive clicks and action buttons now immediately dismiss notifications across the UI and browser extension.
- Resolved TOCTOU race condition in dynamic chunk scheduler and improved lockless boundary tracking.

## [0.2.0-beta.2] - 2026-10-01

### Added
- **Browser Extension Integration:** Native messaging bridge and browser extension for Google Chrome, Microsoft Edge, and Mozilla Firefox.
  - Context menu integration: Right-click any link or file to download with Conflux.
  - Optional auto-interception of browser downloads with configurable exclusions and file extension filters.
  - Secure credential forwarding: Passes session cookies, referer, and user-agent headers so authenticated downloads succeed.
  - Safe cookie boundary: Origin-matching protection prevents cookie leakage if a download URL is edited in the dialog.
  - Handoff queue: Rapid sequential downloads from the browser queue gracefully in the Add Download dialog.
  - Settings page integration: Native messaging host status indicator, setup guide link, and on-demand registration button.
- Project website at https://manjeet2k.github.io/conflux/: frosted-glass hero that fits the first screen on any device, direct download links, a speed calculator, a "pull the plug" simulation, FAQ and social preview cards. Fonts are self-hosted; the page makes no third-party requests except GitHub's API for the latest release.

## [0.2.0-beta.1] - 2026-10-01

First public beta. **Windows 10/11 (64-bit) only. The installer is not code-signed yet**, so
Windows SmartScreen will say "unknown publisher" (More info -> Run anyway); verify the download
with `SHA256SUMS.txt`. Expect rough edges and please report bugs.

### Added
- **Bonded downloads across all your connections.** Wi-Fi, Ethernet and USB/phone tethering are
  used at the same time; each connection takes byte ranges as fast as it can. Adapters that appear
  or disappear mid-download are joined or dropped without restarting, and an adapter that fails
  shows *why* (for example "connect timed out" or "no route to host") on the Network page.
- **In-app updates** (Settings -> About & Updates, plus an optional quiet check at start).
  Running downloads are paused before installing and resume automatically after the restart.
- **Adapter choices by name.** Enable or disable an adapter once; the choice survives address
  changes. If every adapter is disabled, starting a download tells you so instead of silently
  using the default route.
- **Safer downloads on Windows:** NTFS sparse pre-allocation, Mark-of-the-Web (`Zone.Identifier`)
  on finished files, and filenames sanitised so a server cannot write outside your folder or use
  reserved Windows names.
- **Support tools:** rotating log files with URLs, credentials and your Windows username redacted,
  "Copy diagnostics" and "Open logs folder" in Settings, a single running instance, and a backup of
  any settings or history file that cannot be read instead of silently resetting it.
- Installer: publisher, licence page, bundled third-party licence notices, and an uninstall
  prompt that never touches your downloaded files. An offline-WebView2 installer is also published.

### Changed
- Stricter webview security (content security policy, minimal permissions).
- Quitting (or closing the window with "close to tray" off) now pauses downloads and saves state
  first.
- The Add dialog says that links are checked automatically after you paste or type them.

### Fixed
- A finished download could be truncated after pause or quit during the final integrity check; the
  file is now completed only when every byte is verified, and a cancelled check resumes without
  re-downloading.
- A hostile or corrupt server-reported size no longer makes the app allocate huge amounts of
  memory; the download fails cleanly.
- Credentials and tokens in URLs no longer appear in error messages, notifications or logs.
- A brief network drop on a single adapter no longer kills the whole download.
- Settings could be overwritten by a concurrent save; pause/remove right after start could leave a
  download in the wrong state; adapter-scan errors were silently ignored.
- Adapter add/remove events could be dropped under load; two downloads could pick the same file
  name or collide with a resume file.
- Various UI state, dialog and formatting fixes.

## [0.1.5]

Baseline for this changelog. Earlier history is in `git log`: system tray with background
downloading, pause/resume persistence, Fluent UI redesign and multi-adapter chunk visualiser,
and the multi-chunk download engine and CLI.
