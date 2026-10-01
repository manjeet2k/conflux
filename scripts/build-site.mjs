#!/usr/bin/env node
// Builds the GitHub Pages site: copies site/ to _site/ and fills {{PLACEHOLDERS}} with the newest
// published release (betas included), so the page works without JavaScript. site/app.js then
// refreshes the links at view time. Node built-ins only.
//
//   node scripts/build-site.mjs              # uses the GitHub API (GITHUB_TOKEN optional)
//   node scripts/build-site.mjs --offline    # no network: links fall back to the Releases page
import { cpSync, existsSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, extname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "site");
const OUT = join(ROOT, "_site");
const REPO = process.env.GITHUB_REPOSITORY || "manjeet2k/conflux";
const [OWNER, NAME] = REPO.split("/");
const SITE_URL = process.env.SITE_URL || `https://${OWNER}.github.io/${NAME}/`;
const REPO_URL = `https://github.com/${REPO}`;
const RELEASES = `${REPO_URL}/releases`;
const offline = process.argv.includes("--offline");

async function newestRelease() {
  if (offline) return null;
  const headers = { Accept: "application/vnd.github+json", "User-Agent": "conflux-site-build" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const res = await fetch(`https://api.github.com/repos/${REPO}/releases?per_page=20`, { headers });
  if (!res.ok) throw new Error(`GitHub API ${res.status}: ${await res.text()}`);
  const list = await res.json();
  // Same rule as app.js: newest non-draft release with a version tag (skips `updater-beta`).
  return list.find((r) => !r.draft && /^v\d+\.\d+\.\d+/.test(r.tag_name)) ?? null;
}

const rel = await newestRelease();
const version = rel ? rel.tag_name.slice(1) : "";
const asset = (name) => rel?.assets.find((a) => a.name === name);
const setup = rel && asset(`Conflux_${version}_x64-setup.exe`);
if (rel && !setup) throw new Error(`release ${rel.tag_name} has no Conflux_${version}_x64-setup.exe`);

const values = {
  SITE_URL,
  REPO_URL,
  VERSION: version || "beta",
  DOWNLOAD_URL: setup?.browser_download_url ?? RELEASES,
  DOWNLOAD_SIZE: setup ? `${(setup.size / 1048576).toFixed(1)} MB` : "Windows 10/11, 64-bit",
  OFFLINE_URL: asset(`Conflux_${version}_x64-offline-setup.exe`)?.browser_download_url ?? RELEASES,
  SUMS_URL: asset("SHA256SUMS.txt")?.browser_download_url ?? RELEASES,
  RELEASE_URL: rel?.html_url ?? RELEASES,
  BUILD_DATE: new Date().toISOString().slice(0, 10),
};

if (existsSync(OUT)) rmSync(OUT, { recursive: true });
cpSync(SRC, OUT, { recursive: true });

const TEXT = new Set([".html", ".xml", ".txt", ".js", ".css", ".json"]);
let left = [];
const walk = (dir) => {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) { walk(p); continue; }
    if (!TEXT.has(extname(p))) continue;
    const out = readFileSync(p, "utf8").replace(/\{\{([A-Z_]+)\}\}/g, (m, k) => (k in values ? values[k] : m));
    for (const m of out.matchAll(/\{\{[A-Z_]+\}\}/g)) left.push(`${p}: ${m[0]}`);
    writeFileSync(p, out);
  }
};
walk(OUT);
if (left.length) throw new Error(`unfilled placeholders:\n${left.join("\n")}`);
writeFileSync(join(OUT, ".nojekyll"), "");
console.log(`site built in _site/ for ${SITE_URL} (release: ${rel ? rel.tag_name : "none, links -> Releases page"})`);
