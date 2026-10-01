#!/usr/bin/env node
// Exit non-zero unless all version sources agree.
// Usage: node scripts/check-version.mjs [--expect X.Y.Z]   (e.g. a release tag without the "v")
import { existsSync } from "node:fs";
import { P, LOCK_CRATES, cargoVersion, jsonVersion, lockVersions, parseSemver } from "./lib.mjs";

const args = process.argv.slice(2);
const expect = args[0] === "--expect" ? args[1] : undefined;

const sources = {
  "Cargo.toml [workspace.package]": cargoVersion(),
  "crates/conflux-desktop/tauri.conf.json": jsonVersion(P.tauri),
  "ui/package.json": jsonVersion(P.pkg),
};
const lock = lockVersions();
for (const n of LOCK_CRATES) sources[`Cargo.lock (${n})`] = lock[n] ?? "<missing>";

const reference = sources["Cargo.toml [workspace.package]"];
let bad = false;
if (!parseSemver(reference)) { console.error(`not valid semver: ${reference}`); bad = true; }
if (expect !== undefined && expect !== reference) {
  console.error(`expected ${expect} but Cargo.toml has ${reference}`); bad = true;
}
for (const [name, v] of Object.entries(sources)) {
  const ok = v === reference;
  if (!ok) bad = true;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}: ${v}`);
}
// package-lock.json is advisory: it was historically left stale; bump-version fixes it.
if (existsSync(P.pkgLock)) {
  const v = jsonVersion(P.pkgLock);
  if (v !== reference) console.warn(`warn ui/package-lock.json: ${v} (run version:bump to sync)`);
}
if (bad) { console.error("Version mismatch."); process.exit(1); }
console.log(`All versions are ${reference}.`);
