#!/usr/bin/env node
// Usage: node scripts/bump-version.mjs X.Y.Z[-pre]
// Rewrites Cargo.toml, tauri.conf.json, ui/package.json, Cargo.lock (workspace crates only,
// no network) and ui/package-lock.json. Refuses non-semver or non-increasing versions.
import { readFileSync, writeFileSync } from "node:fs";
import { P, LOCK_CRATES, cargoVersion, compareSemver, parseSemver } from "./lib.mjs";

const next = process.argv[2];
if (!next || !parseSemver(next)) {
  console.error("usage: bump-version.mjs X.Y.Z[-pre]  (valid semver required)");
  process.exit(2);
}
const current = cargoVersion();
const cmp = compareSemver(parseSemver(next), parseSemver(current));
if (cmp <= 0) {
  console.error(`refusing: ${next} is ${cmp === 0 ? "equal to" : "lower than"} current ${current}`);
  process.exit(1);
}

const read = (p) => readFileSync(p, "utf8");
const edits = new Map(); // compute everything first, write only if all succeed

// Cargo.toml: only the version line inside [workspace.package].
{
  const t = read(P.cargo);
  const re = /(^\[workspace\.package\]\s*$[\s\S]*?^version\s*=\s*")[^"]+(")/m;
  if (!re.test(t)) throw new Error("Cargo.toml: version line not found");
  edits.set(P.cargo, t.replace(re, `$1${next}$2`));
}
// JSON files: replace the first top-level "version" textually to preserve formatting.
for (const p of [P.tauri, P.pkg]) {
  const t = read(p);
  const re = /^(\s*"version"\s*:\s*")[^"]+(")/m;
  if (!re.test(t)) throw new Error(`${p}: version not found`);
  edits.set(p, t.replace(re, `$1${next}$2`));
}
// Cargo.lock: workspace crates.
{
  let t = read(P.cargoLock);
  for (const n of LOCK_CRATES) {
    const re = new RegExp(`(^name = "${n}"\\nversion = ")[^"]+(")`, "m");
    if (!re.test(t)) throw new Error(`Cargo.lock: ${n} not found (run cargo check first)`);
    t = t.replace(re, `$1${next}$2`);
  }
  edits.set(P.cargoLock, t);
}
// package-lock.json: root "version" and packages[""].version.
{
  const t = read(P.pkgLock);
  const j = JSON.parse(t);
  j.version = next;
  if (j.packages?.[""]) j.packages[""].version = next;
  edits.set(P.pkgLock, JSON.stringify(j, null, 2) + (t.endsWith("\n") ? "\n" : ""));
}

for (const [p, t] of edits) writeFileSync(p, t);
console.log(`Bumped ${current} -> ${next} in ${edits.size} files.`);
