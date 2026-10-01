# Conflux documentation

| Doc | Read it when you want to… |
|-----|---------------------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | understand how channel bonding, chunk scheduling, resume, the watcher and the desktop app work (kept in step with the code) |
| [DEVELOPMENT.md](DEVELOPMENT.md) | set up, run the quality gates, test, version, and understand CI cost |
| [RELEASING.md](RELEASING.md) | cut a beta release: changelog, version, tag, CI draft, update smoke test, rollback, updater key |
| [ROADMAP.md](ROADMAP.md) | see what is left before the public beta and pick up a task (run `node scripts/roadmap-status.mjs` for a status summary) |
| [WINDOWS_TEST_PLAN.md](WINDOWS_TEST_PLAN.md) | run the PASS/FAIL checklists (desktop tests, smoke, install/upgrade, soak/chaos) on real Windows |
| [BENCHMARKS.md](BENCHMARKS.md) | see or reproduce the bonding benchmark method and results (kit in `../scripts/bench/`) |
| [KNOWN_ISSUES.md](KNOWN_ISSUES.md) | see the honest list of current limitations |
| [guides/windows-cross-compilation.md](guides/windows-cross-compilation.md) | build a Windows binary from Linux/WSL2 |
| [archive/PLAN.md](archive/PLAN.md) | read the original design plan (historical; no longer tracks the code) |

Elsewhere in the repo: [`../README.md`](../README.md) (overview and quickstart),
[`../AGENTS.md`](../AGENTS.md) (binding engineering rules for contributors and agents),
[`../CHANGELOG.md`](../CHANGELOG.md) (what changed per release).

## Project policies

- [SECURITY.md](../SECURITY.md): how to report a vulnerability (90-day coordinated disclosure)
- [PRIVACY.md](../PRIVACY.md): what the app contacts and stores (no telemetry)
- [CONTRIBUTING.md](../CONTRIBUTING.md): branches, commits, quality gates, CI cost
- [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md): Contributor Covenant 2.1
- Issue templates and the pull request template live in [`../.github/`](../.github/)

## Keeping docs honest
- Behaviour change → update `ARCHITECTURE.md` in the same change, and add a `CHANGELOG.md` line.
- Finished a roadmap task → set its status and fill its *Done notes* (say what was *not* verified).
- New doc → add it to the table above. Superseded doc → move it to `archive/` with a banner.
- Check relative links with `node scripts/check-docs.mjs`.
