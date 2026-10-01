#!/usr/bin/env node
// Generate THIRD_PARTY_LICENSES.md: every third-party crate that ships in the Windows build
// (from `cargo metadata`) and every production npm package bundled into the UI (from
// ui/package-lock.json + ui/node_modules). Fails on a missing or UNKNOWN licence.
//
//   node scripts/gen-licenses.mjs           # (re)write THIRD_PARTY_LICENSES.md
//   node scripts/gen-licenses.mjs --check   # fail if the file is stale (no write)
//
// Needs `npm --prefix ui ci` to have run (licences are read from node_modules) and the cargo
// registry sources (`cargo fetch`).
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { ROOT } from "./lib.mjs";

const OUT = join(ROOT, "THIRD_PARTY_LICENSES.md");
const check = process.argv.includes("--check");
const TARGET = "x86_64-pc-windows-msvc";
const ROOT_CRATE = "conflux-desktop";
const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense)([-_. ].*)?$/i;

const problems = [];
const bad = (lic) => !lic || /^(UNKNOWN|UNLICENSED|SEE LICENSE.*|)$/i.test(String(lic).trim());

/** Licence/notice files in a package directory: [{ file, text }]. */
function licenseTexts(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && LICENSE_FILE.test(e.name))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((e) => ({ file: e.name, text: readFileSync(join(dir, e.name), "utf8").replace(/\r\n/g, "\n").trim() }))
    .filter((t) => t.text);
}

// ---- Rust crates ----
const meta = JSON.parse(
  execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET],
    { cwd: ROOT, maxBuffer: 512 * 1024 * 1024, encoding: "utf8" },
  ),
);
const byId = new Map(meta.packages.map((p) => [p.id, p]));
const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
const workspace = new Set(meta.workspace_members);
const rootId = meta.packages.find((p) => p.name === ROOT_CRATE)?.id;
if (!rootId) throw new Error(`${ROOT_CRATE} not found in cargo metadata`);

// Normal (non-dev, non-build) dependencies reachable from the desktop app: what ships.
const seen = new Set([rootId]);
const queue = [rootId];
while (queue.length) {
  for (const dep of nodes.get(queue.pop())?.deps ?? []) {
    if (!dep.dep_kinds.some((k) => k.kind === null)) continue;
    if (!seen.has(dep.pkg)) { seen.add(dep.pkg); queue.push(dep.pkg); }
  }
}

const entries = []; // { ecosystem, name, version, license, repository, dir }
for (const id of seen) {
  if (workspace.has(id)) continue;
  const p = byId.get(id);
  if (bad(p.license)) problems.push(`crate ${p.name} ${p.version}: licence missing/UNKNOWN`);
  entries.push({
    ecosystem: "Rust",
    name: p.name,
    version: p.version,
    license: p.license ?? "UNKNOWN",
    repository: p.repository ?? p.homepage ?? "",
    dir: dirname(p.manifest_path),
  });
}

// ---- npm production dependencies ----
const lock = JSON.parse(readFileSync(join(ROOT, "ui/package-lock.json"), "utf8"));
for (const [path, info] of Object.entries(lock.packages ?? {})) {
  if (path === "" || info.dev || info.devOptional) continue;
  const dir = join(ROOT, "ui", path);
  const pkgFile = join(dir, "package.json");
  if (!existsSync(pkgFile)) {
    // Optional platform-specific packages that npm did not install on this OS.
    if (info.optional) continue;
    problems.push(`npm ${path}: not installed (run npm --prefix ui ci)`);
    continue;
  }
  const pkg = JSON.parse(readFileSync(pkgFile, "utf8"));
  let lic = pkg.license ?? info.license;
  if (lic && typeof lic === "object") lic = lic.type;
  if (!lic && Array.isArray(pkg.licenses)) lic = pkg.licenses.map((l) => l.type).join(" OR ");
  if (bad(lic)) problems.push(`npm ${pkg.name} ${pkg.version}: licence missing/UNKNOWN`);
  let repo = typeof pkg.repository === "string" ? pkg.repository : pkg.repository?.url ?? pkg.homepage ?? "";
  repo = repo.replace(/^git\+/, "").replace(/\.git$/, "").replace(/^github:/, "https://github.com/");
  entries.push({ ecosystem: "npm", name: pkg.name, version: pkg.version, license: lic ?? "UNKNOWN", repository: repo, dir });
}

if (problems.length) {
  console.error("Licence problems:\n  " + problems.join("\n  "));
  process.exit(1);
}

entries.sort((a, b) => a.ecosystem.localeCompare(b.ecosystem) || a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

// Unique licence texts, each with the packages that ship it.
const texts = new Map(); // hash -> { text, files:Set, packages:[] }
const missingText = [];
for (const e of entries) {
  const found = licenseTexts(e.dir);
  if (!found.length) missingText.push(`${e.name} ${e.version} (${e.license})`);
  for (const t of found) {
    const h = createHash("sha256").update(t.text).digest("hex");
    if (!texts.has(h)) texts.set(h, { text: t.text, packages: [] });
    texts.get(h).packages.push(`${e.name} ${e.version}`);
  }
}

const cell = (s) => String(s).replace(/\|/g, "\\|");
const lines = [
  "# Third-party licenses",
  "",
  "Conflux itself is dual-licensed under MIT OR Apache-2.0 (see `LICENSE-MIT` and `LICENSE-APACHE`).",
  "It includes the third-party components below, each under its own license.",
  "",
  "_Generated by `node scripts/gen-licenses.mjs` from `cargo metadata` (Windows x86_64 target, normal",
  "dependencies) and `ui/package-lock.json` (production dependencies). Do not edit by hand._",
  "",
  `## Components (${entries.length})`,
  "",
  "| Ecosystem | Name | Version | License | Source |",
  "|-----------|------|---------|---------|--------|",
  ...entries.map((e) => `| ${e.ecosystem} | ${cell(e.name)} | ${e.version} | ${cell(e.license)} | ${cell(e.repository)} |`),
  "",
  "## License texts",
  "",
  "Texts are those shipped in each package; identical texts are listed once with the packages",
  "that carry them.",
  "",
];
const sorted = [...texts.values()].sort((a, b) => a.packages[0].localeCompare(b.packages[0]) || a.text.localeCompare(b.text));
for (const t of sorted) {
  const shown = t.packages.length > 12 ? [...t.packages.slice(0, 12), `and ${t.packages.length - 12} more`] : t.packages;
  lines.push("<details>", `<summary>${shown.join(", ")}</summary>`, "", "```text", t.text, "```", "", "</details>", "");
}
if (missingText.length) {
  lines.push("## Packages without a license file in their distribution", "",
    "These declare their license in package metadata (listed above) but ship no separate text:", "",
    ...missingText.map((m) => `- ${m}`), "");
}
const output = lines.join("\n");

if (check) {
  // Compare with LF line endings: a Windows checkout may have converted the file to CRLF.
  const current = existsSync(OUT) ? readFileSync(OUT, "utf8").replace(/\r\n/g, "\n") : "";
  if (current !== output) {
    console.error("THIRD_PARTY_LICENSES.md is stale. Run: node scripts/gen-licenses.mjs");
    process.exit(1);
  }
  console.log("THIRD_PARTY_LICENSES.md is up to date.");
} else {
  writeFileSync(OUT, output);
  console.log(`Wrote THIRD_PARTY_LICENSES.md: ${entries.length} components, ${texts.size} distinct license texts.`);
}
