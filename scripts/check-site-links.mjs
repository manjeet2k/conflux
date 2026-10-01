#!/usr/bin/env node
// Checks every link in the built site (_site/, from scripts/build-site.mjs):
//   - href/src attributes and the URLs in og:image / twitter:image / canonical / og:url,
//   - in-page anchors (#id) against the ids in the target page,
//   - links into the site itself (relative, or under SITE_URL) against the built files,
//   - external http(s) links with a real request (HEAD, falling back to GET; redirects followed).
// Exits non-zero on any broken link. Node built-ins only.
//
//   node scripts/check-site-links.mjs [--no-external]
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, normalize, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SITE = join(ROOT, "_site");
const REPO = process.env.GITHUB_REPOSITORY || "manjeet2k/conflux";
const [OWNER, NAME] = REPO.split("/");
const SITE_URL = process.env.SITE_URL || `https://${OWNER}.github.io/${NAME}/`;
const external = !process.argv.includes("--no-external");

if (!existsSync(SITE)) { console.error("No _site/: run node scripts/build-site.mjs first"); process.exit(2); }

const pages = [];
const walk = (d) => readdirSync(d).forEach((n) => {
  const p = join(d, n);
  if (statSync(p).isDirectory()) walk(p); else if (extname(p) === ".html") pages.push(p);
});
walk(SITE);

const idsOf = new Map();
const ids = (file) => {
  if (!idsOf.has(file)) idsOf.set(file, new Set([...readFileSync(file, "utf8").matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
  return idsOf.get(file);
};

const problems = [];
const externalUrls = new Map(); // url -> [where]
for (const page of pages) {
  const html = readFileSync(page, "utf8").replace(/<!--[\s\S]*?-->/g, "");
  const where = relative(SITE, page);
  const refs = [
    ...[...html.matchAll(/\s(?:href|src)="([^"]*)"/g)].map((m) => m[1]),
    ...[...html.matchAll(/<meta\s+(?:property|name)="(?:og:image|og:image:secure_url|og:url|twitter:image)"\s+content="([^"]*)"/g)].map((m) => m[1]),
  ];
  for (const raw of refs) {
    const url = raw.replaceAll("&amp;", "&");
    if (!url) { problems.push(`${where}: empty link`); continue; }
    if (/^(mailto|tel|data):/.test(url)) continue;
    let local = null, hash = "";
    if (url.startsWith(SITE_URL)) local = url.slice(SITE_URL.length);
    else if (/^https?:\/\//.test(url)) { (externalUrls.get(url) ?? externalUrls.set(url, []).get(url)).push(where); continue; }
    else local = url;
    [local, hash] = local.split("#");
    let target = local ? normalize(join(local.startsWith("/") ? SITE : dirname(page), local.replace(/^\//, ""))) : page;
    if (local && (local.endsWith("/") || local === "." || local === "./")) target = join(target, "index.html");
    if (!existsSync(target)) { problems.push(`${where}: ${raw} -> missing file ${relative(ROOT, target)}`); continue; }
    if (hash && extname(target) === ".html" && !ids(target).has(hash)) problems.push(`${where}: ${raw} -> no element with id "${hash}"`);
  }
}

async function check(url) {
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      let res = await fetch(url, { method: "HEAD", redirect: "follow", headers: { "User-Agent": "conflux-link-check" } });
      if (res.status === 405 || res.status === 403 || res.status === 400) res = await fetch(url, { redirect: "follow", headers: { "User-Agent": "conflux-link-check" } });
      if (res.status < 400) return null;
      if (res.status !== 429 && res.status < 500) return `HTTP ${res.status}`;
    } catch (e) {
      if (attempt === 3) return `request failed: ${e.message}`;
    }
    await new Promise((r) => setTimeout(r, 1000 * attempt));
  }
  return "HTTP 5xx/429 after 3 attempts";
}

if (external) {
  const urls = [...externalUrls.keys()];
  let i = 0;
  await Promise.all(Array.from({ length: 6 }, async () => {
    while (i < urls.length) {
      const url = urls[i++];
      const err = await check(url);
      if (err) problems.push(`${externalUrls.get(url)[0]}: ${url} -> ${err}`);
    }
  }));
}

const n = pages.length;
if (problems.length) {
  console.error(`Broken links (${problems.length}):\n  ${problems.join("\n  ")}`);
  process.exit(1);
}
console.log(`ok: ${n} page(s), all internal links and anchors resolve${external ? `, ${externalUrls.size} external URLs reachable` : " (external not checked)"}`);
