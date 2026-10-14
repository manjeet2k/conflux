# System review fixes

Date: 2026-10-15
Branch: `codex/system-review-fixes`
Base commit: `1a2b010eac66e4a4e4e1a363ab0e95af3c76e67d`
Merged application version: **0.2.8** (review branch: 0.2.7). Browser extension version: **0.2.2**.

## Approved behavior

- Parallel downloads and block resume require verified ranges, a known size, and a strong ETag. Other transfers use a fresh single stream.
- Automatic file deletion verifies the recorded Windows file identity. Replacement files and files with uncertain ownership are preserved.
- Browser confirmation remains. Automatic interception cancels the browser transfer only after the desktop accepts a task.
- Protocol URLs retain their trailing slashes.
- A matching primary strong ETag permits an omitted secondary Last-Modified header; contradictory supplied validators fail.

## Finding disposition

| Finding | Implemented change | Main files |
| --- | --- | --- |
| H1: unvalidated resume and multipart mixing | Require strong ETags for parallel/resume; bind sidecars to a resource URL digest; restart unvalidated transfers with a whole response. | `engine.rs`, `resume.rs` |
| H2: deletion based on sidecar or file length | Record file identity from the created file handle. Validate identity on an exclusive Windows handle before deletion; preserve uncertain files. Verify resume ownership too. | `file_identity.rs`, desktop `commands.rs`, `state.rs` |
| H3: missing external Hyper-V gateways | Include the gateway discovery flag in GetAdaptersAddresses. | `adapter.rs` |
| M1: browser transfer cancelled before acceptance | Propagate launch failures; correlate confirmation with native handoff receipts; acknowledge a running or completed task before cancellation. Dismissal, timeout, and failure preserve the browser transfer. | `browser_bridge.rs`, `commands.rs`, extension `background.js` |
| M2: automatic redirect Referer leaks signed query | Disable reqwest-generated Referer headers while retaining explicitly supplied browser context. | `adapter.rs` |
| M3: partial or short whole responses accepted | Require HTTP 200 for a whole stream and enforce known byte counts even when Content-Length is omitted. | `engine.rs` |
| M4: unusable explicit CLI adapter selection falls back | Reject explicit selections without a usable matching adapter. | CLI `main.rs` |
| M5: adapter update batches reorder | Use one delivery pump per download to preserve batch order under backpressure. | `adapter_updates.rs`, `commands.rs` |
| M6: adapter toggles during probing are missed | Reconcile adapter selection after the probe under the adapter snapshot lock before registering the handle. | `commands.rs` |
| M7: protected resume probe lacks persisted headers | Restore permitted Referer/User-Agent before probing; retain original authorized origin across redirects; never persist cookies. | `resume.rs`, `commands.rs` |
| M8: concurrent stop/remove orphans a partial file | Wait for the existing stop operation through a durable watch notification before removing the task and deleting its owned file. | `commands.rs`, `state.rs` |
| M9: queue growth remounts the Add form | Key the form by stable request identity, independent of queue count. | `App.tsx`, `AddDownloadDialog.tsx`, `downloadQueue.ts` |
| M10: stale submission closes another dialog | Advance the queue only when completion matches the current request ID. | `App.tsx`, `downloadQueue.ts` |
| M11: links queued behind Remove are stranded | Present queued requests when the other modal closes. | `App.tsx`, `downloadQueue.ts` |
| L1: unknown-length completion reports zero bytes | Record the completed output file's actual length in task history. | `commands.rs` |
| L2: Unicode hex input panics | Parse ASCII hex byte pairs without slicing UTF-8 strings. | `browser_bridge.rs` |
| L3: protocol trailing slash removed | Preserve the exact target URL instead of trimming its final slash. | `browser_bridge.rs` |
| L4: unsplit range returns before response EOF | Retain the final buffer until EOF; reject delayed surplus bytes or errors. Informational finding hardened. | `engine.rs` |
| K1: secondary 206 validator unnecessarily mandatory | Accept missing secondary validators when the primary strong ETag matches; compare secondary values when both are supplied. | `engine.rs`, `resume.rs` |

Source paths above are relative to `crates/conflux-core/src` unless qualified. Desktop paths are under `crates/conflux-desktop/src`; frontend paths are under `ui/src`; the extension is under `extensions/conflux-browser`.

## Validation

- Formatting check passed.
- Core Clippy passed with all targets and warnings denied.
- Windows desktop Clippy passed with warnings denied.
- Linux core suite passed three consecutive runs: **90 unit + 66 integration tests** per run.
- Native Windows core suite passed: **89 unit + 64 integration tests**.
- CLI tests passed on Linux and native Windows: **5 tests** each.
- Native Windows desktop suite passed: **84 tests**, including a final run after the acceptance check was tightened.
- Frontend queue/dialog tests passed: **6 tests**. Extension handoff tests passed: **5 tests**.
- Frontend lint and production build passed. Vite retains its existing large-chunk advisory.
- Application version synchronization, documentation links, and whitespace checks passed.

Validation was repeated with an isolated build directory (`CARGO_TARGET_DIR=/tmp/conflux-system-review-build`) to avoid artifacts from another checkout. The Windows desktop test runtime required the built `WebView2Loader.dll` beside the test executable in `debug/deps`; all 84 tests passed after that test setup correction.

The merged `main` tree at application version 0.2.8 was checked again: formatting, both Clippy gates, three Linux core runs, CLI tests, all native Windows suites, frontend/extension tests, frontend lint/build, documentation links, and version synchronization passed.

## Practical limits

Automated tests cover the recorded regressions. A real browser/WebView confirmation flow and physical Hyper-V/network adapter transitions still need manual smoke testing. The changes were prepared in the separate review worktree and merged into `main`, preserving its newer Windows UI changes.
