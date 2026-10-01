# Changelog

All notable changes to Conflux are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

**Rule:** every user-visible change adds a line under `Unreleased`. On release, rename
`Unreleased` to the new version and date (`scripts/bump-version.mjs` keeps version files in sync).

## [Unreleased]

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
