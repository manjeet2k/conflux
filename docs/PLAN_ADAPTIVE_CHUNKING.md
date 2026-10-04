# Implementation Plan: Dynamic Range Splitting (Sustained Peak Throughput)

> Supersedes the earlier "adaptive chunk size" plan: instead of tuning chunk size, request size is decoupled from the accounting unit.

## Context

Today every worker downloads fixed 4 MiB chunks, sending one HTTP request per chunk (`fetch_chunk`, `crates/conflux-core/src/engine.rs:1680`). At high speeds a chunk lasts about 40–160 ms. Each one is followed by about one round trip of dead air: the worker has to claim the next chunk, send a request and wait for headers. The result is the sawtooth. `docs/BENCHMARKS.md` already shows it: Ethernet alone averaged 20.5 MB/s against a 50.6 MB/s peak.

The original plan, `PLAN_ADAPTIVE_CHUNKING.md`, made chunks bigger. With today's model that trades one problem for three:

- A failed chunk is downloaded again from byte 0 (`ChunkScheduler::fail`, which sets `downloaded = 0`).
- Pause throws away every partial chunk, because the sidecar only records completed chunks.
- A slow adapter holding the last big chunk stalls the finish, because there is no splitting of in-flight chunks.

**Final fix: stop tying the size of each request to the size of the progress-tracking unit.** This is how IDM and aria2 work:

- Each worker streams **one long byte range** (a *segment*) over a single request.
- When a worker goes idle, it **takes over the upper half** of the largest remaining segment.
- Progress, retries and resume work in small fixed **blocks** inside segments, so a failure or pause loses at most the block in progress.
- Chunk size stops being a setting at all. There is nothing to make adaptive.

Decisions taken, since the user delegated "how it has to be done":

- Remove the chunk-size setting from the UI and settings.
- Keep `connections_per_adapter` as the only tuning setting.
- Keep the sidecar on-disk format compatible.
- Drop the `adapter.rs` pool and keepalive tweaks. reqwest's defaults already cover them, and long segments make the issue moot.

## Core model (`conflux-core`)

### Blocks: the accounting unit, internal only
- `block_size_for(total_bytes)` in a new `crates/conflux-core/src/segment.rs` returns the smallest power of two that is at least 1 MiB and gives at most 8192 blocks.
  - Examples: 100 MB → 1 MiB; 20 GB → 4 MiB; 100 GB → 16 MiB.
  - This bounds the size of `chunk_map` sent over IPC, and keeps pause or failure loss to at most one partial block per worker.
- The final block may be short (it ends at `total_bytes`).
- Reuse `try_plan_chunks` / `plan_chunks` from `chunk.rs` to produce the block list, so their existing coverage tests still apply. `Chunk` is a block.

### Segments: the request unit
Each active segment `{ id, start_block, end: Arc<Mutex<SegmentBounds>>, worker_label, attempts }` has this per-segment state:

```rust
struct SegmentBounds { reserved: u64 /* next byte offset accepted */, end: u64 /* exclusive, can only shrink */ }
```

- **Worker side, per network read:** lock, `take = min(len, end - reserved)`, `reserved += take`, unlock. The lock is uncontended and costs about 20 ns.
  - If `take < len`, or `reserved == end`, the segment was cut short by a split. The worker writes what it took, flushes, finishes and drops the response. reqwest then closes that connection; this happens rarely, once per split.
- **Thief side (inside the scheduler):** lock the victim's bounds, `split = max(block_ceil(reserved), midpoint_block(reserved, end))`, set `end = split`, and create the thief segment `[split, old_end)`.
  - Split only if the victim has at least 2 blocks remaining above `block_ceil(reserved)`.
  - Because both sides take the same mutex, ownership of every byte is disjoint by construction.

### `SegmentScheduler`
Replaces `ChunkScheduler` in `chunk.rs`. Delete `ChunkScheduler`, `Claim` and `FailOutcome` there once nothing uses them.

State:
- `blocks: Vec<Chunk>`, where status is `Pending` or `Completed`.
- `active: Vec<Segment>`.
- `pending: Vec<PendingRange { start_block, end_block, attempts, not_before }>`.
- `max_attempts`, `backoff_base`. Keep the existing `backoff_for` formula and constants `MAX_CHUNK_ATTEMPTS` and `DEFAULT_RETRY_BACKOFF`.

API (`pub(crate)`):
- `new(blocks, max_attempts, backoff)`: builds `pending` from the runs of non-completed blocks. On a fresh start that is one range covering everything; on resume it is the holes.
- `claim(now, label) -> Claim { Segment(SegmentLease), Wait(d), Finished }`. In order:
  1. Take the largest ready pending range.
  2. Otherwise split the active segment with the most bytes remaining.
  3. Otherwise `Wait(min(soonest, IDLE_POLL))`.
  4. Otherwise `Finished`.
  - If any range permanently failed, return `Finished`, matching today's behaviour.
  - Startup needs no special case: the first claim takes everything, and each later worker splits the largest segment.
- `commit(segment_id, flushed_offset) -> u64`: marks blocks fully below `flushed_offset` as `Completed`, and returns the bytes newly committed.
  - Called after every buffered flush returns.
  - The short last block counts as complete when `flushed_offset == total_bytes`.
- `finish(segment_id)`: removes the active segment. Only valid once committed up to `end`.
- `fail(segment_id, now) -> FailOutcome`: the remainder `[first_uncommitted_block, end)` goes back to `pending`.
  - If this attempt committed at least one block, attempts reset to 0 and no backoff applies. A tether that drops mid-segment is not a bad range.
  - Otherwise attempts go up by 1 with backoff, and the range becomes permanently `Failed` when attempts reach max.
- `release(segment_id)`: the remainder goes back to `pending` with no penalty. Used on stop or retire.
- Keep `completed_ids()`, `all_completed()`, `incomplete_ids()` and `chunk_map()`.
  - `chunk_map()` stays one char per block: `#` completed; `>` the block currently being written by each active segment; `!` a block in a permanently failed range; `.` otherwise.
  - The UI's `ChunkMap.tsx` works unchanged.

### Worker and fetch (`engine.rs`)
- `run_chunk_worker` becomes `run_segment_worker`. The same loop structure applies, with `claim → fetch_segment → finish/fail/release`.
- All existing outcome handling stays: `Stopped`, `ResourceChanged`, `Disk`, `Failed`, consecutive adapter failures, the last-adapter guard, and retire.
- `retire_default_route` now triggers on the **first committed block** from an explicit adapter, instead of on a completed chunk.
- `fetch_chunk` becomes `fetch_segment`. Keep all of its header and response validation: `If-Range`, the 200-vs-206 rule, ETag and Last-Modified, and an exact `Content-Range` match against the range *as requested*, i.e. `bytes=start-(end_at_request_time-1)`.
- Body loop:
  - **Buffered writes.** Accept bytes through `SegmentBounds` and append them to a `Vec` with 256 KiB capacity (`WRITE_BUFFER_SIZE`).
  - When the buffer is full, at the end of the segment, on a split cut-off, and on stop: `writer.write_at(flush_offset, buf)`, then `scheduler.commit(id, flush_offset)`.
  - Errors map to `AttemptError::Disk` exactly as today.
  - **Byte accounting.** On accept, increment `counters.downloaded` and `adapter.received` right away, so live speed stays real-time, and track `uncommitted += take`. Each `commit` subtracts the newly committed bytes from `uncommitted`.
  - On fail, stop or retire: `counters.downloaded -= uncommitted`. This replaces today's `fetch_sub(counted)`.
  - The final check `downloaded == total_bytes` at `engine.rs:1322` stays as-is.
  - Overlong body (more than the requested range): `Failed`. Body ending short of the current `end`: `Failed`, but any flushed blocks are kept.
  - **Stop flushes first.** On `stop`, flush the buffer and commit before returning `Stopped`, so pausing keeps every whole block written.
- The single-stream path (`download_single_stream`, `engine.rs:1354`) uses the same 256 KiB buffering helper.
- **Logging (Rule 3).** At `debug`, log each segment: adapter, local and remote address, requested range, bytes taken, duration, MB/s, and whether it ended by split, end of range or error. At `debug`, log each split: victim, thief and split offset.

### Progress
`ProgressUpdate` keeps its fields:
- `total_chunks` and `completed_chunks` count blocks.
- `active_chunks` counts active segments.

No struct changes are needed in the desktop or UI.

### Resume sidecar (`resume.rs`): format unchanged
- `ResumeState.chunk_size` now means the block size. Add a doc comment saying so. `completed` stays as `encode_ranges` of block ids, and `SIDECAR_VERSION` stays 1.
- A sidecar written by an old build (block = old chunk) resumes as-is through `open_for_resume` (`engine.rs:955`). One written by a new build is readable by old builds.
- No migration code is needed.

### Engine API
- `DownloadEngine::new(connections_per_adapter)` loses its `chunk_size` parameter. Block size comes from `block_size_for(probe.total_bytes)`.
- Add a `with_block_size(u64)` override, used by tests and kept for diagnostics.
- `check_chunk_plan` at probe time uses the resolved block size.

## Desktop / CLI / UI
- `crates/conflux-desktop/src/settings.rs`: remove `chunk_size_mb`, `MIN_CHUNK_MB`, `MAX_CHUNK_MB` and `chunk_size_bytes()`, and update the tests. Old `settings.json` files still load, because serde ignores unknown fields and the crate has no `deny_unknown_fields`.
- `commands.rs`: `engine_from_settings(connections_per_adapter)` (around :1199). `diagnostics.rs`: drop the chunk size line.
- `crates/conflux-cli/src/main.rs`: drop the chunk-size flag if there is one, and keep the `Chunks:` print, which now shows blocks.
- `ui/src/pages/SettingsPage.tsx`: remove the chunk-size card and the `chunkSizes` list, and reword the footnote to cover connections only.
- `ui/src/types.ts` and `ui/src/dev/mockBackend.ts`: drop `chunk_size_mb`.
- `DetailsPane` text stays `completed/total · N active`.

## Docs
- Replace `docs/PLAN_ADAPTIVE_CHUNKING.md` and `dev/PLAN_ADAPTIVE_CHUNKING.md` with this design.
- Update `docs/ARCHITECTURE.md`. Its chunk-scheduling section (lines 70–121) describes fixed chunks.

## Tests (Superpower 1 and 3)

**Unit tests, `segment.rs`:**
- `block_size_for` table and bounds, covering 0, 1 B, 1 MiB, 100 MB, 20 GB and 100 GB, with at most 8192 blocks.
- Fresh scheduler: the first claim gets the whole range, and with N workers the claims split into disjoint segments that cover everything.
- Split: `split >= block_ceil(reserved)`, the victim's end shrinks, no overlap, and no split below 2 remaining blocks.
- `SegmentBounds` refuses bytes past a shrunk `end`. A concurrent test runs reserving and splitting threads and asserts that ownership is disjoint.
- Fail after progress resets attempts. Fail without progress backs off and becomes `Failed` after max attempts. Release costs no attempt.
- **Property test** (extending `test_scheduler_random_sequence_consistency`): a random interleaving of claim, accept, commit, split, fail, release and finish. After every step, completed ∪ active-remaining ∪ pending covers exactly `[0, n_blocks)` with no overlap, and committed blocks never revert.
- Resume holes: precompleted blocks produce the right pending runs, and `chunk_map` marks them.

**Integration tests, `crates/conflux-core/tests/engine_integration.rs`, reusing `support/mod.rs`:**
- Existing tests: build the engine with `with_block_size(CHUNK)` so there are still 17 blocks.
  - Assertions on `total_chunks` and `chunk_map` stay.
  - `chunk_requests == 17`-style assertions become bounds, e.g. `<= connections + splits + retries`, since requests are no longer one per chunk.
- New tests:
  - **One worker, one adapter → exactly 1 chunk request.** This proves there is no per-chunk turnaround.
  - **Drop mid-body** (`drop_first_n_chunks`): the retry's `Range` starts at a block boundary above 0, `body_bytes_sent < 1.5 × payload`, and the SHA matches.
  - **Slow peer + fast peer** (`stall_peer_first_n_chunks` or `throttle` on 127.0.0.2): the fast adapter takes work from the slow one, the download finishes well before the slow peer alone could, and the SHA matches.
  - **Pause keeps partial segment progress:** `cancelled_partial_download`, then check that the sidecar's completed blocks are non-empty even with 1 worker, then `resume_to_end` downloads fewer than `total - committed + one block` bytes, and the SHA matches.
  - **Old v1 sidecar written with a different chunk size resumes correctly.** The existing test around `tests:1519` covers this; keep it passing.
  - **Adapter retire mid-segment:** the remainder is released, another adapter finishes, and the SHA matches.

## Verification
1. Quality gates (Superpower 5):
   - `cargo fmt --check`
   - `cargo clippy -p conflux-core -- -D warnings`
   - `cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu -- -D warnings`
   - `cargo test -p conflux-core`
   - `npm --prefix ui run lint && npm --prefix ui run build`
2. Before and after on Windows (Rule 3), using the `docs/BENCHMARKS.md` method and the same kernel.org file plus one 2–10 GB file:
   - Record average vs peak MB/s, chunk request count from the debug log, and the SHA-256.
   - Success means the average approaches the peak (no sawtooth), Ethernet + hotspot scaling stays ≥ 89.5%, and the SHA matches.
3. Manual fault checks on Windows:
   - Unplug the tether mid-download and confirm only the in-progress block is re-fetched (debug log ranges).
   - Pause and resume a large file and confirm progress does not jump backwards by more than one block per worker.
4. Commit on `feat/adaptive-chunking-network-throughput`. No installer build (Superpower 7).
