# Changelog

All notable changes to Conflux are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

**Rule:** every user-visible change adds a line under `Unreleased`. On release, rename
`Unreleased` to the new version and date (`scripts/bump-version.mjs` keeps version files in sync).

## [Unreleased]

### Added
- In-app updates (Settings -> About & Updates, optional quiet check on start): running downloads
  are paused before installing and resume automatically after the restart. Release workflow
  (`release.yml`) builds a draft prerelease with standard and offline-WebView2 installers,
  checksums and the updater manifest; release runbook in `docs/RELEASING.md`.
- Installer polish: publisher, license page, description, and an uninstall prompt that never
  touches downloaded files. `THIRD_PARTY_LICENSES.md` is generated and bundled.
- Platform policy made explicit: Windows 10/11 (64-bit) only. README, AGENTS.md, architecture,
  development guide and roadmap say so; Linux code paths remain only as a test/dev host.
- Documentation reorganised: docs index (`docs/README.md`), development guide
  (`docs/DEVELOPMENT.md`), guides/ and archive/ folders, `scripts/roadmap-status.mjs` and
  `scripts/check-docs.mjs`, and a corrected README layout and testing section.
- Non-polling network adapter watcher: adapters that appear or disappear are hot-joined to or
  dropped from running downloads without a restart.
- Adapter selection is now an app-level global setting, with runtime fallback when a selected
  adapter goes away.
- Interface-name overrides for friendlier adapter labels.
- Mark-of-the-Web (`Zone.Identifier`) on completed downloads on Windows.
- Version tooling (`scripts/bump-version.mjs`, `scripts/check-version.mjs`) and
  `version:bump` / `version:check` npm scripts.
- Continuous integration (fmt, clippy, tests, UI build on Linux and Windows) and supply-chain
  checks (`cargo deny`, `cargo audit`, `npm audit`) and a monthly dependency report issue; Dependabot alerts on, no update PRs.

### Changed
- Core engine hardened: If-Range/ETag validation so a changed remote file is detected on resume,
  `SO_BINDTODEVICE` on Linux, and handling of single-adapter connection blips.
- Adapter discovery filters out links that are down or in a non-usable state (Windows).
- Windows downloads use NTFS sparse files for pre-allocation.
- Filenames are sanitised and output paths are claimed atomically to avoid collisions.
- Strict Content Security Policy and trimmed Tauri capabilities for the webview.
- Quitting the app now shuts down gracefully, persisting in-flight download state.

### Fixed
- Settings could be overwritten by a concurrent save (race).
- Start/pause race that could leave a download in the wrong state.
- Adapter-scan errors are surfaced instead of silently ignored.
- Persister and cancellation resource leaks.
- Startup crash caused by starting the network watcher outside the Tokio runtime.
- Various UI settings and state fixes.

## [0.1.5]

Baseline for this changelog. Earlier history is in `git log`: system tray with background
downloading, pause/resume persistence, Fluent UI redesign and multi-adapter chunk visualiser,
and the multi-chunk download engine and CLI.
