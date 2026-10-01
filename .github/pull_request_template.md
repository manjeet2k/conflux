## What and why

<!-- Link the issue or roadmap task (for example "ROADMAP D-2"). -->

## How it was verified

<!-- Tests added, data inspected, anything you could NOT verify (especially Windows-only behaviour). -->

## Quality gates (from docs/DEVELOPMENT.md)

- [ ] `cargo fmt --check`
- [ ] `cargo clippy -p conflux-core --tests -- -D warnings`
- [ ] `cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu --tests -- -D warnings`
- [ ] `cargo test -p conflux-core` (run three times if engine/watcher code changed)
- [ ] `npm --prefix ui run lint && npm --prefix ui run build`
- [ ] `node scripts/check-version.mjs` and `node scripts/check-docs.mjs`

## Checklist

- [ ] Tests assert the invariants I touched
- [ ] `CHANGELOG.md` has a line under Unreleased (user-visible changes)
- [ ] `docs/ARCHITECTURE.md` updated if behaviour changed
- [ ] No release/installer build, no telemetry, no Linux/macOS features added
- [ ] Design decisions were agreed in an issue first
