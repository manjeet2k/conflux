# Development guide

How to build, check and test Conflux. The binding engineering rules (design decisions go to the
user, tests assert invariants, no unrequested release builds) are in [`AGENTS.md`](../AGENTS.md);
this page is the practical "how".

## Platform policy

**Windows 10/11 (64-bit) is the only supported and shipped platform.** The Linux-specific code in
`conflux-core` (netlink watcher, `getifaddrs` discovery, `SO_BINDTODEVICE`, unix positional
writes) is kept **only** so the core test suite also runs natively in the WSL2 dev loop (fast, no
Windows round-trip). CI runs on Windows only. Therefore:

- Don't add Linux or macOS features, bundles or CI release jobs, and don't claim support in docs.
- The Linux paths can be removed later if the fast WSL2 loop is no longer wanted: the Windows
  loop (`scripts/test-windows.sh`) already runs the same tests as real Windows executables.
- Behaviour that matters to users must be correct and verified **on Windows**; passing on Linux
  is necessary but not sufficient (see roadmap task V-1).

## Repository layout

```
conflux/
├── AGENTS.md                          # Binding engineering rules for contributors and agents
├── CHANGELOG.md                       # What changed per release
├── Cargo.toml                         # Cargo workspace (conflux-core, conflux-cli, conflux-desktop)
├── crates/
│   ├── conflux-core/                  # Pure Rust download engine (no UI, testable on Linux)
│   │   ├── src/
│   │   │   ├── adapter.rs             # Adapter discovery, link-state filtering, bound HTTP clients
│   │   │   ├── watcher.rs             # OS network-change watcher (hot-join / hot-remove adapters)
│   │   │   ├── chunk.rs               # Non-overlapping byte-range chunk scheduler
│   │   │   ├── engine.rs              # Multi-adapter coordinator: probe, workers, retries, If-Range
│   │   │   ├── writer.rs              # Sparse file writer (exact-offset positional writes)
│   │   │   ├── resume.rs              # Resume sidecar (completed-chunk bitmap + validators)
│   │   │   ├── filename.rs            # Filename sanitising and atomic unique-name claiming
│   │   │   └── checksum.rs            # Streaming SHA-256
│   │   └── tests/                     # Integration tests against a fault-injecting test server
│   ├── conflux-cli/                   # Terminal client (adapters, probe, download)
│   └── conflux-desktop/               # Tauri v2 Windows app
│       ├── src/                       # commands, task state, settings, history, tray, logging,
│       │                              #   diagnostics, adapter overrides
│       ├── capabilities/              # Webview permissions (kept minimal)
│       └── tauri.conf.json            # Window, CSP and NSIS bundle config
├── ui/                                # React + Fluent UI frontend
│   └── src/
│       ├── App.tsx                    # App shell, event wiring, dialogs
│       ├── components/                # Title bar, download table, details pane, dialogs, ...
│       ├── pages/                     # Network and Settings pages
│       ├── hooks/                     # Downloads, settings, speed history, window theme
│       └── dev/mockBackend.ts         # Mock Tauri backend for browser-only development
├── SECURITY.md  PRIVACY.md            # Vulnerability reporting; what the app does with your data
├── CONTRIBUTING.md  CODE_OF_CONDUCT.md
├── scripts/                           # Version bump/check, docs link check, roadmap status
│   └── bench/                         # Benchmark kit: range-capable test server + adapter counters
├── .github/workflows/                 # CI, security scans, monthly dependency report
└── docs/                              # See docs/README.md for the index
    ├── ARCHITECTURE.md                # How it works (kept in step with the code)
    ├── DEVELOPMENT.md                 # Setup, quality gates, CI cost
    ├── ROADMAP.md                     # Path to the public beta (task list for agents)
    ├── WINDOWS_TEST_PLAN.md           # PASS/FAIL checklists for real Windows machines
    ├── BENCHMARKS.md                  # Bonding benchmark method and results
    ├── KNOWN_ISSUES.md                # Honest list of current limitations
    ├── guides/                        # How-tos (Windows cross-compilation)
    └── archive/                       # Historical docs (original plan)
```

## Environment

Conflux targets Windows, but day-to-day development happens on **Linux / WSL2**:

| What | Where it runs |
|------|---------------|
| `conflux-core` (engine) unit + integration tests | Linux/WSL2, **and as real Windows exes from WSL2** (`scripts/test-windows.sh`), and Windows CI |
| `conflux-cli` tests | Linux/WSL2 and as a Windows exe (`scripts/test-windows.sh`) |
| UI lint, build, and browser dev with the mock backend | Anywhere with Node |
| `conflux-desktop` (Tauri) **tests** | As a Windows exe from WSL2 (`scripts/test-windows.sh desktop`) and Windows CI; native Linux needs GTK/WebKit and is not used |
| `conflux-desktop` **compile / clippy check** | Linux, with the Windows target |

Setup on Linux/WSL2:
```bash
rustup target add x86_64-pc-windows-gnu      # plus the pinned toolchain in rust-toolchain.toml
sudo apt install -y mingw-w64                # linker + windres for the Windows target
npm --prefix ui ci
```
The Windows clippy check needs the mingw `windres` (`x86_64-w64-mingw32-windres`) on `PATH`
because `tauri-winres` runs it from a build script. If it is installed outside the default path,
prepend its directory to `PATH` for that command.

Never run a bare `cargo test` on Linux — always pass `-p <crate>`.

### Running the tests as real Windows executables (WSL2)

WSL2 can launch Windows `.exe` files, so the cross-compiled test binaries run **natively on the
Windows host**. This exercises the Windows-only code (NTFS sparse files, `GetAdaptersAddresses`,
the `NotifyUnicastIpAddressChange` watcher, Mark-of-the-Web) that the Linux run skips:

```bash
PATH=<dir with x86_64-w64-mingw32-windres>:$PATH scripts/test-windows.sh        # core + cli + desktop
scripts/test-windows.sh desktop                                                  # just the desktop crate
```
Details worth knowing (the script handles them): the desktop tests use `--lib` because a plain
`cargo test` also links the crate's `cdylib`, which exceeds MinGW's 65,535-export limit; and test
executables need a Common Controls v6 manifest (the Tauri dialog plugin imports
`TaskDialogIndirect`), which the script links in from `scripts/windows-test/`. This runs the unit
tests only — it does not start the app or the webview, so it does not replace the smoke test in
`docs/WINDOWS_TEST_PLAN.md`. If `cmd.exe /c ver` fails in your shell, interop is off.

## Quality gates

Run from the repo root before calling a change done. `AGENTS.md` lists the minimum; this is the
full set. CI ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)) is Windows-only and runs
the Windows equivalents: format, version check, UI lint and build, clippy and tests for core, cli
and desktop on `windows-latest`.

```bash
cargo fmt --check
cargo clippy -p conflux-core --tests -- -D warnings
cargo clippy -p conflux-core --target x86_64-pc-windows-gnu --tests -- -D warnings
cargo clippy -p conflux-cli --tests -- -D warnings
cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu --tests -- -D warnings   # see windres note above
cargo test -p conflux-core
cargo test -p conflux-cli
npm --prefix ui run lint && npm --prefix ui run build
node scripts/check-version.mjs
```
The engine and watcher integration tests are timing-sensitive: when you touch that code, run
`cargo test -p conflux-core` three times in a row. Before reporting desktop work done, also run
`scripts/test-windows.sh` (it needs WSL2 interop; otherwise rely on the CI Windows job).

## Running things & CLI quickstart

The `conflux-cli` terminal client allows inspecting network interfaces, testing range support, and running bonded downloads directly from the command line:

### 1. Inspect Available Network Adapters
Inspect all physical network interfaces detected on the host:
```bash
cargo run --bin conflux -- adapters
```

### 2. Probe a Remote Endpoint
Inspect remote file size and HTTP Range header support:
```bash
cargo run --bin conflux -- probe http://archive.ubuntu.com/ubuntu/dists/noble/main/binary-amd64/Packages.xz
```

### 3. Run a Multi-Interface Bonded Download
Download a file with dynamic chunking, sparse allocation, and streaming SHA-256 verification:
```bash
cargo run --bin conflux -- download http://archive.ubuntu.com/ubuntu/dists/noble/main/binary-amd64/Packages.xz -o Packages.xz
# With verbose engine debug logging:
RUST_LOG=conflux_core=debug cargo run --bin conflux -- download <url> -o out.bin
```

### 4. Run the Desktop UI in Browser Mode
Run the hot-reloading development server with a mock backend (works anywhere with Node, no Windows/Tauri requirements):
```bash
npm --prefix ui run dev
```
Open `http://localhost:5173` to explore the dashboard, adapter toggles, and live color-coded chunk map.

## Versions and releases

- The version lives in three files that must agree: `Cargo.toml` (`[workspace.package]`),
  `crates/conflux-desktop/tauri.conf.json`, and `ui/package.json`.
  `node scripts/check-version.mjs` verifies them; `node scripts/bump-version.mjs X.Y.Z` changes
  all of them (and the lock files) together.
- **Do not build the release `.exe` / NSIS installer unless asked.** Release builds come from
  CI via [`.github/workflows/release.yml`](../.github/workflows/release.yml) (runbook:
  [RELEASING.md](RELEASING.md)). To build for Windows from
  Linux anyway, see the [Windows cross-compilation guide](guides/windows-cross-compilation.md).
- Add a line under *Unreleased* in [`CHANGELOG.md`](../CHANGELOG.md) for every user-visible change.

## Website (GitHub Pages)

The landing page lives in `site/` (plain HTML, CSS and one small script, no build tooling) and is
deployed to https://manjeet2k.github.io/conflux/ by `.github/workflows/pages.yml` on pushes that
touch it, after every Release run, and by hand (`gh workflow run pages.yml`).

- `node scripts/build-site.mjs` writes `_site/`, filling `{{PLACEHOLDERS}}` with the newest
  published release (betas included) so the download button works without JavaScript; it fails if
  any placeholder is left. `--offline` builds without network (links fall back to the Releases page).
  Preview: `npx --yes serve _site` or `python3 -m http.server -d _site`.
- `node scripts/check-site-links.mjs` checks every link in `_site/` (internal files, `#anchors`,
  og/twitter image URLs, and every external URL with a real request); the Pages workflow runs it
  and refuses to deploy on any broken link. `--no-external` skips the network.
- `site/demo.js` drives the interactive parts: the hero fit (scales the glass hero so it fits the
  first screen, capped by `--hero-max`, with readability floors and a compact fallback), the
  letter/line reveals, the hover tilt on glass surfaces (`data-tilt="max degrees"`), magnetic
  buttons, the speed calculator and the "pull the plug" chunk simulation. Demos use example
  numbers, pause off-screen and in hidden tabs, and respect reduced motion.
- Design: deep indigo with violet/pink corner light, uppercase Fahkwang headings, DM Sans body,
  Outfit labels (self-hosted in `site/fonts/`, SIL Open Font License; licences alongside), square
  two-part buttons, frosted-glass hero and panels, light "calculator" sections.
- `site/app.js` refreshes the links from GitHub's public API in the browser (CORS allowed), so the
  page is right even before the next deploy. The page's CSP only allows that one API origin.
- Social previews: `site/og.png` (1200x630) is rendered from `site/og.svg`. To re-render after
  editing the SVG, in a scratch folder: `npm i @resvg/resvg-js@2`, download `Fahkwang-Bold.ttf` and
  `DMSans[opsz,wght].ttf` from the google/fonts repository (`ofl/…`), and render with
  `new Resvg(svg, { fitTo: { mode: "width", value: 1200 }, font: { loadSystemFonts: false, fontFiles: [...], defaultFontFamily: "DM Sans" } }).render().asPng()`.
  Check previews with a social card validator after deploying.
- A README cannot embed the page (GitHub strips iframes and scripts); the README links to it with
  the banner image and badges instead.

## CI cost

The repository is public, so standard GitHub runners are free; CI is still Windows-only and slow, so it skips docs-only changes
and can be started by hand (`gh workflow run ci.yml`); the Security workflow runs weekly or when
dependency files change; there are no Dependabot update PRs — a monthly "Dependency report" issue
lists outdated dependencies instead. Add `[skip ci]` to a commit message to skip all workflows.
