## Summary & Context

<!-- Provide a clear, concise overview of what this PR does and why. -->
<!-- Explain the problem solved from first principles (Karpathy Rule 1 & 2). -->

- **Issue / Task reference:** <!-- e.g., Fixes #123, ROADMAP D-2, or PLAN_ADAPTIVE_CHUNKING -->
- **Component(s) touched:** <!-- e.g., conflux-core, conflux-desktop, ui, extensions, docs, ci -->

---

## Type of Change

<!-- Mark the appropriate type: -->
- [ ] 🚀 **Feature** (non-breaking change introducing new functionality)
- [ ] 🐛 **Bug fix** (non-breaking fix resolving an issue)
- [ ] ⚡ **Performance** (improving throughput, I/O, latency, or memory allocation)
- [ ] ♻️ **Refactor** (code cleanup or restructuring without behaviour change)
- [ ] 📝 **Documentation** (documentation, architectural specs, or comments)
- [ ] 🔧 **Chore / Maintenance** (dependencies, CI, scripts, versioning, template)

---

## Technical Details & Architecture Decisions

<!-- Describe key architectural decisions, invariants preserved, and trade-offs considered. -->
<!-- Note: If there was a fork in the road, describe user alignment (Superpower 4). -->

---

## Verification & Data (Karpathy Rule 3 & Superpower 1, 3)

<!-- Never guess; inspect the data! -->
- **Environment:**
  - [ ] WSL2 / Linux dev host (unit & integration tests)
  - [ ] Windows 10/11 native (smoke / manual / network testing)
- **Data Inspected:**
  - Exact requested offsets vs received byte counts verified?
  - HTTP status codes & headers validated (206, `Range`, `If-Range`, `Content-Range`)?
  - Real-time throughput / adapter logs observed?
- **Tests Added / Updated:**
  - <!-- List unit/integration tests asserting mathematical invariants (Superpower 1) -->
  - <!-- List fault-injection/resilience tests added (Superpower 3) -->

---

## UI / UX Changes (if applicable)

<!-- If frontend was modified, include before/after screenshots or describe UI state changes. Otherwise write N/A. -->

---

## Quality Gates Checklist (Superpower 5 & 6)

Ensure all quality gates pass locally before requesting review:

- [ ] Version bumped sequentially (e.g. `0.2.x`) via `node scripts/bump-version.mjs <new-version>`
- [ ] `node scripts/check-version.mjs` passes (all 5 version sources match)
- [ ] `node scripts/check-docs.mjs` passes (no broken doc links)
- [ ] `cargo fmt --check` passes (clean formatting)
- [ ] `cargo clippy -p conflux-core -- -D warnings` passes (clean core linting)
- [ ] `cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu -- -D warnings` passes (clean desktop linting)
- [ ] `cargo test -p conflux-core` passes (all unit and integration tests pass on Linux host)
- [ ] `npm --prefix ui run lint && npm --prefix ui run build` passes (clean frontend types and bundle)

---

## Platform & Safety Invariants

- [ ] **Windows-Only Policy:** No Linux/macOS desktop features, packaging, or system dependencies added (`docs/DEVELOPMENT.md`).
- [ ] **Data Integrity:** Chunk boundaries cover `[0, total_bytes - 1]` with zero gaps/overlaps, exact file size preserved.
- [ ] **No Telemetry / Privacy:** No user telemetry or analytics tracking added.
- [ ] **On-Demand Installer:** No release installer `.exe` built unless explicitly requested (Superpower 7).
- [ ] **Documentation & Changelog:** `CHANGELOG.md` updated under `[Unreleased]` for user-visible changes.
