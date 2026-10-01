#!/usr/bin/env node
// Fails if any relative Markdown link in the repo's docs points at a missing file.
import { readFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const files = execFileSync("git", ["ls-files", "*.md"], { cwd: root, encoding: "utf8" })
  .split("\n").filter(Boolean);
let bad = 0;
for (const f of files) {
  const text = readFileSync(join(root, f), "utf8").replace(/```[\s\S]*?```/g, "");
  for (const m of text.matchAll(/\]\(([^)\s]+)\)/g)) {
    const target = m[1].split("#")[0];
    if (!target || /^[a-z]+:/i.test(target)) continue;
    if (!existsSync(resolve(root, dirname(f), target))) {
      console.error(`${f}: broken link -> ${m[1]}`);
      bad++;
    }
  }
}
if (bad) process.exit(1);
console.log(`ok: ${files.length} markdown files, no broken relative links`);
