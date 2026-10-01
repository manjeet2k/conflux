// Shared helpers for version scripts. Node built-ins only.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
export const P = {
  cargo: join(ROOT, "Cargo.toml"),
  cargoLock: join(ROOT, "Cargo.lock"),
  tauri: join(ROOT, "crates/conflux-desktop/tauri.conf.json"),
  pkg: join(ROOT, "ui/package.json"),
  pkgLock: join(ROOT, "ui/package-lock.json"),
};
export const LOCK_CRATES = ["conflux-core", "conflux-cli", "conflux-desktop"];

const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?$/;

export function parseSemver(v) {
  const m = SEMVER.exec(v);
  if (!m) return null;
  return { major: +m[1], minor: +m[2], patch: +m[3], pre: m[4] ? m[4].split(".") : [] };
}

/** Semver precedence: negative if a < b, 0 if equal, positive if a > b. */
export function compareSemver(a, b) {
  for (const k of ["major", "minor", "patch"]) if (a[k] !== b[k]) return a[k] - b[k];
  if (!a.pre.length || !b.pre.length) return b.pre.length - a.pre.length; // release > prerelease
  for (let i = 0; i < Math.max(a.pre.length, b.pre.length); i++) {
    const x = a.pre[i], y = b.pre[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xn = /^\d+$/.test(x), yn = /^\d+$/.test(y);
    if (xn && yn) { if (+x !== +y) return +x - +y; }
    else if (xn) return -1;
    else if (yn) return 1;
    else if (x !== y) return x < y ? -1 : 1;
  }
  return 0;
}

const read = (p) => readFileSync(p, "utf8");

export function cargoVersion(text = read(P.cargo)) {
  const sec = /^\[workspace\.package\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(text);
  const m = sec && /^version\s*=\s*"([^"]+)"/m.exec(sec[1]);
  if (!m) throw new Error("Cargo.toml: [workspace.package].version not found");
  return m[1];
}
export const jsonVersion = (p) => JSON.parse(read(p)).version;

/** Versions of the workspace crates recorded in Cargo.lock. */
export function lockVersions(text = read(P.cargoLock)) {
  const out = {};
  for (const name of LOCK_CRATES) {
    const m = new RegExp(`^name = "${name}"\\nversion = "([^"]+)"`, "m").exec(text);
    out[name] = m ? m[1] : null;
  }
  return out;
}
