#!/usr/bin/env node
// Prints the status of every task in docs/ROADMAP.md, grouped by status, so nobody has to
// maintain a hand-written summary. Usage: node scripts/roadmap-status.mjs [--todo]
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const text = readFileSync(join(root, "docs/ROADMAP.md"), "utf8");
const names = { " ": "todo", "~": "in progress", x: "done", "!": "blocked", "-": "deferred/dropped" };
const groups = { todo: [], "in progress": [], blocked: [], done: [], "deferred/dropped": [] };

for (const m of text.matchAll(/^#### ([A-Z]+-\d+) — (.+?)\s+`\[(.)\]`(.*)$/gm)) {
  const [, id, title, mark, rest] = m;
  const gate = /Gate: ([^`]+?)\s*$/.exec(rest)?.[1];
  groups[names[mark] ?? "todo"].push(`${id}  ${title}${gate ? `  (gate: ${gate})` : ""}`);
}

const todoOnly = process.argv.includes("--todo");
for (const [name, items] of Object.entries(groups)) {
  if (todoOnly && (name === "done" || name === "deferred/dropped")) continue;
  console.log(`\n${name.toUpperCase()} (${items.length})`);
  for (const i of items) console.log(`  ${i}`);
}
