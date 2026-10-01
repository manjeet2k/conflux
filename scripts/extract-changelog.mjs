#!/usr/bin/env node
// Print the CHANGELOG.md section for a version (release notes). Falls back to "Unreleased"
// when the version has no section of its own.
//   node scripts/extract-changelog.mjs 0.2.0-beta.1 [--file CHANGELOG.md]
// Exits non-zero if neither the version nor Unreleased has any content.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { ROOT } from "./lib.mjs";

/** Body of the `## [heading]` section, without the heading line; null if absent. */
export function section(text, heading) {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const want = heading.toLowerCase();
  const start = lines.findIndex((l) => {
    const m = /^##\s+\[?([^\]\s]+)\]?/.exec(l);
    return m && m[1].toLowerCase() === want;
  });
  if (start < 0) return null;
  let end = lines.findIndex((l, i) => i > start && /^##\s/.test(l));
  if (end < 0) end = lines.length;
  // Drop link-reference definitions that trail the last section.
  const body = lines.slice(start + 1, end).filter((l) => !/^\[[^\]]+\]:\s/.test(l));
  return body.join("\n").trim();
}

/** Notes for `version` (a leading "v" is ignored), falling back to Unreleased. */
export function notesFor(text, version) {
  const v = version.replace(/^v/, "");
  for (const heading of [v, "Unreleased"]) {
    const body = section(text, heading);
    if (body) return { heading, body };
  }
  return null;
}

import { fileURLToPath } from "node:url";
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const fi = args.indexOf("--file");
  const file = fi >= 0 ? args.splice(fi, 2)[1] : join(ROOT, "CHANGELOG.md");
  const version = args[0];
  if (!version) {
    console.error("usage: extract-changelog.mjs <version> [--file CHANGELOG.md]");
    process.exit(2);
  }
  const found = notesFor(readFileSync(file, "utf8"), version);
  if (!found) {
    console.error(`No changelog content for ${version} or Unreleased in ${file}`);
    process.exit(1);
  }
  if (found.heading !== version.replace(/^v/, "")) {
    console.error(`note: no section for ${version}; using ${found.heading}`);
  }
  process.stdout.write(found.body + "\n");
}
