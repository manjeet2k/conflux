# Dependency security patches

Sources are unmodified crates.io releases except for the changes below. Their original licenses are included.

- `tauri-plugin-updater` 2.13.1: Windows ZIP dependency upgraded from 4 to >=9.0.5, preserving default-features=false. Fixes GHSA-8hxf-qr8p-62x4, GHSA-q7v9-x93c-xqv3 and GHSA-m665-v7pm-7grj. Remove the patch when upstream adopts a patched ZIP release.
- `glib` 0.18.5: backport the mutable out-pointer fix in VariantStrIter::impl_get (GHSA-wrw7-89jp-8q8g; https://github.com/gtk-rs/gtk-rs-core/pull/1343). The local pointer is mutable and passed as &mut p to the variadic C API. This is a Linux-only transitive dependency, absent from the shipped Windows app. Remove when Tauri migrates away from the affected GTK dependency chain.

Local GLib retains its upstream version for compatibility; version-only scanners may still report it. No advisory is suppressed.

## Verification (2026-11-06)

- Windows desktop Clippy compiles the updater against ZIP 9.4.0.
- Core Clippy, 109 unit tests and 74 integration tests pass.
- GLib's patched iterator passes an optimized regression that reads a three-string variant array using both next() and next_back(), checks order and exhaustion.
- Frontend audit reports zero vulnerabilities; lint, production build and all 57 tests pass.
- Workspace formatting, version synchronization (0.5.2), and whitespace checks pass.

The patches preserve existing updater archive support and do not add Linux product support.

The full RustSec scan also required lockfile upgrades: rustls-webpki 0.103.16, webpki-root-certs and webpki-roots 1.0.11 (RUSTSEC-2026-0381/0382/0383), and Tokio 1.54.1 (RUSTSEC-2026-0386). Final cargo audit reports zero vulnerabilities. Existing unmaintained-package warnings remain for proc-macro-error and the zbus serialization dependencies.
