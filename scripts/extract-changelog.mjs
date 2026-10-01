#!/usr/bin/env node
// Print the CHANGELOG.md section for a version (release notes).
//   node scripts/extract-changelog.mjs 0.2.0-beta.1 [--file CHANGELOG.md] [--allow-unreleased]
// Exits non-zero when the version has no non-empty section of its own (the release workflow
// relies on this). Only with --allow-unreleased does it fall back to "Unreleased", for previews.
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

/** Notes for `version` (a leading "v" is ignored); Unreleased only as an explicit fallback. */
export function notesFor(text, version, allowUnreleased = false) {
  const v = version.replace(/^v/, "");
  for (const heading of allowUnreleased ? [v, "Unreleased"] : [v]) {
    const body = section(text, heading);
    if (body) return { heading, body };
  }
  return null;
}

import { fileURLToPath } from "node:url";
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes("--self-test")) {
    const t = "## [Unreleased]\n- u\n\n## [1.0.0] - d\n- a\n\n## [0.9.0]\n- b\n";
    const ok =
      notesFor(t, "v1.0.0")?.body === "- a" &&
      notesFor(t, "1.0.1") === null &&
      notesFor(t, "1.0.1", true)?.heading === "Unreleased" &&
      notesFor("## [Unreleased]\n\n## [1.0.0]\n- a\n", "1.0.1", true) === null;
    if (!ok) { console.error("extract-changelog self-test FAILED"); process.exit(1); }
    console.log("extract-changelog self-test: ok");
    process.exit(0);
  }
  const ai = args.indexOf("--allow-unreleased");
  const allowUnreleased = ai >= 0;
  if (ai >= 0) args.splice(ai, 1);
  const fi = args.indexOf("--file");
  const file = fi >= 0 ? args.splice(fi, 2)[1] : join(ROOT, "CHANGELOG.md");
  const version = args[0];
  if (!version) {
    console.error("usage: extract-changelog.mjs <version> [--file CHANGELOG.md] [--allow-unreleased]");
    process.exit(2);
  }
  const found = notesFor(readFileSync(file, "utf8"), version, allowUnreleased);
  if (!found) {
    console.error(`::error::No changelog section with content for ${version.replace(/^v/, "")} in ${file}. Rename [Unreleased] to the version first (docs/RELEASING.md).`);
    process.exit(1);
  }
  if (found.heading !== version.replace(/^v/, "")) {
    console.error(`note: no section for ${version}; using ${found.heading}`);
  }
  process.stdout.write(found.body + "\n");
}
