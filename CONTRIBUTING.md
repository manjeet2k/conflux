# Contributing to Conflux

Thanks for helping. Conflux is a Windows-only (10/11 x64) Rust + Tauri v2 download manager in
public beta. Please read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) first.

## Where to start

- [`AGENTS.md`](AGENTS.md): the binding engineering rules (test the invariants, verify with data,
  discuss design forks before building, no unrequested release builds).
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md): setup, the quality gates, versioning, CI cost.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): how the engine and app work.
- [`docs/ROADMAP.md`](docs/ROADMAP.md): open tasks. Pick one, or open an issue to discuss first.
- [`docs/KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md) and [`docs/WINDOWS_TEST_PLAN.md`](docs/WINDOWS_TEST_PLAN.md):
  good places to help if you have a Windows machine with more than one network adapter.

Platform policy: Windows only. Do not add Linux or macOS features or packaging; the Linux code
paths exist only so the core tests run on the WSL2 dev host. CI runs on Windows only.

## Workflow

1. Open or claim an issue for anything bigger than a small fix. For design choices (UI framework,
   dependencies, defaults, API contracts) propose options with pros/cons first.
2. Branch from `main`: `feat/<short-name>`, `fix/<short-name>`, `docs/<short-name>`,
   `chore/<short-name>` (roadmap work uses `roadmap/<task-id>`).
3. Commit messages follow Conventional Commits: `feat: ...`, `fix: ...`, `docs: ...`,
   `test: ...`, `chore: ...`, imperative and under about 72 characters in the subject.
4. Add tests that assert the invariants you touch; add a line under *Unreleased* in
   [`CHANGELOG.md`](CHANGELOG.md) for user-visible changes; update `docs/ARCHITECTURE.md` when
   behaviour changes.
5. Run the quality gates (full list in `docs/DEVELOPMENT.md`) and open a pull request using the template.

```bash
cargo fmt --check
cargo clippy -p conflux-core --tests -- -D warnings
cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu --tests -- -D warnings
cargo test -p conflux-core
npm --prefix ui run lint && npm --prefix ui run build
node scripts/check-version.mjs && node scripts/check-docs.mjs
```
Never run a bare `cargo test` on Linux/WSL2; always pass `-p conflux-core`.

## CI cost note

CI runs on Windows only and skips docs-only changes. Standard GitHub runners are free for public
repositories, but they are still shared and slow, so please do not push many small commits to a
PR branch: batch your work and run the gates locally first (`scripts/test-windows.sh` runs the
tests as real Windows executables from WSL2). A maintainer must approve the first CI run of a
PR from a first-time contributor. Add
`[skip ci]` to a commit message for changes that need no CI. Do not build release installers
unless a maintainer asks; releases come from the release workflow.

## Reporting bugs and security issues

Bugs: use the issue templates and paste **Copy diagnostics** output. Security problems: follow
[SECURITY.md](SECURITY.md), do not file a public issue.

## Licence

By contributing you agree that your work is licensed under MIT OR Apache-2.0, as the project is.
