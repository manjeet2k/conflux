# Development guide

How to build, check and test Conflux. The binding engineering rules (design decisions go to the
user, tests assert invariants, no unrequested release builds) are in [`AGENTS.md`](../AGENTS.md);
this page is the practical "how".

## Platform policy

**Windows 10/11 (64-bit) is the only supported and shipped platform.** The Linux-specific code in
`conflux-core` (netlink watcher, `getifaddrs` discovery, `SO_BINDTODEVICE`, unix positional
writes) is kept **only** so the core test suite runs on the WSL2 dev host and in Linux CI, which
is faster and cheaper than Windows runners. Therefore:

- Don't add Linux or macOS features, bundles or CI release jobs, and don't claim support in docs.
- Don't remove the Linux paths without a replacement plan for running the core tests (the
  Windows CI job costs double minutes and the local dev loop would lose its test run).
- Behaviour that matters to users must be correct and verified **on Windows**; passing on Linux
  is necessary but not sufficient (see roadmap task V-1).

## Environment

Conflux targets Windows, but day-to-day development happens on **Linux / WSL2**:

| What | Where it runs |
|------|---------------|
| `conflux-core` (engine) unit + integration tests | Linux/WSL2 (and Windows CI) |
| `conflux-cli` tests | Linux/WSL2 |
| UI lint, build, and browser dev with the mock backend | Anywhere with Node |
| `conflux-desktop` (Tauri) **tests** | Windows only — needs GTK/WebKit on Linux |
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

## Quality gates

Run from the repo root before calling a change done. `AGENTS.md` lists the minimum; this is the
full set CI runs ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)).

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
`cargo test -p conflux-core` three times in a row.

Desktop logic that is pure (parsing, redaction, migration) can't be tested on Linux in place. To
check it anyway, `#[path]`-include the module in a scratch crate outside the repo and run its
tests there, and say so in your report. The real tests run on the CI Windows job.

## Running things

```bash
cargo run --bin conflux -- adapters          # list adapters
cargo run --bin conflux -- probe <url>       # size + Range support
cargo run --bin conflux -- download <url> -o out.bin
RUST_LOG=conflux_core=debug cargo run --bin conflux -- download <url>   # engine debug log

npm --prefix ui run dev                      # UI in a browser, using ui/src/dev/mockBackend.ts
```

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

## CI cost

Runner minutes cost money on private repos (Windows counts double). CI skips docs-only changes
and can be started by hand (`gh workflow run ci.yml`); the Security workflow runs weekly or when
dependency files change; there are no Dependabot update PRs — a monthly "Dependency report" issue
lists outdated dependencies instead. Add `[skip ci]` to a commit message to skip all workflows.
