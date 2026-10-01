# Security policy

## Supported versions

Conflux is in public beta. Only the **latest beta release** (`0.N.0-beta.K`, newest pre-release on
the [Releases page](https://github.com/manjeet2k/conflux/releases)) receives security fixes.
Please update before reporting, in case the problem is already fixed.

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

- Email **Manjeet Singh <manjeetsgh11@gmail.com>** with the subject "Conflux security".
- Or, once the repository is public, use GitHub's private vulnerability reporting
  (Security tab > "Report a vulnerability") if it is enabled.

Include what you found, the version (Settings > About, or the installer file name), Windows
version, steps to reproduce, and the impact you expect. Do not include real credentials or
other people's data.

## What to expect

- Acknowledgement within 7 days (this is a one-person beta project; best effort).
- A status update when the issue is confirmed or rejected.
- **Coordinated disclosure with a 90-day window**: we aim to ship a fix within 90 days of your
  report and ask that you keep details private until a fix is released or 90 days have passed,
  whichever comes first. If a fix needs longer, we will tell you why and agree on a new date.
- Credit in the release notes if you want it.

## Scope and notes

- The installer is **unsigned** during the beta. That is a known limitation, not a vulnerability
  (see [docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md)). Verify downloads with `SHA256SUMS.txt`.
- In scope: the Conflux application and installer, the update mechanism, handling of untrusted
  servers and URLs (redirects, headers, filenames, path handling), log and diagnostics leakage.
- Out of scope: vulnerabilities in third-party servers you download from, and issues that need an
  already-compromised Windows account.
