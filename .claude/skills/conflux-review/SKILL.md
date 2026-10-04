---
name: conflux-review
description: Full or area-scoped, read-only bug and security review of the Conflux codebase. Fans out parallel reviewers by area, re-checks every top finding against the code, and produces a ranked report (optionally docs/CODE_REVIEW_<date>.md). Use when asked to "review the codebase", "audit", "find issues", or review one area (core, adapters, desktop, security, ui).
argument-hint: "[all | core | adapters | desktop | security | ui ...] [--doc]"
---

# Conflux codebase review

A review whose findings can be trusted. A finding counts only if it has a concrete failure
scenario that was checked against the real code, and the report says plainly what was
checked and what was not. **This is read-only. Do not fix anything, edit source, or commit.**
Fixing comes afterwards, once the user has decided the open questions (AGENTS.md Superpower 4).

## Arguments
- No area, or `all`: run all five areas below.
- One or more area names: run only those areas.
- `--doc`: also write the report to `docs/` (step 6). Without it, offer to write the doc at the end.

## Step 1: Inventory and gates (run in parallel)

1. Map the code: `git ls-files | xargs wc -l | sort -n`. Note the large files and any new
   modules that the area list below doesn't cover yet, and give those to the nearest area.
2. Start the quality gates **in the background**, writing their output to the scratchpad:
   ```
   cargo fmt --check
   cargo clippy -p conflux-core --all-targets -- -D warnings
   cargo test -p conflux-core
   cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu -- -D warnings
   npm --prefix ui run lint && npm --prefix ui run build
   ```
   Never run a bare `cargo test`, because `conflux-desktop` needs cross-compilation. Record the pass/fail
   result and any warnings, such as the bundle size.

## Step 2: Fan out reviewers (one message, all in parallel)

Launch one `general-purpose` Agent per area. Every prompt must include the **shared contract**,
followed by that area's file list and things to look for.

**Shared contract (paste into every prompt):**
> You are reviewing /home/manjeet/conflux, a Windows-only Rust and Tauri v2 download manager
> that combines bandwidth from several network adapters (see AGENTS.md). This is a READ-ONLY review: do not edit files.
> Look for real bugs only. Check every finding by reading the actual code path, and read
> dependency sources in ~/.cargo/registry if a claim depends on library behaviour. Do not
> speculate. Report each finding with: file:line, severity (critical/high/medium/low), a concrete
> failure scenario (inputs or state leading to the wrong result), and a suggested fix. Rank findings by severity. Skip
> style nits. End with a short "checked and fine" list of the things you verified are correct,
> and a list of gaps in test coverage. Keep the report under about 900 words.

**Areas:**

| Area | Files | Look for |
|------|-------|----------|
| `core` | `conflux-core/src/{engine,chunk,writer,resume,checksum,lib}.rs`, skim `tests/` | chunk gaps or overlaps, off-by-one errors in Range, handling of 200, 206 and 416, a wrong Content-Range, short or over-long bodies, races when chunks are stolen or re-queued, progress counted twice, the order of persisting resume data versus writing the file (fsync), races in cancel and pause, task leaks, locks held across `.await`, overflow, zero-length or unknown-length files, redirects and the final URL, disk errors classified as network errors, checksum gaps |
| `adapters` | `conflux-core/src/{adapter,filename,watcher}.rs`, `conflux-cli/src/main.rs` | socket binding per IP family (an IPv4 bind with an IPv6 destination), bind before connect, DNS family, APIPA, loopback, virtual, no-gateway and multi-IP adapters, filename sanitising (traversal, CON/NUL/COM1, trailing dots and spaces, RFC 5987, length, Unicode), watcher races and leaks, CLI matching that quietly falls back to the default route |
| `desktop` | `conflux-desktop/src/*` except bridge, redact and updater; `capabilities/`, `tauri*.conf.json`, `build.rs` | races between commands (pause, resume, remove, double start), mutexes held across await, non-atomic persistence, path reservation and ownership, deleting or opening files outside the download directory, settings validation and migration, unwrap or panic on paths a user can reach, swallowed errors, over-broad capabilities, CSP |
| `security` | `browser_bridge.rs`, `redact.rs`, `updater.rs`, `installer/*.json`, `extensions/conflux-browser/`, `.github/workflows/`, `scripts/*updater*` | native-messaging trust (allowed_origins, ID pinning, framing, size limits), web pages or other extensions triggering downloads, cookies or Authorization leaking across hosts, redirects, logs (including encoded payloads), files on disk or temp files, gaps in redaction, updater signatures and downgrade attacks, extension permissions, XSS, CI risks (`pull_request_target`, untrusted input in `run:`, unpinned actions, install scripts running next to secrets) |
| `ui` | `ui/src/**` | **drift between `types.ts`/`api.ts` and the Rust command signatures and serde shapes (highest value)**, `listen()` cleanup, stale closures, async races, refs updated in effects, form keys that don't remount, NaN or Infinity in formatters, sort bugs, shortcut conflicts, `invoke` calls with no error handling, state not updated after an action, accessibility blockers |

While the reviewers run, tell the user which areas are in progress. Don't predict their results.

## Step 3: Verify, so the report can be trusted

When each report arrives:
1. **Re-read the code behind every Critical and High finding**, and behind any Medium finding that
   seems surprising. Use `sed -n`/`grep` on the cited lines. If a claim depends on a library,
   read that library's source in `~/.cargo/registry` or `node_modules`. Mark a finding you
   confirmed as **✔ verified**.
2. **Check that the line numbers are plausible**: compare them with `wc -l`. Reviewers sometimes
   cite lines past the end of the file. Correct them, or cite only the file and symbol.
3. Drop any finding you disproved. Mention the drop briefly if it's useful.
4. **Merge duplicates** across reviewers. Cross-area bugs, such as a desktop call site causing an
   engine-level data loss, are usually the most important; list them once, at the higher severity.
5. Never present an unverified claim as fact. Leave it unmarked, or say "plausible".

## Step 4: Severity standard

- **Critical:** remote compromise, or data loss or corruption on a common path.
- **High:** breaks the core promise (bandwidth aggregation, resume, file integrity) or loses data
  in a realistic scenario.
- **Medium:** a security or privacy leak, the whole download failing because of one adapter, a race
  with visible effects, a UI flow getting stuck, an accessibility blocker.
- **Low:** an edge case, hardening, something that only affects the dev host, a performance nit with no visible effect.

## Step 5: Report to the user

Order:
1. **Gate results** in one line.
2. **High** (and Critical), numbered. Each with file:line, mechanism, failure scenario and fix.
3. **Medium**, grouped by area (Security and privacy, Engine, Adapters and CLI, Desktop, UI).
4. **Low**, as a compact summary.
5. **Checked and found sound**: one line per area.
6. **Decisions for you**: every fix that involves a design fork (API contract, default,
   persistence location, behaviour change locked in by an existing test). Give the options with a
   recommendation (AGENTS.md Superpower 4).
7. Offer the next step: fix High first, with a regression test for each. Do not start until the user agrees.

## Step 6: Review doc (when `--doc` is given or the user asks)

Write `docs/CODE_REVIEW_<YYYY-MM-DD>.md` with:
- A header giving the date, version and short commit SHA, the scope, a legend for ✔, and the tracking rule:
  mark each fixed item **Fixed in `<commit>`**, and list behaviour changes in `CHANGELOG.md` and `ARCHITECTURE.md`.
- Gate results.
- A **Decisions needed** table (IDs D-R1…), with the recommended option in bold.
- Findings with stable IDs: **H1…**, **M1…**, and grouped Low items. Each finding has Where / Failure / Fix.
- Gaps in test coverage, and "Checked and found sound".

Then add a row for it in the table in `docs/README.md`, run `git add -N` on the new file, and run
`node scripts/check-docs.mjs` (it must pass). Do not commit unless the user asks.
