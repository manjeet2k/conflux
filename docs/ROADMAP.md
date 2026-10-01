# Conflux Production Roadmap

Goal: take Conflux from "works on the developer's machine" to a **publicly distributed Windows
beta** with an auto-updating, repeatable release process.

> **Release target (user decision): BETA, not 1.0.** Versions stay `0.x`; every release is
> published as a GitHub **prerelease** named `0.N.0-beta.K`, and the README/installer/app say
> "beta". Code signing and the 1.0 checklist are **deferred until after the beta** (see
> [Post-beta backlog](#post-beta-backlog)). Do not tag, label or describe anything as 1.0 or
> "stable" until the user says so.
>
> **Platform target (user decision): Windows 10/11 x64 only.** Linux code paths exist only as a
> dev/test convenience (see [DEVELOPMENT.md](DEVELOPMENT.md#platform-policy)); do not add Linux or
> macOS features, packaging or CI release jobs.

This file is the single source of truth for that work. It is written so that **any agent can
open it, pick the next unblocked task, finish it, and leave the file accurate for the next
agent.** Read [How to use this roadmap](#how-to-use-this-roadmap) first.

Last reviewed against the repo: version `0.1.5`, branch `main`.

---

## How to use this roadmap

### Picking a task
1. Read `AGENTS.md` (binding rules for this repo) and the [Ground rules](#ground-rules) below.
2. Run `node scripts/roadmap-status.mjs --todo` for the current task list, then find the first task with status `[ ]` whose **Depends on** tasks are all `[x]` and whose
   **Gate** (if any) is resolved in [Open decisions](#open-decisions).
3. Mark it `[~]` (in progress) and add your agent/session name in **Owner** *before* you start,
   so parallel agents don't collide. If a task is already `[~]`, pick another.
4. Work only on files listed under **Files**. If you must touch others, note why in your report.

### Finishing a task
1. Satisfy every line of **Acceptance** and run every command under **Verify**.
2. Run the repo quality gates ([DEVELOPMENT.md](DEVELOPMENT.md#quality-gates)). They must pass.
3. Change the status to `[x]` and fill **Done notes** (what changed, what was *not* verified,
   follow-ups). Be honest: say "compiled but not run on Windows" when that is the case.
4. If you discover new work, **add a new task** (next free ID in that phase) instead of
   silently expanding yours. If you discover a decision the user must make, add it to
   [Open decisions](#open-decisions) and mark the dependent tasks `[!]` (blocked).
5. Do **not** commit unless the user asked you to. Do **not** build the release `.exe` /
   installer unless the user explicitly asks (AGENTS.md, Superpower 7). CI-driven release
   workflows (R-tasks) may *define* an installer build but must not run one locally.

### Status legend
`[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (reason in Done notes) · `[-]` dropped (reason in Done notes)

### Task format
```
#### ID — Title                                              status
Why:        the problem, in one or two sentences
Depends on: task IDs (or none)         Gate: open decision ID (or none)
Files:      where the work happens
Do:         concrete steps
Acceptance: observable, checkable outcomes
Verify:     exact commands / manual steps
Done notes: (filled in by the agent that finishes it)
```

---

## Ground rules

These come from `AGENTS.md`; they are repeated because they cause the most mistakes.

- **Never make unilateral design decisions.** A fork in the road (dependency, UI framework,
  default, API contract) goes to the user with options, pros/cons and a recommendation.
- **Tests assert invariants.** Write the failing test first, see it fail, then fix.
- **No speculative complexity.** Simplest end-to-end path first.
- **Inspect, don't guess.** Log and verify real addresses, byte ranges, status codes.
- **Never run bare `cargo test`** on Linux/WSL2. Always `-p <crate>`.
- **Version bumps** touch all three: `Cargo.toml` `[workspace.package].version`,
  `crates/conflux-desktop/tauri.conf.json` `version`, `ui/package.json` `version`.
- **Concurrency etiquette:** several agents may edit the same working tree. Stay inside your
  task's files; if a build breaks in a file you don't own, wait and retry, don't "fix" it.

### Environment and quality gates
The dev environment (WSL2, what can and cannot be tested on Linux) and the **full list of quality
gates** live in [DEVELOPMENT.md](DEVELOPMENT.md). Run every gate there before reporting a task
done, and run `cargo test -p conflux-core` three times when you touched engine/watcher code.

Project state that matters for picking tasks:
- **Unit/integration tests now run as real Windows executables** from WSL2 (`scripts/test-windows.sh`: core 72+45, cli 4, desktop 65 pass). That covers the Windows-only code paths in the test suites (NTFS sparse, adapter enumeration, watcher, Zone.Identifier).
- **Still unverified on real Windows:** the running app and webview (strict CSP, trimmed capabilities, the manifest with long-path support), tray, installer/uninstaller, the updater, real multi-adapter bonding. That is what V-1, V-2, V-4 and V-5 cover.

---

## Open decisions

Tasks marked **Gate: Dn** must not start until the user answers. Each entry lists the
recommended default. If the user hasn't answered, **ask** (don't assume).

| ID | Question | Options | Recommendation | Answer |
|----|----------|---------|----------------|--------|
| D1 | Platforms for the beta | Windows only · +Linux · +macOS | Windows only | **Windows 10/11 x64 only** (answered; Linux code kept only as a test/dev host) |
| D2 | Code signing route | Azure Trusted Signing · OV cert · EV cert · unsigned beta | Azure Trusted Signing if identity can be verified | **Unsigned for the beta** (answered). Signing route to be chosen after the beta — S-1/S-2 deferred. The beta must warn users about SmartScreen (see D-1) |
| D3 | Publisher identity | Publisher name, security contact email, final repo URL | — (user must supply) | **Answered:** publisher "Manjeet Singh", security contact manjeetsgh11@gmail.com, repo `github.com/manjeet2k/conflux` (private now; must be public before the beta so updates can download) |
| D4 | Updates | In-app auto-update · manual download only | In-app auto-update (`tauri-plugin-updater`) | **In-app auto-update** (answered) |
| D5 | Beta scope | Include browser-capture extension / proxy / speed limit / scheduling? | Not in the beta | **Resolved by the "beta, not 1.0" decision**: no extras are required for the beta. Browser-capture extension and speed limit/scheduler (ticked earlier) are post-beta candidates — P-7 stays deferred until the user picks them |
| D6 | Telemetry | None · opt-in crash reports | None (app makes no telemetry calls today) | **None** (answered) |
| D7 | Strict 206 validation | Keep: any ETag/Last-Modified mismatch (or absence) on a 206 fails the download · Loosen: fail only on mismatch, let `If-Range` cover a missing header | Decide after V-2 finds real-world CDN behaviour | _unanswered_ |

---

## Before the first beta

Code and docs for the beta are in place; what is left needs you or a Windows machine:
1. ~~Back up `~/.config/conflux-secrets/`~~ (done by the maintainer). ~~Hardening~~ applied: `release` environment limited to `main` + `v*.*.*` holds the signing secrets, tag ruleset `protect-release-tags` is active. **Still to do when the repo goes public:** add required reviewers to the `release` environment (unavailable on private repos on this plan). Optional: `VIRUSTOTAL_API_KEY`.
2. Run V-1, V-2, V-4, V-5 on Windows (kits in `docs/WINDOWS_TEST_PLAN.md`, `scripts/bench/`) and fix what they find.
3. Add screenshots/GIF and benchmark numbers (D-1).
4. ~~Run `ci.yml` once by hand~~ — done: the Windows job passed (the Linux job was removed).
5. Make the repo public (updates cannot download from a private repo), then follow `docs/RELEASING.md` for B-1.

## Phase map

```
P0 Hygiene ──► P1 Verify on Windows ──► P2 Product gaps ──┐
        └────► P3 CI / release pipeline ──────────────────┼──► P4 Sign & update ─► P5 Distribute ─► P6 Beta
```
P1, P2 and P3 can run in parallel after P0. For the beta, P4 means **S-3 (auto-update) and S-4 (runbook) only**; S-1/S-2 (signing) are deferred. P5 needs P3.

---

## Phase 0 — Repo hygiene (≈½ day)

#### H-1 — Remove committed stray files and harden `.gitignore`  `[x]`
Why: `conflux_panic.txt` is tracked in git; `Conflux.exe`, `Conflux-Setup.exe`, `WebView2Loader.dll` sit in the repo root (gitignored, but they confuse contributors and could be published by mistake).
Depends on: none
Files: `conflux_panic.txt`, `.gitignore`, repo root
Do:
1. `git rm conflux_panic.txt`; delete the three untracked binaries from the working tree **only after confirming with the user** they aren't needed (they may be the user's last hand-built release).
2. Add `conflux_panic*.txt`, `*.pdb`, `*.msi`, `/dist-installer/`, `*.pfx`, `*.p12`, `*.key`, `updater*.key*` to `.gitignore`.
Acceptance: `git ls-files | grep -iE "panic|\.exe|\.dll|\.pfx|\.key"` prints nothing; `git status` clean after.
Verify: command above.
Done notes: `conflux_panic.txt` untracked, `.gitignore` extended, and the stray `Conflux.exe`, `Conflux-Setup.exe`, `WebView2Loader.dll` deleted from the repo root (user approved).

#### H-2 — Fix repo metadata  `[x]`
Why: `Cargo.toml` `repository`, `authors`, and `ui/package.json` carry placeholder-ish identity data.
Depends on: none
Files: `Cargo.toml`, `ui/package.json`, `crates/*/Cargo.toml`, `README.md`
Do: set `repository`, `homepage`, `authors`, `description`, `keywords`, `categories` consistently; make the README's badge/links point at the real repo; set `publish = false` in crates not meant for crates.io.
Acceptance: `cargo metadata --no-deps` shows the same repository/authors for all workspace crates.
Verify: `cargo metadata --no-deps --format-version 1 | jq '.packages[] | {name,repository,authors}'`
Done notes: Consistent repository/homepage/description/keywords/categories across crates and `ui/package.json`; `cargo metadata --no-deps` OK.

#### H-3 — Single version source + bump script  `[x]`
Why: the version lives in three files and must move together (AGENTS.md Superpower 6). Humans forget.
Depends on: none
Files: new `scripts/bump-version.mjs` (Node is already required for `ui/`), `scripts/check-version.mjs`, `ui/package.json` (scripts)
Do: script takes `X.Y.Z[-pre]`, rewrites the three files (and `Cargo.lock` via `cargo update -w` or `cargo check`), refuses a non-semver or lower version. A second script exits non-zero if the three differ. Add `npm run version:check` / `version:bump`.
Acceptance: bump then check passes; hand-editing one file makes check fail.
Verify: `node scripts/bump-version.mjs 0.1.6 && node scripts/check-version.mjs && git diff --stat` then revert.
Done notes: `scripts/bump-version.mjs`, `check-version.mjs`, `lib.mjs`; `npm run version:check|version:bump`; `check-version.mjs --expect X.Y.Z` is for the release workflow. Bump also syncs Cargo.lock workspace crates and `ui/package-lock.json` (currently stale at 0.1.0, so the first real bump changes it). Tested bump → check → break → restore.

#### H-4 — Docs reconciliation  `[x]`
Why: README project tree, `docs/archive/PLAN.md` (original design incl. `socket2` language), and `docs/ARCHITECTURE.md` have drifted from the code.
Depends on: none
Files: `README.md`, `docs/ARCHITECTURE.md`, `docs/archive/PLAN.md`
Do: update architecture doc for: interface-name overrides, If-Range/ETag validation, `claim_unique_path`, watcher fallback polling, `quit_gracefully`, Linux `SO_BINDTODEVICE`. Mark `docs/PLAN.md` as historical. Link `docs/ROADMAP.md` from README.
Acceptance: every module named in ARCHITECTURE.md exists; no mention of removed behaviour.
Verify: manual read + `git grep -n "socket2" docs README.md`.
Done notes: ARCHITECTURE.md rewritten against the code (sections 5–7 new); PLAN.md marked historical; cross-compile doc notes the windres PATH requirement. Found: SHA-256 is computed but never compared to an expected value (doc reworded). Window close calls `quit_gracefully` only when close-to-tray is off.

---

## Phase 1 — Verify on real Windows (≈2–3 days, biggest risk)

> These tasks need a Windows 10/11 machine or VM (clean, no dev tools for V-3). An agent running
> on Linux/WSL2 **cannot complete them**: it should write the scripts/checklists and mark the
> run itself `[!]` "needs Windows host", leaving exact instructions for the human.

#### V-1 — Run desktop tests and smoke-test the app on Windows  `[~]`
Why: the running app is unverified on Windows (unit tests already run natively via `scripts/test-windows.sh`).
Depends on: none
Files: new `docs/WINDOWS_TEST_PLAN.md`
Do: write a checklist, then (on Windows) run it and record results:
1. `cargo test -p conflux-desktop` (includes migration, `set_override`, `partial_file_is_ours`, Zone.Identifier tests).
2. `npm --prefix ui run build` then `cargo run -p conflux-desktop --release`; confirm UI renders (CSP), tray works, window controls work (minimize/maximize/close/drag).
3. Add a download; confirm completion, history persists across restart, notification shows.
4. Check the file has `Zone.Identifier` (`Get-Content file -Stream Zone.Identifier`).
5. Confirm preallocated file is sparse (`fsutil sparse queryflag <file>`).
6. Settings: change theme/chunk size, toggle an adapter, restart → toggle survives; change adapter's IP (renew DHCP) → toggle still applies.
7. Close window with `close_to_tray` off and on; Quit from tray with 3 active downloads → exits fast, downloads resume afterwards.
Acceptance: every checklist line has PASS/FAIL + notes; every FAIL has a new task filed.
Verify: results table committed in `docs/WINDOWS_TEST_PLAN.md`.
Done notes: Kit ready: `docs/WINDOWS_TEST_PLAN.md` (desktop tests, 23-row smoke test). **Not run — needs a Windows machine.**

#### V-2 — Real multi-adapter bonding benchmark  `[~]`
Why: the whole product claim is aggregated bandwidth; it has only been tested against loopback aliases.
Depends on: V-1 (app launches)
Files: new `scripts/bench/` (PowerShell + a small Rust or Node range-capable test server), `docs/BENCHMARKS.md`
Do: serve a large file from a LAN/VPS you control with `Accept-Ranges`; download via (a) Ethernet only, (b) Wi-Fi only, (c) both, (d) both + phone tethering. Record per-adapter throughput from the app's own stats and from Windows `Get-NetAdapterStatistics`. Include a CDN-hosted file to observe ETag/Last-Modified behaviour on 206s (informs **D7**).
Acceptance: `docs/BENCHMARKS.md` has a table with link speeds, observed aggregate, efficiency %, test date, Windows build; notes any adapter that connected but moved 0 bytes.
Verify: raw logs attached (`RUST_LOG=conflux_core=debug`).
Done notes: Kit ready: `scripts/bench/` (range server tested with curl; `measure-adapters.ps1` never run) and `docs/BENCHMARKS.md` methodology. **Results table still empty — needs real adapters on Windows.**

#### V-3 — Weak-host / no-gateway adapter diagnostics  `[x]`
Why: on Windows an adapter without its own default gateway may accept `bind(ip)` yet fail to connect; today the user just sees a stalled adapter with no explanation.
Depends on: V-2 findings
Files: `crates/conflux-core/src/engine.rs`, `adapter.rs`, `crates/conflux-desktop/src/commands.rs`, `ui/src/pages/NetworkPage.tsx`, `ui/src/types.ts`
Do: surface per-adapter last error and drop reason (e.g. "connect timed out", "no route") in `AdapterProgress`/`AdapterInfo` and show it on the Network page and in the details pane. Optionally pre-flight each adapter with a tiny ranged probe when a download starts.
Acceptance: unplugging gateway / using a no-gateway adapter shows a human-readable reason in the UI within the stall timeout; unit/integration test covers the error plumbing (use `fail_peer_ip` in the test server).
Verify: gates + manual check from V-2 setup.
Done notes: `AdapterProgress.last_error`/`drop_reason` + `describe_failure` (Linux errno and Windows WSA codes) → desktop (URLs redacted) → Network page and details pane. Tests: strengthened `failing_adapter_is_dropped_and_others_finish`, new `removed_adapter_reports_drop_reason`, 3 unit tests. Pre-flight probe skipped (startup latency/races). **Windows mapping untested on a real no-gateway adapter.** `last_error` clears on the next successful chunk; the text only shows while a task is downloading.

#### V-4 — Clean-machine install/uninstall test  `[~]`
Why: installer behaviour with and without WebView2, per-user install, upgrade-over-previous.
Depends on: R-3 (an installer artifact from CI)
Files: `docs/WINDOWS_TEST_PLAN.md`
Do: on a fresh Windows 10 and 11 VM: install, launch, upgrade from previous version, uninstall. Confirm user downloads are never deleted; settings/history handling matches what the uninstall page says.
Acceptance: results recorded; failures filed as tasks.
Verify: checklist.
Done notes: Checklist ready in `docs/WINDOWS_TEST_PLAN.md` (16 rows incl. WebView2-absent, offline installer, upgrade, uninstall). **Not run — needs clean Windows 10/11 VMs and an installer from a release.**

#### V-5 — Soak and chaos run  `[~]`
Why: confirm resilience claims (AGENTS.md Superpower 3) on real hardware.
Depends on: V-1
Files: `docs/WINDOWS_TEST_PLAN.md`
Do: 10+ GB file; disable/enable Wi-Fi mid-download; sleep/resume laptop; kill the process (check resume from sidecar); fill the disk; revoke the save folder; run 10 concurrent downloads for an hour. Verify SHA-256 against the server's.
Acceptance: all runs end correct or fail with a clear error; none produce a corrupt file reported as complete.
Verify: SHA-256 comparison logged per run.
Done notes: Checklist ready in `docs/WINDOWS_TEST_PLAN.md` (12 rows with SHA-256 comparison). **Not run — needs Windows hardware.**

---

## Phase 2 — Product gaps (≈3–4 days)

#### P-1 — Single-instance guard  `[x]`
Why: two launches share one `downloads.json`/`settings.json` and fight over files.
Depends on: none
Files: `crates/conflux-desktop/Cargo.toml`, `src/lib.rs`
Do: add `tauri-plugin-single-instance`; on second launch, show/focus the main window (use the same path as the tray "show"). Pass any CLI args (URL) through for later protocol-handler work.
Acceptance: second launch exits immediately and the first window comes to front (also from tray).
Verify: compile-check for Windows; manual on Windows (V-1).
Done notes: `tauri-plugin-single-instance` registered first in `lib.rs`; second launch shows the main window and emits `second-instance` (args logged redacted). **Not verified on Windows** — second process exit / window focus (V-1).

#### P-2 — Persistent logging  `[x]`
Why: release builds have no console; support is impossible without logs. The panic file goes to `%TEMP%` and is overwritten.
Depends on: none
Files: `crates/conflux-desktop/src/lib.rs`, `src/main.rs`, `Cargo.toml` (`tracing-appender`)
Do: write daily-rotating logs (keep ~7 files) to `app_log_dir`; keep the env-filter; redact URL query strings and credentials in logs (`https://u:p@host/?token=…`); move the panic hook output into the same directory with a timestamp and include version + OS; add `open_logs_folder` command (use `opener` from Rust, no JS permission needed).
Acceptance: after a run, a log file exists in the app log dir with startup info; a forced panic writes a timestamped file there; URLs in logs have no credentials/query.
Verify: unit test for the URL redaction function; manual on Windows.
Done notes: New `logging.rs` + `redact.rs`: daily-rotating `conflux.<date>.log` (keep 7) in app log dir, redacting writer (URL credentials + query strings), panic hook writes `panic-<UTC>.txt` with version/OS/backtrace, `open_logs_folder` command. Limitation: tokens in URL *paths* are not redacted. Scratch-crate run: URL/IP/path redaction + end-to-end file test passed.

#### P-3 — "Copy diagnostics" + "Open logs" in Settings  `[x]`
Why: makes beta bug reports actionable.
Depends on: P-2
Files: `crates/conflux-desktop/src/commands.rs`, `ui/src/pages/SettingsPage.tsx`, `ui/src/api.ts`, `ui/src/types.ts`, `ui/src/dev/mockBackend.ts`
Do: command `get_diagnostics` returning app version, OS/build, WebView2 version, adapter list (names, enabled, link state — **no IPs beyond the subnet-masked form**), settings (minus paths), last 20 error lines. UI button copies it to the clipboard; second button opens the logs folder. Update the mock backend to match.
Acceptance: pasting the output into an issue reveals no usernames/paths/credentials.
Verify: unit test that the diagnostics struct excludes paths/IPs; UI build.
Done notes: `diagnostics.rs` + `get_diagnostics`; Settings → Support has "Copy diagnostics" and "Open logs folder"; mock backend updated. Allow-listed payload (masked subnets, no paths, last 20 WARN/ERROR lines scrubbed). Gaps: no real link-state field (only `usable`), OS has no build number, interface names are included verbatim (e.g. "John's iPhone").

#### P-4 — Safer data files  `[x]`
Why: a corrupt `downloads.json` is silently replaced, losing the user's history.
Depends on: none
Files: `crates/conflux-desktop/src/history.rs`, `settings.rs`
Do: on parse failure, rename the bad file to `*.corrupt-<timestamp>` (keep at most 3), start empty, and surface a one-time notice in the UI. Same for settings. Add a `schema_version` field to both files for future migrations.
Acceptance: test writes garbage, loads, finds a `.corrupt-*` backup and an empty/default state; version field round-trips and old files without it still load.
Verify: `cargo clippy` for desktop and `scripts/test-windows.sh desktop` (see [DEVELOPMENT.md](DEVELOPMENT.md)) — or on Windows.
Done notes: New `fileutil.rs`: unparseable file → `<name>.corrupt-<UTC>` (keep 3, never overwrite); `schema_version` added to settings (v1) and history (`{"schema_version":1,"tasks":[…]}`; bare array still loads); `take_startup_notices` command → warning toast in `App.tsx`.

#### P-5 — Installer polish  `[x]`
Why: first impression + support burden.
Depends on: none
Files: `crates/conflux-desktop/tauri.conf.json`, new `crates/conflux-desktop/installer/*.nsh` if needed
Do: set `bundle.publisher`, `copyright`, `shortDescription`, `longDescription`, license file shown in installer (`bundle.licenseFile`), `nsis.installerIcon`; choose `webviewInstallMode` (keep `downloadBootstrapper`, plus offline variant via a second CI build — see R-4); uninstall page: ask whether to delete settings/history, never touch downloads; start-menu shortcut; optional "launch after install".
Acceptance: installer shows publisher and license; uninstall leaves the download folder intact.
Verify: V-4.
Done notes: Publisher, copyright, descriptions, licence file, bundled third-party licences in `tauri.conf.json`; `installer/hooks.nsh` adds a keep-by-default prompt for settings/history on top of Tauri's own checkbox and never touches downloads. **NSIS hook never compiled; Start-menu shortcut assumed to be the default — verify in V-4.**

#### P-6 — Robustness backlog from the review  `[x]`
Why: items from the full-app review not yet addressed.
Depends on: none
Files: various (see list)
Do (each is its own commit-sized change; add a test first where feasible):
- `claim_unique_path` placeholder leak: on any early failure in `start_download` after reserving, remove the empty placeholder and release the reservation.
- Engine: `unique_path` is still used by pure helpers; make `claim_unique_path` the only public naming API to avoid regressions.
- Desktop `classify_kind` uses only the adapter *name*; reuse the core `looks_virtual`/Description/IfType signal so a TAP adapter named "Local Area Connection 2" is labelled virtual in the UI.
- Settings page: warn when `default_save_dir` is on a drive that no longer exists.
- A real Linux uplink named `br0` is disabled by default — add a "why is this disabled?" hint on the Network page.
Acceptance: one test or manual step per bullet, recorded in Done notes.
Verify: quality gates.
Done notes: All items done: `claim_unique_path` is the only public naming API; CLI placeholder leak fixed (discover adapters before claiming the file); desktop `start_download` has no fallible step between reservation and spawn.

#### P-7 — Optional features  `[-]` deferred past the beta
Why: the competitors (IDM/FDM) ship these; not needed for the beta.
Depends on: user picks features after the beta
Do: when the user picks any, split into separate tasks first (browser extension + native messaging host; `conflux://` protocol handler; proxy settings; global speed limit; scheduler; start-with-Windows). Each needs its own design note and user sign-off (AGENTS.md Superpower 4).
Acceptance: user-approved scope list added here as P-7a, P-7b, ...
Done notes:

---

## Phase 3 — CI / release pipeline (≈2 days, can start immediately)

#### R-1 — Pull-request CI  `[x]`
Why: nothing enforces the quality gates today (no `.github/`).
Depends on: none
Files: new `.github/workflows/ci.yml`, new `rust-toolchain.toml`
Do: jobs — (a) **linux**: fmt, clippy core/cli with `--tests`, `cargo test -p conflux-core`, `cargo test -p conflux-cli`, UI lint+build; (b) **windows**: `cargo clippy` for core/cli/desktop with `--tests -D warnings`, `cargo test -p conflux-core -p conflux-cli -p conflux-desktop` (desktop tests *can* run here), UI build. Cache with `Swatinem/rust-cache` and `actions/setup-node` npm cache. Pin the Rust toolchain in `rust-toolchain.toml`. Run integration tests 3× on Linux to catch timing flakiness (or use `cargo nextest --retries 0 --repeat`).
Acceptance: a PR with a deliberate fmt error fails; a clean PR passes on both OSes in < ~15 min.
Verify: open a draft PR; screenshot or link the run in Done notes.
Done notes: `.github/workflows/ci.yml` (Windows-only job: format, version check, UI, clippy and tests for core/cli/desktop; the Linux job was removed on the maintainer's request) and `rust-toolchain.toml` pinned to 1.98.1. **Not verified:** the workflows have never run; Linux commands were run locally and pass. Expect tuning on first run.

#### R-2 — Supply-chain checks  `[x]`
Why: a download manager is a high-trust app; dependency risk matters.
Depends on: R-1
Files: `.github/workflows/security.yml`, `.github/workflows/dependency-report.yml`, `deny.toml`
Do: `cargo audit`, `cargo deny check` (licenses allow-list MIT/Apache-2.0/BSD/ISC/MPL/Unicode; deny unknown registries), `npm audit --omit=dev --audit-level=high`; weekly schedule + on PR. Dependabot for `cargo`, `npm` (ui), `github-actions`.
Acceptance: workflow green on `main`; a known-bad test advisory fails it (try on a branch).
Verify: workflow run link.
Done notes: `security.yml` (cargo-deny, rustsec/audit-check, npm audit; PR/push/weekly), `deny.toml`. `cargo deny check` passes locally. **Not run:** `cargo audit`, `npm audit`. Added `CDLA-Permissive-2.0` to the allow-list (needed by webpki-roots) and ignored RUSTSEC-2024-0370 (proc-macro-error, unmaintained, Tauri build-time) — **pending user OK**. **Update (cost control):** Dependabot version-update PRs are disabled (they triggered CI/Security per PR); Dependabot alerts stay on, security-fix PRs off. `dependency-report.yml` keeps one "Dependency report" issue up to date monthly; the weekly `cargo-audit` run opens issues for advisories. CI skips docs-only changes; Security runs weekly or when dependency files change.

#### R-3 — Release workflow  `[x]`
Why: installers must come from CI, not a developer laptop.
Depends on: R-1, H-3
Files: new `.github/workflows/release.yml`
Do: trigger on tag `v*.*.*`. Steps: checkout → `node scripts/check-version.mjs` (tag must equal version) → setup Rust (MSVC, `x86_64-pc-windows-msvc`) and Node → `tauri-apps/tauri-action` builds the NSIS bundle → compute SHA-256 → create a **draft** GitHub Release with the installer, `SHA256SUMS.txt`, and release notes extracted from `CHANGELOG.md` → upload SBOM (`cargo cyclonedx`). Prerelease flag for tags containing `-`.
Acceptance: pushing `v0.1.6-test` on a fork produces a draft release with installer + checksums; version mismatch aborts.
Verify: workflow run on a test tag. (This task *defines* the build; it is the one sanctioned place an installer is produced — by CI, not locally.)
Done notes: `.github/workflows/release.yml` (tag `v*.*.*` / manual; Windows build; always a draft prerelease; SHA256SUMS; changelog notes; SBOM; `publish-updater` job copies `latest.json` to `updater-beta` only when the draft is published). **Never run; tauri-action inputs for this layout, the NSIS filename for a prerelease version, and the SBOM install are unverified.**

#### R-4 — Offline-WebView2 installer variant  `[x]`
Why: locked-down/offline machines can't use the downloading bootstrapper.
Depends on: R-3
Files: `release.yml`, a second Tauri config override (e.g. `tauri.offline.conf.json`)
Do: build a second NSIS artifact with `webviewInstallMode: { "type": "offlineInstaller" }` (note: it is large, ~130 MB — publish as a separate asset).
Acceptance: release has two installers, named clearly.
Verify: V-4 on an offline VM.
Done notes: `tauri.offline.conf.json` + second build in `release.yml`, published as `_x64-offline-setup.exe`, no updater artifacts. Unverified (never built).

#### R-5 — Changelog and release-notes discipline  `[x]`
Why: users and the updater both need human-readable notes.
Depends on: none
Files: new `CHANGELOG.md` (Keep a Changelog format)
Do: seed `0.1.5` → current `Unreleased` from `git log`; document the rule "every user-visible change adds a line under Unreleased"; add that rule to `AGENTS.md` under Superpower 6 **only with user approval**.
Acceptance: `CHANGELOG.md` exists with an `Unreleased` section; R-3 extracts from it.
Verify: R-3 test tag.
Done notes: `CHANGELOG.md` (Keep a Changelog) with Unreleased + 0.1.5 baseline. The "every user-visible change adds a line" rule is stated in the changelog only; adding it to AGENTS.md needs user approval.

---

## Phase 4 — Signing and updates (work ≈1–2 days; certificate lead time is days to weeks)

> **Start the certificate paperwork on day 1** — it is the longest pole and needs the user.

#### S-1 — Obtain a code-signing identity  `[-]` deferred until after the beta   Gate: D2, D3
Why: unsigned installers trigger SmartScreen "unknown publisher" and many AV false positives.
Depends on: D2, D3
Do (human + agent): the user completes identity verification with the chosen provider; agent documents the exact CI secrets/OIDC setup in `docs/RELEASING.md`. Never ask the user to paste private keys into chat; use GitHub Actions secrets / OIDC federation.
Acceptance: a test signing run succeeds in CI.
Done notes:

#### S-2 — Sign binaries and installer in CI  `[-]` deferred until after the beta
Depends on: S-1, R-3
Files: `release.yml`, `tauri.conf.json` (`bundle.windows.signCommand` or `certificateThumbprint`)
Do: sign `conflux.exe` and the NSIS installer (SHA-256, RFC 3161 timestamp). Fail the release if unsigned.
Acceptance: `Get-AuthenticodeSignature` shows `Valid` and the right publisher on both files; timestamp present.
Verify: command above on a downloaded artifact.
Done notes:

#### S-3 — Auto-update  `[x]`
Why: shipping fixes quickly is mandatory for a networking tool, especially in a beta. Note: the updater signature (its own key) is separate from code signing and **is** required even while the installer is unsigned.
Depends on: R-3, D4 (answered: in-app auto-update)
Files: `crates/conflux-desktop/Cargo.toml`, `src/lib.rs`, `tauri.conf.json` (`plugins.updater`), `capabilities/default.json`, UI (`SettingsPage.tsx`, `StatusBar.tsx`), `release.yml`
Do:
1. Generate the updater key pair (`tauri signer generate`); public key in `tauri.conf.json`; private key + password only in CI secrets. **Back up the private key — losing it strands every installed copy.**
2. Add `tauri-plugin-updater`; release workflow publishes `latest.json` + signatures.
3. UI: "Check for updates" in Settings + a quiet startup check (configurable); show release notes; **never interrupt active downloads** — pause them via `pause_all_internal`, install, relaunch, and auto-resume.
4. Add only the updater permissions the JS uses to `capabilities/default.json`.
Acceptance: install `v0.1.6-test`, publish `v0.1.7-test`, app updates itself and resumes a download that was in flight.
Verify: manual on Windows; unit test for the "pause before install" ordering where possible.
Done notes: `updater.rs`: check/install in Rust, pause→persist→install ordering with rollback, resume at startup, notify-only startup check; Settings → About & Updates. Updater key generated locally (`~/.config/conflux-secrets/`), public key in `tauri.conf.json`. Its unit tests run as part of the desktop suite (65 pass as a Windows exe via `scripts/test-windows.sh`). **Real update flow never exercised** (needs two published betas on a public repo).

#### S-4 — Release runbook  `[x]`
Depends on: R-3, S-3 (S-2 later, when signing lands)
Files: new `docs/RELEASING.md`
Do: step-by-step: update changelog → bump version → tag → CI → verify signature → publish draft → smoke test the update path → announce. Include rollback ("yank" a release, withdraw `latest.json`), key rotation, and who holds which secret.
Acceptance: a person who has never released can follow it end to end.
Done notes: `docs/RELEASING.md` full runbook (repo must be public first; key backup/rotation; SmartScreen; rollback).

---

## Phase 5 — Distribution and trust (≈2 days)

#### D-1 — README / landing page  `[~]`
Depends on: V-2 (honest numbers)
Files: `README.md`, optionally `docs/index.md` (GitHub Pages)
Do: **a clear "Beta" banner and a "Windows protected your PC / unknown publisher" section explaining the unsigned installer (More info → Run anyway) plus the SHA-256 to verify the download**; screenshots, a short GIF of bonding with the per-adapter speed pills, install instructions, system requirements (Windows 10 1809+/11, WebView2), an honest "how much gain to expect" note (needs independent physical links with their own gateways), FAQ (why is SmartScreen showing…, how updates work, where data is stored).
Acceptance: a new user can install and complete a first bonded download from the README alone.
Done notes: README rewritten (beta banner, unsigned-installer guidance, checksum verification, FAQ, requirements). **Still needs real screenshots/GIF and benchmark numbers.**

#### D-2 — Policy and community files  `[x]`
Files: `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `PRIVACY.md`, `.github/ISSUE_TEMPLATE/*`, `.github/pull_request_template.md`
Do: security contact + disclosure window; privacy statement matching D6 (today: no telemetry; app talks only to URLs the user adds, plus the update endpoint if S-3); issue templates asking for "Copy diagnostics" output; PR template with the quality-gate checklist.
Acceptance: files exist; PRIVACY.md is accurate to the code (grep for network calls: only engine probes/downloads and the updater).
Done notes: `SECURITY.md`, `PRIVACY.md` (verified against code: only the engine's reqwest and the updater make network calls), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, issue/PR templates.

#### D-3 — Third-party licence notices  `[x]`
Why: MIT/Apache dependencies require attribution; the installer should carry it.
Depends on: none
Files: new `THIRD_PARTY_LICENSES.md` (generated), `scripts/gen-licenses.*`, `tauri.conf.json` (`bundle.resources`)
Do: generate with `cargo about` + `license-checker`; fail CI if a dependency has a licence outside the allow-list (reuse `deny.toml`); show it in Settings → About.
Acceptance: file generated in CI and bundled; Settings → About lists app version, licences link, repo link.
Done notes: `scripts/gen-licenses.mjs` (+ `--check`) generates `THIRD_PARTY_LICENSES.md` (410 components); bundled and linked from About.

#### D-4 — Package-manager listings  `[-]` deferred until after the beta
Depends on: S-2, a published signed release
Do: submit a `winget` manifest PR (`microsoft/winget-pkgs`); optionally Scoop bucket and Chocolatey. Automate manifest updates from the release workflow (`winget-releaser` action).
Acceptance: `winget install <id>` installs the signed build.
Done notes:

#### D-5 — Antivirus reputation  `[x]`
Depends on: R-3 (S-2 later)
Do: upload each release to VirusTotal in CI and fail loudly on > N detections; document the Microsoft Defender false-positive submission process in `docs/RELEASING.md`.
Acceptance: process documented; first release's scan result recorded.
Done notes: VirusTotal step in `release.yml` (skipped without `VIRUSTOTAL_API_KEY`); Defender false-positive steps in `docs/RELEASING.md`. Unverified (never run).

---

## Phase 6 — Public beta (1–2 weeks calendar)

#### B-1 — Publish `0.2.0-beta.1`  `[ ]`
Depends on: R-3, S-3, S-4, P-2, P-3, V-1, D-1 (beta/SmartScreen wording), D-2 (privacy + security contact; needs D3)
Do: bump with `npm run version:bump -- 0.2.0-beta.1` (only when the user asks for a release); the release workflow must mark it a GitHub **prerelease**; cut it via the runbook; recruit 10–20 testers across Windows versions and ISPs; track issues with a `beta` label; maintain `docs/KNOWN_ISSUES.md`.
Acceptance: unsigned beta prerelease is downloadable, installs per-user, self-updates to the next beta, and the README says "beta" and explains the SmartScreen warning.
Done notes:

#### B-2 — Triage and fix beta findings  `[ ]`
Depends on: B-1
Do: file every finding as a task here (F-1, F-2…), prioritise data-loss/wrong-file bugs first, regression test for each.
Done notes:

#### B-3 — Stable-release (1.0) readiness checklist  `[-]` deferred; not the current target
Depends on: B-2, V-4, V-5, D-1, D-2, D-3, S-4, **S-1/S-2 (signing)**
Only start when the user decides to leave beta. Exit criteria (all must be true):
- [ ] No open bug that can lose data or report a corrupt file as complete.
- [ ] V-1, V-4, V-5 re-run on the release candidate; results recorded.
- [ ] Installer and exe are signed (`Valid`), timestamped; update path tested from the previous release.
- [ ] `cargo audit` / `cargo deny` / `npm audit` clean or each exception justified in `deny.toml`.
- [ ] README, PRIVACY, SECURITY, CHANGELOG, THIRD_PARTY_LICENSES present and accurate.
- [ ] Benchmarks published with methodology (V-2).
- [ ] D7 decided and the 206-validation behaviour documented.
- [ ] All three version fields equal the tag; `main` is green.
Done notes:

---

## Post-beta backlog

Not part of the current target. Pick up only after the user decides to leave beta or asks for
the item. Task entries above keep their full detail.

- S-1, S-2 — code signing (route still undecided, D2).
- B-3 — 1.0 readiness checklist.
- D-4 — winget/Scoop/Chocolatey listings (better once signed).
- P-7 — optional features (browser-capture extension, speed limit/scheduler, proxy, start with Windows).
- D6 telemetry, D7 strict-206 policy — decide with data from the beta.

---

## Completed work log

Newest first. Add a line whenever you finish a task, so the history survives even if the
task entry is later reorganised.

- Independent fresh-context review (three reviewers) → fixes: finished-download truncation after pause/quit, hostile-size OOM guard, sidecar-name collision, credential redaction in errors, reliable adapter events, resume-after-update retry, release workflow guards (published-tag rebuild, manifest verification, SHA-pinned actions, locked builds), doc corrections. Open design question: disabling the last enabled adapter mid-download (see `commands.rs` comments).
- Target changed from 1.0 to a public **beta**: signing, 1.0 checklist, winget and optional features deferred (Post-beta backlog).
- H-3, H-4, P-1, P-2, P-3, P-4, R-1, R-2, R-5 done; H-1, P-6 partly done (see Done notes). Full gates pass on the combined tree; Windows-runtime behaviour and the CI workflows themselves are unverified.

- _(before this roadmap)_ Full-app review and fixes: settings-overwrite race, single-adapter blip
  handling, start/pause race, adapter-scan error handling, interface-name overrides,
  If-Range/ETag validation, `SO_BINDTODEVICE`, persister/cancel leaks, NTFS sparse files,
  link-state filtering, filename hardening, atomic output-path claiming, strict CSP and trimmed
  capabilities, Mark-of-the-Web, graceful quit, UI settings/state fixes. See `git log`.

---

## Appendix A — Useful commands

```
# Gates (see DEVELOPMENT.md for the full list)
cargo fmt --check && cargo test -p conflux-core

# Windows compile-check from Linux/WSL2
PATH=<dir-with-x86_64-w64-mingw32-windres>:$PATH cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu --tests -- -D warnings

# Debug logging for an engine run
RUST_LOG=conflux_core=debug cargo run -p conflux-cli -- download <url>

# Browser-only UI dev (uses ui/src/dev/mockBackend.ts)
npm --prefix ui run dev
```

## Appendix B — Where things live

| Area | Path |
|------|------|
| Engine (chunking, workers, resume, If-Range) | `crates/conflux-core/src/engine.rs`, `chunk.rs`, `resume.rs`, `writer.rs` |
| Adapters, link-state, watcher | `crates/conflux-core/src/adapter.rs`, `watcher.rs` |
| Filename safety / atomic claim | `crates/conflux-core/src/filename.rs` |
| Engine tests + fault-injecting test server | `crates/conflux-core/tests/` |
| Tauri commands, task lifecycle | `crates/conflux-desktop/src/commands.rs`, `state.rs`, `lib.rs`, `tray.rs` |
| Settings / history persistence | `crates/conflux-desktop/src/settings.rs`, `history.rs`, `adapters.rs` |
| Webview security | `crates/conflux-desktop/tauri.conf.json`, `capabilities/default.json` |
| UI | `ui/src/` (`App.tsx`, `hooks/`, `components/`, `pages/`, `dev/mockBackend.ts`) |
| CLI | `crates/conflux-cli/src/main.rs` |
