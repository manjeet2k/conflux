#!/usr/bin/env node
// Generate the Tauri updater signing key pair (one time, on the maintainer's machine).
//
//   node scripts/setup-updater-key.mjs                 # keys -> ~/.config/conflux-secrets/, pubkey -> tauri.conf.json
//   node scripts/setup-updater-key.mjs --out DIR --no-config   # test run: touch nothing in the repo
//   node scripts/setup-updater-key.mjs --force         # replace an existing key (strands installed copies!)
//
// The private key and its password are written ONLY outside the repository. Only the public
// key goes into crates/conflux-desktop/tauri.conf.json (plugins.updater.pubkey).
import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, relative, resolve, isAbsolute } from "node:path";
import { P, ROOT } from "./lib.mjs";

const args = process.argv.slice(2);
const flag = (n) => args.includes(n);
const value = (n) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);

const outDir = resolve(value("--out") ?? join(homedir(), ".config", "conflux-secrets"));
const writeConfig = !flag("--no-config");
const force = flag("--force");

// Refuse to put secrets inside the repository.
const rel = relative(ROOT, outDir);
if (rel === "" || (!rel.startsWith("..") && !isAbsolute(rel))) {
  console.error(`Refusing to write secrets inside the repository (${outDir}).`);
  process.exit(1);
}

const keyPath = join(outDir, "updater.key");
const pubPath = `${keyPath}.pub`;
const passPath = join(outDir, "updater.key.password");
if (existsSync(keyPath) && !force) {
  console.error(`${keyPath} already exists. Back it up, or pass --force to replace it.`);
  console.error("Replacing the key strands every installed copy: they can no longer verify updates.");
  process.exit(1);
}

mkdirSync(outDir, { recursive: true, mode: 0o700 });
chmodSync(outDir, 0o700);
const password = randomBytes(24).toString("base64url");
// KNOWN EXPOSURE: `tauri signer generate` only accepts the password via `--password` (no env
// var or stdin option; checked with `tauri signer generate --help`). For the second or so that
// the process runs, the password is visible in the process list (`ps`, /proc/<pid>/cmdline) to
// other local users. Run this only on a single-user machine you trust. (`tauri signer sign`
// reads TAURI_SIGNING_PRIVATE_KEY_PASSWORD from the environment, so signing is not affected.)
const tauriArgs = ["--prefix", join(ROOT, "ui"), "tauri", "signer", "generate", "--ci", "-w", keyPath, "--password", password];
if (force) tauriArgs.push("--force");
execFileSync("npx", tauriArgs, { stdio: ["ignore", "ignore", "inherit"] });

writeFileSync(passPath, password + "\n", { mode: 0o600 });
for (const f of [keyPath, pubPath, passPath]) chmodSync(f, 0o600);

const pubkey = readFileSync(pubPath, "utf8").trim();
if (writeConfig) {
  const conf = JSON.parse(readFileSync(P.tauri, "utf8"));
  conf.plugins ??= {};
  conf.plugins.updater ??= {};
  conf.plugins.updater.pubkey = pubkey;
  writeFileSync(P.tauri, JSON.stringify(conf, null, 2) + "\n");
  console.log(`Public key written to ${relative(ROOT, P.tauri)} (plugins.updater.pubkey).`);
} else {
  console.log("--no-config: tauri.conf.json left untouched.");
}

console.log(`
Private key : ${keyPath}
Password    : ${passPath}
Public key  : ${pubPath}

Set these GitHub repository secrets (Settings > Secrets and variables > Actions):
  TAURI_SIGNING_PRIVATE_KEY           = contents of ${keyPath}
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD  = contents of ${passPath}
  VIRUSTOTAL_API_KEY                  = (optional) your VirusTotal API key
e.g.  gh secret set TAURI_SIGNING_PRIVATE_KEY < ${keyPath}
      gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < ${passPath}

BACK UP both files somewhere safe and offline (password manager). If the private key is
lost, no installed copy can ever auto-update again. Never commit them.
`);
