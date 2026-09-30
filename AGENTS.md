# Conflux Agent & Engineering Guidelines

Welcome to **Conflux**: A high-performance native Windows & cross-platform download manager built in Rust and Tauri v2 that aggregates bandwidth across all available network interfaces (Wi-Fi, Ethernet, USB 4G/5G mobile tethering) and multiple mirror sources.

All agents, contributors, and automated tooling operating within this repository MUST adhere to the principles and guidelines outlined below.

---

## 1. Andrej Karpathy Engineering Rules

### Rule 1: First Principles & Deep Understanding
- Never write boilerplate or cargo-cult code. Understand every layer:
  - How the Windows TCP/IP stack routes packets according to interface metrics.
  - How explicit socket binding to an adapter's local IP address (`socket2::Socket::bind` / `tokio::net::TcpSocket`) forces the OS to bypass the default gateway single-adapter metric.
  - How HTTP Range requests (`Range: bytes=X-Y`) split files into non-overlapping byte chunks.
  - How sparse file pre-allocation (`SetFileInformationByHandle` / `tokio::fs::File::set_len`) avoids disk fragmentation and eliminates post-download file merging overhead.

### Rule 2: Minimal Working Baseline First
- Never build complex, multi-threaded GUI systems before proving the simplest end-to-end path.
- Workflow:
  1. Build a simple single-socket byte-range downloader and verify the downloaded file's SHA-256 hash.
  2. Scale to multiple concurrent chunks on a single interface.
  3. Introduce socket binding across multiple local IP adapters.
  4. Integrate the work-stealing queue and dynamic EMA speed balancing.
  5. Connect the Tauri v2 desktop UI.

### Rule 3: Inspect the Data (Never Guess)
- Never assume an adapter is working or packets are flowing without verifying.
- Log and verify:
  - Exact local bound IP and remote socket addresses.
  - Requested byte offsets `[start, end]` vs received byte count.
  - HTTP response codes (206 Partial Content vs 200 OK vs 416 Range Not Satisfiable).
  - Real-time rolling-window throughput per adapter.

### Rule 4: Make It Work, Make It Right, Make It Fast
- **Make It Work**: Get the end-to-end functionality running and byte-level correctness verified.
- **Make It Right**: Handle edge cases (adapter disconnect, timeout, 404 mirrors, unseekable files, server doesn't support ranges).
- **Make It Fast**: Profile memory, zero-copy buffers, lock-free work-stealing channels.

### Rule 5: Simplicity Over Cleverness
- Write clean, idiomatic, explicit Rust.
- Avoid unnecessary macros, over-engineered trait hierarchies, or premature abstractions until a concrete need arises.
- Clear mental model > clever syntax.

---

## 2. Superpowers Engineering Standards

### Superpower 1: Test-Driven Invariants & Integrity
- Every module in `conflux-core` must have unit tests asserting its mathematical invariants:
  - Chunk boundaries must cover `[0, total_bytes - 1]` with zero gaps and zero overlaps.
  - The sparse file writer must write bytes at exact offsets without corruption.
  - Chunk re-queueing must maintain state consistency when workers fail.

### Superpower 2: Scientific Hypothesis-Driven Debugging
- When a bug occurs:
  1. Formulate a clear, falsifiable hypothesis.
  2. Add targeted logging or write a minimal reproduction test.
  3. Validate or invalidate the hypothesis using evidence.
  4. Fix the root cause, not the symptoms.

### Superpower 3: Fault Injection & Resilience Testing
- Downloads must be resilient to real-world network chaos:
  - Simulated adapter disconnect (e.g. Wi-Fi drops or phone unplugged).
  - Slow or stalled mirror servers.
  - Incomplete chunk recovery without corrupting the overall download.

### Superpower 4: Transparent User Alignment
- **Never make unilateral design or architectural decisions.**
- When a fork in the road is encountered (UI frameworks, dependency choices, API contracts, defaults), present the options clearly to the user with pros/cons and a recommendation, and ask for their input.

### Superpower 5: Quality Gates
Before any commit:
- `cargo fmt --check` (clean formatting)
- `cargo clippy -- -D warnings` (zero compiler warnings)
- `cargo test` (all unit & integration tests pass)

### Superpower 6: Strict Version Incrementation
- On every iteration/release, increment the version across:
  - `Cargo.toml` (`[workspace.package.version]`)
  - `crates/conflux-desktop/tauri.conf.json` (`version`)
  - `ui/package.json` (`version`)
- Ensure the installer bundle reflects the newly incremented version (e.g. `Conflux_<version>_x64-setup.exe`).
