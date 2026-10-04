# Conflux Codebase Review Remediation Plan

**Date:** 2026-10-04  
**Target Release:** v0.2.0-beta.3  
**Status:** Fully implemented & verified (Batch 1 + Rework R1 & R2 completed)  
**Scope:** Remediation of high-severity and medium-severity findings from the Opus and Antigravity reviews, incorporating confirmed user architectural decisions and subagent security enhancements.

---

## 1. Architectural Decisions (Confirmed by User)

| Decision | Topic | Selected Decision | Core Rationale |
|---|---|---|---|
| **D1** | **IPv6 Adapter Binding** | **Constrain bound workers to the adapter's IP family** (IPv4-bound workers resolve and connect only to IPv4 A records). | Prevents dual-stack CDNs from silently routing traffic out the default gateway via IPv6 `[::]:0`, guaranteeing true multi-adapter bonding without the complexity of managing rotating IPv6 privacy addresses. |
| **D2** | **Session Cookie Storage** | **Do not persist cookies to disk in `.conflux.json`**; require browser re-handoff if an authenticated download needs resume. | Eliminates credential leakage from public Downloads folders (often synced to OneDrive). Prevents stale session tokens from corrupting resumed downloads. |
| **D3** | **Resume on Range Loss / Probe Divergence** | **Abort resume with an explicit error and preserve the partial file and sidecar intact** (zero data loss). | Never wipe, truncate, or overwrite a multi-GB partial file when a probe returns 200 without Range support (e.g. login redirect, transient server error, or probe timeout). |
| **D4** | **Hyper-V `vEthernet` Uplink** | **Distinguish external uplink switches from internal/NAT switches** by inspecting default gateways. | Preserves the host's primary physical connection when Hyper-V External Virtual Switch is active, while continuing to filter host-only switches (e.g., WSL, Default Switch). |

---

## 2. High-Severity Remediation (Core Integrity & Data Safety) — ✅ DONE

### H1: Prevent IPv6 Default-Gateway Bypass for Bound Adapters ✅
- **Target Files:** [`crates/conflux-core/src/adapter.rs`](../crates/conflux-core/src/adapter.rs#L401-L421), [`crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs)
- **Implementation:** Custom `FamilyFilteredResolver` filters DNS results by bound adapter's IP family. Attached only when `local_ip` is `Some(ip)`.
- **Tests Added:** `test_family_filtered_resolver_ipv4`

### H2: Zero-Data-Loss Resume Protection & Ephemeral Cookie Handling ✅
- **Target Files:** [`crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L922-L930), [`crates/conflux-core/src/resume.rs`](../crates/conflux-core/src/resume.rs#L25)
- **Implementation:** Resume + no-range-support → bail with file/sidecar preserved. Sidecar `cookie` field is always written as `null` (Decision D2).
- **Tests Added:** `single_stream_resume_preserves_partial_file_and_aborts_on_range_loss`, `sidecar_never_persists_cookies_to_disk`

### H3: Direct `final_url` Targeting & Cross-Origin Header Sanitization ✅
- **Target Files:** [`crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L669-L695)
- **Implementation:** Both `probe_with_headers` and `probe_head` set `DownloadProbe.url = final_url`. Cross-origin redirects strip `cookie` from headers.
- **Tests Added:** `probe_records_final_url_and_chunks_target_final_endpoint`

### H4: Prevent Paused Download Filename Collisions ✅
- **Target Files:** [`crates/conflux-desktop/src/commands.rs`](../crates/conflux-desktop/src/commands.rs#L1216-L1242)
- **Implementation:** `reserve_output_path` takes `claimed_paths: &[PathBuf]` (all task save paths) and checks for existing `.conflux.json` sidecars.
- **Tests Added:** Paused-task collision test, sidecar-on-disk collision test

---

## 3. Medium-Severity Remediation — ✅ DONE

### M1: Redact Hex-Encoded Payloads in Second-Instance Log ✅
- **Target File:** [`crates/conflux-desktop/src/lib.rs`](../crates/conflux-desktop/src/lib.rs#L66-L88)
- **Implementation:** `--from-browser` argument's value replaced with `"<redacted-payload>"` before logging.

### M2: Browser Extension Referer Privacy ✅
- **Target File:** [`extensions/conflux-browser/background.js`](../extensions/conflux-browser/background.js#L44-L60)
- **Implementation:** `sanitizeReferer()` sends origin-only for cross-origin, strips fragment for same-origin. Applied at both callsites.

### M3: Distinguish Disk Full / I/O Errors from Network Errors ✅
- **Target File:** [`crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L226-L228)
- **Implementation:** `AttemptError::Disk(io::Error)` variant. `fetch_chunk` catches writer errors and returns `Disk`. Worker stops download immediately without retrying or penalizing adapters. `ChunkedShared.disk_error` carries the message to the join point.

### M4: Browser Native Bridge "Open Conflux" ✅
- **Status:** Verified already implemented. No changes needed.

### M5: Add Download Dialog Key React Remount ✅
- **Target File:** [`ui/src/components/AddDownloadDialog.tsx`](../ui/src/components/AddDownloadDialog.tsx#L84)
- **Implementation:** Key includes `${props.queueCount ?? 0}`. Prop threaded from `App.tsx`.

### M6: Hyper-V `vEthernet` Gateway Detection (Decision D4) ✅
- **Target File:** [`crates/conflux-core/src/adapter.rs`](../crates/conflux-core/src/adapter.rs#L137-L153)
- **Implementation:** `has_gateway: bool` on `InterfaceAddress`, populated from `FirstGatewayAddress` on Windows. `classify_interface` treats `vEthernet` + gateway + no internal markers as enabled.
- **Tests Added:** `test_classify_hyperv_external_switch_with_gateway`

---

## 4. Verification and Quality Gates

Every change must pass the repository's strict quality gates:
1. `cargo fmt --check`
2. `cargo clippy -p conflux-core --all-targets -- -D warnings`
3. `cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu -- -D warnings`
4. `cargo test -p conflux-core` (currently 126 tests: 76 unit + 50 integration)
5. `npm --prefix ui run lint && npm --prefix ui run build`

---

## 5. Rework Items (From Post-Implementation Review) — ✅ DONE

These two items were identified during the post-fix review on 2026-10-04 as missing or
incomplete in the batch 1 implementation. Both have been implemented and verified with tests.

---

### R1: Signal `retire_tx` When Adapter Is Dropped After Consecutive Failures ✅

- **Severity:** High (Review finding H2 from `CODE_REVIEW_2026-10-04-agy.md`)
- **Target File:** [`crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L1627-L1636)
- **Root Cause:** When an adapter reaches `MAX_CONSECUTIVE_ADAPTER_FAILURES`, the worker
  at line 1628 calls `adapter.drop_with_reason(...)` and returns. However, it never calls
  `adapter.retire_tx.send_replace(true)`. Sibling workers on the same adapter are blocked
  inside `fetch_chunk` in the `tokio::select!` on `wait_for_true(retire)` (line 1490). Since
  `retire_tx` is never signaled, they wait for the full 20-second `stall_timeout` before
  timing out naturally, one at a time. During this entire stall window, their assigned chunks
  are locked in `ChunkStatus::Downloading`, preventing healthy adapters from claiming them.

- **How the Code Currently Looks (line 1627–1636):**
  ```rust
  if !adapter.dropped.load(Ordering::SeqCst) {
      adapter.drop_with_reason(format!(
          "dropped after {failures} consecutive failures"
      ));
      error!(
          "Dropping adapter {} for this download after {} consecutive failures",
          adapter.label, failures
      );
  }
  return;
  ```

- **Exact Fix:** Add one line — `adapter.retire_tx.send_replace(true);` — immediately after
  the `adapter.drop_with_reason(...)` call, inside the same `if` block, before the `error!`
  log. This is consistent with how `retire_tx` is signaled in two other places in the same
  file:
  - `AdapterUpdate::Remove` handler at line 1185: `target.retire_tx.send_replace(true);`
  - `retire_default_route()` at line 1670: `fallback.retire_tx.send_replace(true);`

- **Fixed code should look like:**
  ```rust
  if !adapter.dropped.load(Ordering::SeqCst) {
      adapter.drop_with_reason(format!(
          "dropped after {failures} consecutive failures"
      ));
      adapter.retire_tx.send_replace(true);
      error!(
          "Dropping adapter {} for this download after {} consecutive failures",
          adapter.label, failures
      );
  }
  return;
  ```

- **Regression Test (optional, noted in review doc §7 item 2):** An integration test
  that drops an adapter mid-download and asserts sibling workers release chunks promptly
  (well under the 20s stall timeout). Not strictly required for this one-line rework.

- **Verification:** `cargo test -p conflux-core` must still pass all 126 tests. Specifically
  confirm `failing_adapter_is_dropped_and_others_finish` still passes (it exercises the
  adapter-drop path but currently succeeds only because the stall timeout eventually fires;
  after the fix, it will complete faster).

---

### R2: Content-Range Total Size Mismatch Must Return `ResourceChanged`, Not `Failed` ✅

- **Severity:** Medium (Review finding M2 from `CODE_REVIEW_2026-10-04-agy.md`)
- **Target File:** [`crates/conflux-core/src/engine.rs`](../crates/conflux-core/src/engine.rs#L1775-L1782)
- **Root Cause:** When a chunk response's `Content-Range` header reports a different total
  file size than the probe saw (`total != shared.total_bytes`), the code returns
  `AttemptError::Failed(...)`. Since `Failed` is retryable, the worker retries the same chunk
  up to 5 times with exponential backoff, re-downloading bytes that cannot possibly match
  the pre-allocated file size. After exhausting retries it increments
  `adapter.consecutive_failures`, potentially dropping a healthy adapter for a server-side
  resource change. The correct error is `AttemptError::ResourceChanged`, which immediately
  stops the download on first occurrence (no retries, no adapter penalty).

- **How the Code Currently Looks (line 1775–1782):**
  ```rust
  if let Some(total) = parsed.total {
      if total != shared.total_bytes {
          return Err(AttemptError::Failed(anyhow!(
              "remote size changed: Content-Range total {} != probed {}",
              total,
              shared.total_bytes
          )));
      }
  }
  ```

- **Exact Fix:** Change `AttemptError::Failed` to `AttemptError::ResourceChanged` on line
  1777. Nothing else changes — the error message and format arguments stay the same.

- **Fixed code should look like:**
  ```rust
  if let Some(total) = parsed.total {
      if total != shared.total_bytes {
          return Err(AttemptError::ResourceChanged(anyhow!(
              "remote size changed: Content-Range total {} != probed {}",
              total,
              shared.total_bytes
          )));
      }
  }
  ```

- **Why this is safe:** `AttemptError::ResourceChanged` is already handled in
  `run_chunk_worker` (line 1556–1571): it subtracts counted bytes, releases the chunk,
  records the error in `shared.resource_changed`, signals `stop_tx`, and returns. The
  post-join check in `download_chunked` (line ~1256) reads `resource_changed` and bails
  with a clear message. All of this is already tested by
  `same_size_change_mid_download_fails_via_etag_compare` and
  `same_size_change_mid_download_fails_via_if_range`.

- **Verification:** `cargo test -p conflux-core` must still pass all 126 tests.
