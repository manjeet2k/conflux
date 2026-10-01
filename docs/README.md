# Conflux documentation

| Doc | Read it when you want to… |
|-----|---------------------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | understand how channel bonding, chunk scheduling, resume, the watcher and the desktop app work (kept in step with the code) |
| [DEVELOPMENT.md](DEVELOPMENT.md) | set up, run the quality gates, test, version, and understand CI cost |
| [ROADMAP.md](ROADMAP.md) | see what is left before the public beta and pick up a task (run `node scripts/roadmap-status.mjs` for a status summary) |
| [guides/windows-cross-compilation.md](guides/windows-cross-compilation.md) | build a Windows binary from Linux/WSL2 |
| [archive/PLAN.md](archive/PLAN.md) | read the original design plan (historical; no longer tracks the code) |

Elsewhere in the repo: [`../README.md`](../README.md) (overview and quickstart),
[`../AGENTS.md`](../AGENTS.md) (binding engineering rules for contributors and agents),
[`../CHANGELOG.md`](../CHANGELOG.md) (what changed per release).

## Keeping docs honest
- Behaviour change → update `ARCHITECTURE.md` in the same change, and add a `CHANGELOG.md` line.
- Finished a roadmap task → set its status and fill its *Done notes* (say what was *not* verified).
- New doc → add it to the table above. Superseded doc → move it to `archive/` with a banner.
- Check relative links with `node scripts/check-docs.mjs`.
