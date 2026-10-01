# Releasing Conflux

Step-by-step runbook for cutting a **beta** release. You need no prior release experience; every
command is copy-pasteable. CI (not your laptop) builds the installers
([`.github/workflows/release.yml`](../.github/workflows/release.yml)).

Beta policy (decided by the maintainer): Windows 10/11 x64 only, **unsigned** installer (no code
signing yet), in-app auto-update, no telemetry. Versions look like `0.2.0-beta.1`, tags like
`v0.2.0-beta.1`, and every release is a GitHub **prerelease**.

## How it fits together

```
tag v0.2.0-beta.1 pushed
  -> release.yml "build" job (windows-latest)
       builds Conflux_0.2.0-beta.1_x64-setup.exe (+ .sig, signed with the UPDATER key)
       builds Conflux_0.2.0-beta.1_x64-offline-setup.exe (WebView2 embedded, ~130 MB)
       writes SHA256SUMS.txt, latest.json, (optional) SBOM
       creates a DRAFT prerelease with all of it             <- you review this
  -> you publish the draft
  -> release.yml "publish-updater" job copies latest.json onto the fixed release "updater-beta"
       installed apps poll
       https://github.com/manjeet2k/conflux/releases/download/updater-beta/latest.json
```

Why a fixed `updater-beta` release: GitHub's `releases/latest/download/...` URL ignores
prereleases (and every beta is one) and does not work for private repos. A release with a fixed
tag has a stable URL, and its `latest.json` is simply overwritten on each publish.

There are two **unrelated** kinds of signing:

| Key | Purpose | Status |
|-----|---------|--------|
| Updater key (minisign, made by `tauri signer generate`) | Lets installed apps verify an update is really from you. **Required** even while the installer is unsigned. | Set up once, see [Updater key](#updater-key-setup-backup-rotation). |
| Authenticode code-signing certificate | Removes the SmartScreen "unknown publisher" warning. | **Not used in the beta** (roadmap S-1/S-2). |

## One-time setup (before the first beta)

1. **Make the repository public.** The app downloads updates from `github.com/.../releases`
   anonymously. While the repo is private, `latest.json` and the installers return 404 to everyone
   but you, so **no installed copy can update**. Settings -> General -> Danger Zone -> Change
   visibility. Do this *before* announcing or publishing the first beta.
2. Create the updater key and set the GitHub secrets: [Updater key](#updater-key-setup-backup-rotation).
3. Optional: add the `VIRUSTOTAL_API_KEY` secret ([Antivirus](#antivirus-and-smartscreen)).
4. **Protect the signing key** ([Hardening](#hardening-the-release-pipeline-manual-github-settings)):
   already applied on 2026-10-01 (environment `release` limited to `main` and `v*.*.*`, signing
   secrets live only there, tag ruleset active). **Add required reviewers when the repo goes
   public** (not available on private repos on this plan).
5. Check Actions are enabled for the repo and the workflow permission is "Read and write"
   *or* leave the default; `release.yml` asks for `contents: write` itself.

## Release checklist

Work on `main` with a clean tree.

### 1. Make sure `main` is releasable
```bash
cargo fmt --check
cargo clippy -p conflux-core --tests -- -D warnings
PATH=<dir>:$PATH cargo clippy -p conflux-desktop --target x86_64-pc-windows-gnu --tests -- -D warnings
cargo test -p conflux-core
npm --prefix ui run lint && npm --prefix ui run build
node scripts/check-version.mjs
```
(`<dir>` = prepend the directory that contains `x86_64-w64-mingw32-windres`, if it is not already on `PATH`.)
CI on `main` should be green, **including the Security workflow** (cargo-deny, RustSec audit, npm
audit): the release workflow does not re-run it, so check Actions -> Security on the commit you tag. Run through [`WINDOWS_TEST_PLAN.md`](WINDOWS_TEST_PLAN.md) on a real
Windows machine for anything risky.

### 2. Update the changelog
In [`CHANGELOG.md`](../CHANGELOG.md), rename `## [Unreleased]` to the new version and date, and
add a fresh empty `## [Unreleased]` above it:
```markdown
## [Unreleased]

## [0.2.0-beta.1] - 2026-10-15
### Added
- ...
```
The release notes (and the "what's new" text the in-app updater shows) are exactly this section.
Preview them:
```bash
node scripts/extract-changelog.mjs 0.2.0-beta.1
```
If the version has no section (or it is empty) the script **fails**, and so does the release
workflow: it never silently ships the `Unreleased` text. To preview unreleased notes before you
rename the heading, add `--allow-unreleased`.

### 3. Bump the version
```bash
node scripts/bump-version.mjs 0.2.0-beta.1
node scripts/check-version.mjs
node scripts/gen-licenses.mjs        # refresh THIRD_PARTY_LICENSES.md if dependencies changed
git add -A && git commit -m "release: 0.2.0-beta.1"
```
`bump-version` updates `Cargo.toml`, `tauri.conf.json`, `ui/package.json` and the lock files
together and refuses to go backwards. Versions must be valid semver; `0.2.0-beta.2` is newer than
`0.2.0-beta.1`, and `0.2.0` is newer than any `0.2.0-beta.N`.

### 4. Tag and push
```bash
git tag -a v0.2.0-beta.1 -m "Conflux 0.2.0-beta.1"
git push origin main v0.2.0-beta.1
```
The workflow aborts immediately if the tag does not equal the version in the files, if the tagged
commit is not an ancestor of `origin/main`, or if the changelog has no section for the version.
Installers are built with `cargo ... --locked` (the committed `Cargo.lock` must be current).

**Rebuild rule.** A published release is immutable. The build job refuses to run for a tag whose
release is already published (it may only run when no release exists or the release is still a
*Draft*). To rebuild: **delete the draft** (`gh release delete vX --yes`, keep the tag, or move it
if the commit changed) and re-run `gh workflow run release.yml -f tag=v0.2.0-beta.1` (Actions tab
-> Release -> Run workflow). Never touch a published release; to fix one, cut a **new beta number**.

### 5. Wait for CI and inspect the draft
Watch Actions -> Release (about 15-25 minutes). Then open **Releases**; there is a *Draft* with:

| Asset | What |
|-------|------|
| `Conflux_<v>_x64-setup.exe` | standard installer (downloads WebView2 if missing) |
| `Conflux_<v>_x64-setup.exe.sig` | updater signature |
| `Conflux_<v>_x64-offline-setup.exe` | offline installer with WebView2 included, for offline/locked-down PCs |
| `SHA256SUMS.txt` | checksums of both installers |
| `latest.json` | updater manifest (only the standard installer is listed) |
| `Conflux_<v>_sbom.cdx.json` | SBOM, when the optional step succeeded |

The job summary lists the checksums and, if configured, the VirusTotal report link.

### 6. Verify the draft
On a Windows machine, download the installer from the **draft** (while signed in, Releases page):
```powershell
Get-FileHash .\Conflux_0.2.0-beta.1_x64-setup.exe -Algorithm SHA256
```
The hash must equal the line in `SHA256SUMS.txt`. Then install it.

**SmartScreen**: the installer is unsigned, so Windows shows "Windows protected your PC / Unknown
publisher". That is expected in the beta: click **More info -> Run anyway**. Downloads made by a
browser carry a Mark-of-the-Web, which is what triggers it. Reputation builds with downloads;
signing is the real fix (roadmap S-1/S-2). The README must tell users this and show the SHA-256.

Checks: installs per-user without admin, the Start-menu shortcut works, the installer shows
publisher "Manjeet Singh" and the license, Settings -> About & Updates shows the version,
"Third-party licenses" opens, one bonded download completes.

### 7. Smoke-test the update path (every release)
This needs the **previous** beta installed and the new release **published**, so use the safest
order: test with a throwaway pre-release tag first if you changed updater code.

1. Install the previous beta (for example `0.2.0-beta.1`).
2. Start a large download (several GB or a throttled test server) so it is mid-flight.
3. Publish the new draft (step 8). Wait for the `publish-updater` job to finish.
4. In the old app: Settings -> About & Updates -> **Check for updates**. It should show the new
   version and the release notes.
5. **Install and restart**. The download pauses, the installer runs passively, the app restarts on
   the new version and the same download resumes by itself (`resume_after_update.json` is read and
   deleted at startup; see `%APPDATA%\com.conflux.desktop\`).
6. Check the log (`Settings -> Open logs folder`) for "Resuming downloads paused for the update".

If this fails, **yank the release** (below) before more users take it.

### 8. Publish the draft
Releases -> the draft -> Edit -> confirm **"Set as a pre-release" is ticked** and *not* "latest"
-> **Publish release**. Publishing triggers `publish-updater`, which first **verifies** the manifest
(`scripts/verify-updater-manifest.mjs`): the signature checks out against the public key in
`tauri.conf.json`, the version equals the tag and is strictly newer than the one currently in
`updater-beta`, and every `url` points into this release. Only then does it create the
`updater-beta` release if needed and overwrite its `latest.json`. If the job fails, nothing went
live; fix forward with a new beta number. (A published older version can never replace a newer
manifest; use the rollback steps below by hand for that.) From this moment installed copies are offered
the update (startup check is on by default; users can also click Check for updates).

Confirm: open
`https://github.com/manjeet2k/conflux/releases/download/updater-beta/latest.json` in a private
browser window and check that `version` is the new one.

### 9. Announce
Post the release link, point to the SmartScreen note and the checksums, and ask beta testers to
attach "Copy diagnostics" output to bug reports.

## Hardening the release pipeline (manual GitHub settings)

The workflow file cannot configure these; they are repository settings. Current state (applied
with `gh` on 2026-10-01):

1. **Environment `release`** exists, restricted to the branch `main` and the tag pattern
   `v*.*.*`. `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are stored as
   **environment secrets only** (the repository-level copies were deleted), so workflows from
   other branches, other tags, and jobs without `environment: release` (including
   `publish-updater`, which needs no key) cannot read the key. The build job declares
   `environment: release`.
   - **Not applied: required reviewers.** GitHub rejected them ("billing plan does not support
     the required reviewers protection rule"): they are unavailable on private repos on the
     current plan. Today a push of a `v*.*.*` tag by a maintainer therefore starts the build
     without an approval step (it still only produces a *draft* prerelease). When the repo
     becomes public, add them: Settings -> Environments -> release -> Required reviewers.
2. **Tag ruleset `protect-release-tags`** (active): only repository admins (bypass) can create,
   update or delete tags matching `v*.*.*`. Anyone else with write access cannot start a release
   build by pushing a tag.
3. Optionally require a pull request and status checks on `main`; the workflow already refuses
   tags whose commit is not on `main`.

Built into the workflow: third-party actions are pinned to full commit SHAs (update them
deliberately, resolving a tag with `gh api repos/<owner>/<repo>/git/ref/tags/<tag>` and, for
annotated tags, `.../git/tags/<sha>`), and `VIRUSTOTAL_API_KEY` is visible only to the VirusTotal
steps. Security checks (cargo-deny, RustSec, npm audit) are not repeated in the release job; keep
the Security workflow green before tagging.

## Rollback and yanking a bad release

*Goal: stop new update offers quickly, then ship a fix.* Users already on the bad version keep
running it; the updater only ever moves forward.

1. **Withdraw the manifest.** Put the previous good version back:
   download the *previous* release's `latest.json` and overwrite the manifest:
   ```bash
   gh release download v0.2.0-beta.1 --pattern latest.json --dir . --clobber
   gh release upload updater-beta latest.json --clobber
   ```
   An app already on the bad version will not "downgrade" to it (the updater needs a strictly newer
   version), but *no one else* is offered the bad one. If there is no previous release, delete
   the asset instead: `gh release delete-asset updater-beta latest.json --yes` (checks then fail
   quietly; apps keep running).
2. **Hide the bad release.** Edit it back to *Draft*, or delete it (`gh release delete v0.2.0-beta.2`).
   Keep the tag unless the commit itself is wrong; never reuse a version number.
3. **Ship the fix as a new, higher version** (`0.2.0-beta.3`), through the normal checklist. Users
   on the bad version are then offered the fix.
4. Say so in the changelog and the announcement.

## Updater key: setup, backup, rotation

### Setup (once)
```bash
node scripts/setup-updater-key.mjs
```
This runs `tauri signer generate` with a random password, saves the keys **outside the repo** in
`~/.config/conflux-secrets/` (`updater.key`, `updater.key.password`, `updater.key.pub`; directory
mode 700, files 600) and writes only the **public** key into
`crates/conflux-desktop/tauri.conf.json` (`plugins.updater.pubkey`). Commit that change.
`--out DIR --no-config` makes a test key without touching the repo; `--force` replaces a key.

Known exposure: the Tauri CLI accepts the key password only as the `--password` argument (no
environment variable or stdin option for `signer generate`), so the random password is visible in
the process list for about a second. Run this only on a single-user machine you trust.

Then set the GitHub Actions secrets (Settings -> Secrets and variables -> Actions, or `gh`):
```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.config/conflux-secrets/updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < ~/.config/conflux-secrets/updater.key.password
gh secret set VIRUSTOTAL_API_KEY      # optional; paste your key when prompted
```

| Secret | Held by | Used for |
|--------|---------|----------|
| `TAURI_SIGNING_PRIVATE_KEY` | maintainer + GitHub Actions | signs the update installer |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | maintainer + GitHub Actions | unlocks the key |
| `VIRUSTOTAL_API_KEY` (optional) | maintainer + GitHub Actions | VirusTotal scan step |
| `GITHUB_TOKEN` | provided by GitHub | creating releases (no setup) |

### Backup (do this now)
Copy `updater.key` **and** `updater.key.password` to an encrypted, offline place (password
manager attachment plus a USB stick in a drawer). **If the private key is lost, no installed copy
can ever verify an update again**: every user would have to reinstall by hand. Never email it,
paste it in chat, or commit it (`.gitignore` blocks `*.key`, `updater*.key*`, but check
`git status` anyway).

### Rotation (suspected leak, or a lost password)
Installed apps trust only the public key baked into them, so rotation needs a **bridge release**:

1. Generate a new key to a different folder: `node scripts/setup-updater-key.mjs --out ~/.config/conflux-secrets-new --no-config`.
2. Build the bridge release signed with the **old** key but with the **new** public key in
   `tauri.conf.json`. Old apps accept it (old signature) and from then on trust the new key.
3. Replace the GitHub secrets with the new key and password; later releases are signed with the new
   key.
4. If the old key *leaked* (not just lost), also treat every update published until the bridge as
   suspect and move quickly; if it was *lost*, a bridge is impossible, and users must reinstall.
5. Retire the old key files after the bridge release has been out for a while.

## Antivirus and SmartScreen

Unsigned, little-known installers are often flagged heuristically. Mitigations:

- **VirusTotal.** With the `VIRUSTOTAL_API_KEY` secret set, CI uploads the standard installer and
  puts the report link in the job summary. Without the secret the step is skipped. Open the link
  before publishing; a handful of low-reputation "generic" detections on an unsigned installer is
  common, a named malware family is not (stop and investigate).
- **Microsoft Defender false positive.** Submit the exact file at
  <https://www.microsoft.com/en-us/wdsi/filesubmission> ("Software developer" -> "I believe this
  file is incorrectly detected as malware"), sign in with a Microsoft account, upload the installer
  and its SHA-256, and describe it ("Open-source download manager, source at github.com/manjeet2k/conflux").
  Results arrive by email, usually within days; re-submit for each new build if it is flagged
  again. Other vendors have similar forms (listed in the VirusTotal report).
- **SmartScreen** reputation is per file hash/publisher and improves with downloads; only code
  signing removes the warning for good.
- Always publish `SHA256SUMS.txt` so users can verify what they downloaded.

## Troubleshooting

| Symptom | Likely cause |
|---------|--------------|
| Workflow stops at "Check tag matches" | Tag and version files differ. Delete the tag (`git push --delete origin vX`, `git tag -d vX`), fix with `bump-version`, re-tag. |
| "Third-party licenses are up to date" fails | Dependencies changed: run `node scripts/gen-licenses.mjs` and commit. |
| Build fails asking for a signing key | The `TAURI_SIGNING_PRIVATE_KEY`/`_PASSWORD` secrets are missing or wrong. |
| App says "Could not check for updates" | Repo still private, `updater-beta` release missing, or no network. Open the endpoint URL in a private window. |
| Update downloads but fails to install | Signature mismatch: the public key in `tauri.conf.json` does not match the signing secret. |
| Offline installer is much larger | By design (embeds the WebView2 runtime). |
