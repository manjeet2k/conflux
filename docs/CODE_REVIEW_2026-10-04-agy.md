# Conflux Codebase Review (2026-10-04)

- **Date:** 2026-10-04
- **Version:** `0.2.0-beta.2`
- **Commit:** [`0394993`](https://github.com/manjeet2k/conflux/commit/0394993)
- **Scope:** Full codebase (`core`, `adapters`, `desktop`, `security`, `ui`)
- **Legend:** **✔ verified** = checked against actual source and dependency behavior with a concrete failure scenario.
- **Tracking Rule:** When resolving an item, mark it **Fixed in `<commit>`**, and document any resulting behavior changes in [`../CHANGELOG.md`](../CHANGELOG.md) and [`ARCHITECTURE.md`](ARCHITECTURE.md).

---

## 1. Quality Gates

| Gate | Target | Result |
|---|---|---|
| `cargo fmt --check` | Workspace | **Passed** (clean formatting) |
| `cargo clippy -p conflux-core --all-targets -- -D warnings` | Core | **Passed** (0 warnings) |
| `cargo test -p conflux-core` | Core unit + integration | **Passed** (74 unit + 48 integration tests = 122 passed, 0 failed) |
| `cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu -- -D warnings` | Desktop (Windows GNU) | **Passed** (0 warnings) |
| `npm --prefix ui run lint` | UI (oxlint) | **Passed** (0 errors, 0 warnings across 33 files) |
| `npm --prefix ui run build` | UI (tsc + vite) | **Passed** (bundle built in 497ms) |

---

## 2. Decisions Needed

| ID | Area | Topic | Options | Recommended Choice |
|---|---|---|---|---|
| **D-R1** | Core / Adapters | **IPv6 Target Handling** | **1. Detect IPv6 targets; bind to usable IPv6 adapters or fall back to unbound OS default routing (`local_ip: None`).**<br>2. Expand adapter discovery to enumerate IPv6 addresses per interface and dual-stack bind per connection. | **Option 1**: Simplest and most robust; preserves multi-adapter aggregation for IPv4 while allowing IPv6 downloads to complete seamlessly via the OS default route. |
| **D-R2** | Desktop | **Stop / Resume Concurrency Contract** | **1. Introduce explicit `TaskStatus::Stopping` and retain handle in `handles` until the engine task fully joins.**<br>2. Add a per-task mutex and generation ID so `resume_task_internal` awaits the winding down task before launching. | **Option 1**: Directly reflects the true asynchronous lifecycle in UI and backend, preventing collision errors and ghost pauses. |
| **D-R3** | Adapters / CLI | **CLI Filter on Disabled/Virtual Adapters** | **1. If `--adapter` matches an adapter, explicitly enable it for that run; if none usable, exit with an error.**<br>2. Reject disabled adapters with an error unless `--include-virtual` is passed. | **Option 1**: Matches user intent when explicitly specifying an adapter on the command line. |

---

## 3. High & Critical Findings

### H1: Binding IPv4 Source Sockets Fails All Connections for IPv6-Only Hosts
- **Where:** [`../crates/conflux-core/src/adapter.rs`](../crates/conflux-core/src/adapter.rs#L401-L412), [`../crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L750-L766) (✔ verified)
- **Mechanism:** In `engine.rs:751`, `is_usable` filters strictly for IPv4 (`adapter.is_ipv4 && adapter.ip.is_ipv4()`), so `select_bind_targets` only selects IPv4 adapters. `build_bound_http_client` binds each worker client to an IPv4 source address via `reqwest::ClientBuilder::local_address(Some(ip))`. In hyper's connector (`split_by_preference`), binding an IPv4 source address causes it to strip all IPv6 destination addresses.
- **Failure Scenario:** A user downloads from an IPv6-only server or hostname with only AAAA records. The initial unbound probe succeeds via default OS routing (`local_ip: None`). When chunk workers launch, hyper cannot connect to the IPv6 address from an IPv4-bound socket, failing immediately with `ConnectError: Network unreachable`. All workers fail, exceeding `MAX_CONSECUTIVE_ADAPTER_FAILURES`, and abort the download despite the probe succeeding.
- **Fix:** If the destination host is IPv6 (or resolves only to AAAA), either bind matching IPv6 adapters or fall back to unbound OS default routing (`local_ip: None`).

### H2: Dropped Adapter Fails to Signal `retire_tx` to Sibling Chunk Workers, Starving Download
- **Where:** [`../crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L1568-L1576) (✔ verified)
- **Mechanism:** When an adapter reaches `MAX_CONSECUTIVE_ADAPTER_FAILURES`, Worker 1 calls `adapter.drop_with_reason(...)` and exits. However, `adapter.retire_tx.send_replace(true)` is never invoked (unlike in `AdapterUpdate::Remove` at line 1150 and `retire_default_route` at line 1610).
- **Failure Scenario:** When an adapter link drops (e.g. Wi-Fi disconnects or mobile tethering is unplugged), Worker 1 drops the adapter. Sibling workers (Worker 2..N) on the same adapter awaiting bytes inside `fetch_chunk` are never signaled and block for the full 20-second `stall_timeout`. During this time, their assigned chunks remain locked in `ChunkStatus::Downloading`, starving healthy adapters from claiming and finishing them.
- **Fix:** Add `adapter.retire_tx.send_replace(true);` alongside `adapter.drop_with_reason(...)` at line 1568.

### H3: Local Disk Full or Permission Errors Classified as Network Failures & Retried Over Wire
- **Where:** [`../crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L229-L233), [`../crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L1744) (✔ verified)
- **Mechanism:** `fetch_chunk` writes to disk via `shared.writer.write_at(offset, bytes).await?`. An `io::Error` (e.g. `ENOSPC` disk full or permission denied) is coerced by `From<anyhow::Error>` into `AttemptError::Failed`.
- **Failure Scenario:** When the target drive fills up, the worker treats disk write failure as a transient chunk error. It re-downloads the same chunk over the network up to 5 times with exponential backoff, increments `adapter.consecutive_failures`, and after 3 failures drops healthy network adapters. If all adapters fail to write, the engine terminates claiming all network adapters dropped rather than reporting disk exhaustion.
- **Fix:** Distinguish disk I/O errors from network errors with a dedicated `AttemptError::Disk(io::Error)`. Disk errors should immediately trigger `shared.stop_tx` and fail the download cleanly without retrying or blaming adapters.

### H4: Second-Instance Launch Logs Hex-Encoded Browser Cookies & Auth Tokens to Disk
- **Where:** [`../crates/conflux-desktop/src/lib.rs`](../crates/conflux-desktop/src/lib.rs#L68-L71), [`../crates/conflux-desktop/src/browser_bridge.rs`](../crates/conflux-desktop/src/browser_bridge.rs#L220-L225) (✔ verified)
- **Mechanism:** When a browser extension forwards a download with session cookies or bearer tokens, `browser_bridge.rs` serializes the payload and passes `--from-browser <hex>` to the desktop executable. In `lib.rs`, `single_instance::init` logs CLI arguments wrapped in `redact::redact_urls`. Because `redact_urls` only scans for `"://"`, the hex-encoded string is untouched.
- **Failure Scenario:** Any browser download intercepted by the extension containing sensitive session cookies (e.g. authenticated sessions under 3 KB) writes the raw hex payload into `conflux.log`. Any local application or user with read access to the app logs can decode full user session cookies using `from_hex`.
- **Fix:** Scrub or mask the `--from-browser` argument (e.g. replace the argument following `--from-browser` with `"<redacted-payload>"`) prior to logging.

### H5: Race Between `stop_task` and `resume_download` Causes False Collision Errors or Ghost Pauses
- **Where:** [`../crates/conflux-desktop/src/commands.rs`](../crates/conflux-desktop/src/commands.rs#L981), [`../crates/conflux-desktop/src/commands.rs`](../crates/conflux-desktop/src/commands.rs#L1009), [`../crates/conflux-desktop/src/commands.rs`](../crates/conflux-desktop/src/commands.rs#L312), [`../crates/conflux-desktop/src/commands.rs`](../crates/conflux-desktop/src/commands.rs#L330) (✔ verified)
- **Mechanism:** `stop_task` removes the task handle from `state.handles` immediately before waiting for `join_handle` to finish (up to 10s). `resume_task_internal` checks `state.handles.contains_key(task_id)`, sees `false`, and proceeds.
- **Failure Scenario:**
  - *Window A:* If the previous task has not exited, `reserved_paths.insert()` fails with `"<path> is being written by another download"`.
  - *Window B:* If `reserved_paths` was released and resume launches a new engine, `stop_task` reaches line 1009 (`pause_if_downloading`), sees `task.status == Downloading`, and unconditionally reverts the task to `Paused` in UI/state while the newly resumed engine continues downloading in the background.
- **Fix:** Retain the handle in `handles` marked in a stopping state until `join_handle` completes so `resume_task_internal` can wait or cleanly report "stopping", and ensure `pause_if_downloading` only pauses if the task was not resumed with a newer generation ID.

### H6: Win32 `CancelMibChangeNotify2` Asynchronous Deregistration Use-After-Free on Watcher Drop
- **Where:** [`../crates/conflux-core/src/watcher.rs`](../crates/conflux-core/src/watcher.rs#L267-L279) (✔ verified)
- **Mechanism:** `OsWatcher::drop` calls `CancelMibChangeNotify2`. Per MSDN documentation, cancellation is asynchronous and in-flight callbacks can execute briefly after `CancelMibChangeNotify2` returns. When `OsWatcher` finishes dropping, `_address_tx: Box<UnboundedSender<()>>` and `_interface_tx` are freed.
- **Failure Scenario:** If network IP or interface changes occur during app exit or watcher teardown, a worker thread executing `unicast_ip_change_callback` dereferences `callercontext as *const UnboundedSender<()>` after free, leading to an access violation crash.
- **Fix:** Wrap callback contexts in an `Arc` (e.g. `Arc<UnboundedSender<()>>`) with an `AtomicBool` active flag to prevent deallocation while threadpool callbacks are in flight.

### H7: External Task Removal Locks UI Dialog System Permanently
- **Where:** [`../ui/src/App.tsx`](../ui/src/App.tsx#L192-L194), [`../ui/src/App.tsx`](../ui/src/App.tsx#L563-L570), [`../ui/src/components/RemoveDialog.tsx`](../ui/src/components/RemoveDialog.tsx#L21-L25) (✔ verified)
- **Mechanism:** In `App.tsx`, `dialogOpenRef.current` tracks `addOpen || removeIds.length > 0`. If `removeIds` contains task IDs that disappear externally (cleared via tray, error cleanup, or completed prune), `RemoveDialog` receives `tasks = []` and renders `<Dialog open={false}>`. Because this close happens via props without `onOpenChange`, `onClose` is never called, leaving `removeIds` non-empty.
- **Failure Scenario:** `dialogOpenRef.current` remains `true` permanently. All future calls to `openAdd` no-op (`if (dialogOpenRef.current) return;`), global URL paste is ignored, and shortcuts (`Ctrl+N`, `Ctrl+F`) become unresponsive. The user cannot add any downloads until restarting the application.
- **Fix:** Add an effect in `App.tsx` to automatically reset `removeIds` to `[]` when none of the specified IDs exist in `tasks`.

---

## 4. Medium Findings

### Security and Privacy
- **M1:** [`../crates/conflux-desktop/src/lib.rs`](../crates/conflux-desktop/src/lib.rs#L122) (✔ verified)
  - **Failure:** Cold start external download log outputs `payload.url` directly without passing it through `redact::redact_url`, potentially leaking query parameters (e.g. presigned S3 URLs, bearer tokens) into `conflux.log`.
  - **Fix:** Wrap `payload.url` in `redact::redact_url(&payload.url)`.

### Core Engine
- **M2:** [`../crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L1715-L1723) (✔ verified)
  - **Failure:** Mid-download `Content-Range` total size changes (`total != shared.total_bytes`) return `AttemptError::Failed` instead of `AttemptError::ResourceChanged`. Workers waste 5 retry cycles with backoff writing mismatched data into a file pre-allocated to the old size.
  - **Fix:** Return `AttemptError::ResourceChanged` to immediately abort the download and notify the user that the remote resource was replaced.
- **M3:** [`../crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L668), [`L735`](../crates/conflux-core/src/engine.rs#L735) (✔ verified)
  - **Failure:** `DownloadProbe.url` retains the initial unredirected URL rather than `final_url`. Chunk requests from all workers repeatedly incur roundtrip redirects (e.g. unencrypted `http://` redirects, GitHub to S3 redirects on every chunk).
  - **Fix:** Populate `DownloadProbe.url` with `final_url.to_string()`.

### Adapters and CLI
- **M4:** [`../crates/conflux-cli/src/main.rs`](../crates/conflux-cli/src/main.rs#L133-L140) & [`../crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L762) (✔ verified)
  - **Failure:** CLI `--adapter` filter matching disabled/virtual adapters (e.g. Tailscale/VPN) silently falls back to default route without warning the user.
  - **Fix:** Enable matched adapters for that run or terminate with an explicit error if no usable adapters match.
- **M5:** [`../crates/conflux-core/src/filename.rs`](../crates/conflux-core/src/filename.rs#L16-L20) (✔ verified)
  - **Failure:** Windows reserved DOS device name `CLOCK$` is omitted from `RESERVED_NAMES`, causing `CreateFile` failures if a server suggests `filename="clock$.txt"`.
  - **Fix:** Add `"CLOCK$"` to `RESERVED_NAMES`.

### Desktop
- **M6:** [`../crates/conflux-desktop/src/commands.rs`](../crates/conflux-desktop/src/commands.rs#L489), [`L530-L536`](../crates/conflux-desktop/src/commands.rs#L530-L536) (✔ verified)
  - **Failure:** `partial_file_is_ours` returns `false` for incomplete single-stream downloads with bytes on disk, permanently leaking partial files on disk when a task is removed.
  - **Fix:** Check `!task.supports_ranges && len <= task.downloaded_bytes` in `partial_file_is_ours`.
- **M7:** [`../crates/conflux-desktop/src/settings.rs`](../crates/conflux-desktop/src/settings.rs#L93), [`L104`](../crates/conflux-desktop/src/settings.rs#L104) (✔ verified)
  - **Failure:** `validate()` checks `path.is_dir()` on `default_save_dir`. If an external drive is unplugged, saving any unrelated UI preference (theme, connections) fails validation.
  - **Fix:** Only check directory existence when `default_save_dir` is explicitly modified, or check only path syntax/absoluteness on preference updates.

### UI
- **M8:** [`../ui/src/components/AddDownloadDialog.tsx`](../ui/src/components/AddDownloadDialog.tsx#L81-L86) (✔ verified)
  - **Failure:** `AddDownloadForm` key is `${props.open}:${props.initialUrl}:${props.initialFilename}`. Enqueuing multiple items with the same URL prevents React remount, retaining dirty form inputs from previous downloads.
  - **Fix:** Append a request ID or counter to the dialog key: `${props.open}:${props.initialUrl}:${props.initialFilename ?? ''}:${props.queueCount}`.
- **M9:** [`../ui/src/utils/sort.ts`](../ui/src/utils/sort.ts#L22-L23) (✔ verified)
  - **Failure:** Descending ETA sort places completed/paused downloads above active downloads, and comparing two zero-ETA tasks evaluates `Infinity - Infinity = NaN`, violating JavaScript strict weak ordering contracts.
  - **Fix:** Partition inactive/zero-ETA downloads to the bottom before comparing numeric ETA values.

---

## 5. Low Findings (Summary)

- **L1 (Core Engine - [`engine.rs:1181-1185`](../crates/conflux-core/src/engine.rs#L1181-L1185)):** Worker task panic logs error but leaves chunk in `ChunkStatus::Downloading`, hanging remaining workers in poll wait.
- **L2 (Filename - [`filename.rs:218-222`](../crates/conflux-core/src/filename.rs#L218-L222)):** Truncation fallback in `truncate_preserving_extension` can leave trailing spaces or dots.
- **L3 (Filename - [`filename.rs:29-39`](../crates/conflux-core/src/filename.rs#L29-L39)):** Unicode line separators (`\u{2028}`, `\u{2029}`) are not filtered in `is_invisible_format_char`.
- **L4 (Desktop - [`tray.rs:56-65`](../crates/conflux-desktop/src/tray.rs#L56-L65)):** Tooltip percentage displays false 100% when active downloads mix streaming (unknown size) and fixed-size tasks.
- **L5 (Desktop - [`tray.rs:100-123`](../crates/conflux-desktop/src/tray.rs#L100-L123)):** Tray icon double-click triggers both click (show) and double-click (hide) on Windows.
- **L6 (Desktop - [`commands.rs:1082-1087`](../crates/conflux-desktop/src/commands.rs#L1082-L1087)):** Concurrent detached tasks for adapter updates can deliver out of order on rapid interface flapping.
- **L7 (UI - [`useDownloads.ts:40`](../ui/src/hooks/useDownloads.ts#L40), [`TitleBar.tsx:77-78`](../ui/src/components/TitleBar.tsx#L77-L78)):** Unhandled promise rejection on unlisten if registration fails.
- **L8 (UI - [`App.tsx:272-277`](../ui/src/App.tsx#L272-L277), [`SettingsPage.tsx:75-76`](../ui/src/pages/SettingsPage.tsx#L75-L76)):** Clicking "View" on update notification toast navigates to Settings with uninitialized `update = null`.

---

## 6. Checked and Found Sound

- **Core Engine:** Chunk boundary coverage `[0, total - 1]` invariant verified; RFC 9110 range syntax verified; atomic byte accounting verified; resume sidecar write-sync-rename ordering verified; cancel during SHA-256 calculation preserves sidecar.
- **Adapters & CLI:** Path traversal (`..`, `\`, `/`), control characters, and reserved Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`, `CONIN$`, `CONOUT$`) are stripped/escaped; APIPA and virtual adapters properly disabled by default.
- **Desktop:** Lock hierarchy (`last_adapters` -> `settings` -> `handles` -> `tasks`) is strictly deadlock-free; atomic persistence safely quarantines corrupted files; Zone.Identifier Mark-of-the-Web stream is preserved for SmartScreen; credentials scrubbed from crash reports.
- **Security & IPC:** Tauri v2 capabilities ([`../crates/conflux-desktop/capabilities/default.json`](../crates/conflux-desktop/capabilities/default.json)) are strictly minimal with zero shell/fs access from JS; native messaging manifests pin extension IDs; updater verified via Ed25519/minisign with version monotonicity checks; GitHub Actions pinned to SHA hashes.
- **UI:** Exact type and serde field alignment between `types.ts`/`api.ts` and Rust structs (`DownloadTask`, `Settings`, `AdapterInfo`, `UpdateInfo`); table keyboard navigation and focus management cleanly bounded.

---

## 7. Gaps in Test Coverage

1. **IPv4 Binding with IPv6 Target:** No integration test asserts behavior when an IPv4-bound socket connector encounters an IPv6-only target.
2. **Sibling Worker Cancellation on Adapter Drop:** No test asserts that sibling workers terminate promptly without waiting for the 20s stall timeout when an adapter reaches failure limit.
3. **Failing Writer / Disk Full Simulation:** No test injects a failing `SparseFileWriter` to assert that disk exhaustion is not attributed to network failure.
4. **Command Race Conditions:** No tests cover concurrent invocations of `pause_download` vs `resume_download` or `stop_task` vs `remove_download`.
5. **Frontend Automated Testing:** The `ui` project lacks unit and component tests (no test runner in `ui/package.json`).
