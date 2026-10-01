#!/usr/bin/env node
// Gate for the updater manifest, run by release.yml (publish-updater) BEFORE it overwrites
// updater-beta/latest.json. Node built-ins only, so it works on any runner.
//
//   node scripts/verify-updater-manifest.mjs --manifest latest.json --installer-dir DIR \
//        --conf crates/conflux-desktop/tauri.conf.json --repo owner/name --tag v0.2.0-beta.2 \
//        [--current published-latest.json]
//   node scripts/verify-updater-manifest.mjs --self-test
//
// Checks, for every platform entry:
//   (a) its signature verifies (minisign, Ed25519) over the installer file named by its url,
//       against plugins.updater.pubkey in tauri.conf.json;
//   (b) manifest.version == the tag's version, and is strictly greater (semver, prerelease
//       aware) than the version in --current (skipped when --current is absent);
//   (c) url starts with https://github.com/<repo>/releases/download/<tag>/.
//
// Tauri's .sig is base64(minisign signature file); the pubkey is base64(minisign .pub file).
// Minisign formats: pubkey bytes = "Ed" + keyid(8) + ed25519 pk(32); signature bytes =
// "Ed" (sign the file) or "ED" (sign the BLAKE2b-512 of the file) + keyid(8) + sig(64); the
// trusted comment is covered by a second signature over sig(64) + comment.
import { createHash, createPublicKey, verify, generateKeyPairSync, sign } from "node:crypto";
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { basename, join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { parseSemver, compareSemver } from "./lib.mjs";

const SPKI_ED25519 = Buffer.from("302a300506032b6570032100", "hex");
const b64 = (s) => Buffer.from(s.trim(), "base64");
const edKey = (raw) => createPublicKey({ key: Buffer.concat([SPKI_ED25519, raw]), format: "der", type: "spki" });

function parsePubkey(pubkeyB64) {
  const lines = b64(pubkeyB64).toString("utf8").split(/\r?\n/).filter(Boolean);
  const raw = b64(lines[1] ?? "");
  if (raw.length !== 42 || raw.toString("latin1", 0, 2) !== "Ed") throw new Error("pubkey is not a minisign public key");
  return { keyId: raw.subarray(2, 10), pk: raw.subarray(10) };
}

/** Throws unless `sigB64` (Tauri .sig content) is a valid signature of `data` by `pubkeyB64`. */
export function verifyMinisign(pubkeyB64, sigB64, data) {
  const { keyId, pk } = parsePubkey(pubkeyB64);
  const lines = b64(sigB64).toString("utf8").split(/\r?\n/).filter(Boolean);
  // lines: untrusted comment, base64 signature, trusted comment, base64 global signature
  if (lines.length < 4) throw new Error("malformed signature file");
  const raw = b64(lines[1]);
  const algo = raw.toString("latin1", 0, 2);
  if (raw.length !== 74 || (algo !== "Ed" && algo !== "ED")) throw new Error("unknown signature algorithm");
  if (!raw.subarray(2, 10).equals(keyId)) throw new Error("signature was made by a different key (key id mismatch)");
  const sig = raw.subarray(10);
  const key = edKey(pk);
  const msg = algo === "ED" ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, msg, key, sig)) throw new Error("signature does not match the installer");
  const prefix = "trusted comment: ";
  if (!lines[2].startsWith(prefix)) throw new Error("missing trusted comment");
  const global = Buffer.concat([sig, Buffer.from(lines[2].slice(prefix.length), "utf8")]);
  if (!verify(null, global, key, b64(lines[3]))) throw new Error("trusted-comment signature invalid");
}

/** Returns a list of problems (empty = manifest is good). `readInstaller(name)` returns a Buffer. */
export function checkManifest({ manifest, pubkey, repo, tag, current, readInstaller }) {
  const errs = [];
  const version = tag.replace(/^v/, "");
  const v = parseSemver(String(manifest.version));
  if (!v) errs.push(`manifest version '${manifest.version}' is not valid semver`);
  if (manifest.version !== version) errs.push(`manifest version ${manifest.version} != tag version ${version}`);
  if (v && current) {
    const c = parseSemver(String(current.version));
    if (!c) errs.push(`currently published version '${current.version}' is not valid semver`);
    else if (compareSemver(v, c) <= 0) errs.push(`version ${manifest.version} is not strictly greater than published ${current.version}`);
  }
  const entries = Object.entries(manifest.platforms ?? {});
  if (!entries.length) errs.push("manifest has no platforms");
  const prefix = `https://github.com/${repo}/releases/download/${tag}/`;
  for (const [plat, e] of entries) {
    if (typeof e.url !== "string" || !e.url.startsWith(prefix)) { errs.push(`${plat}: url '${e.url}' does not start with ${prefix}`); continue; }
    const name = e.url.slice(prefix.length);
    if (!name || /[\/\\?#]/.test(name)) { errs.push(`${plat}: unexpected url tail '${name}'`); continue; }
    if (!e.signature) { errs.push(`${plat}: missing signature`); continue; }
    try { verifyMinisign(pubkey, e.signature, readInstaller(name)); }
    catch (err) { errs.push(`${plat}: ${err.message}`); }
  }
  return errs;
}

function selfTest() {
  const assert = (c, m) => { if (!c) { console.error("SELF-TEST FAILED: " + m); process.exit(1); } };
  const mkKey = () => {
    const { publicKey, privateKey } = generateKeyPairSync("ed25519");
    const pk = publicKey.export({ format: "der", type: "spki" }).subarray(12);
    const keyId = Buffer.from("0102030405060708", "hex");
    const pub = Buffer.from(`untrusted comment: test\n${Buffer.concat([Buffer.from("Ed"), keyId, pk]).toString("base64")}\n`).toString("base64");
    return { pub, privateKey, keyId };
  };
  const mkSig = ({ privateKey, keyId }, data, algo = "ED") => {
    const msg = algo === "ED" ? createHash("blake2b512").update(data).digest() : data;
    const s = sign(null, msg, privateKey);
    const tc = "timestamp:1 file:x";
    const g = sign(null, Buffer.concat([s, Buffer.from(tc)]), privateKey);
    const raw = Buffer.concat([Buffer.from(algo), keyId, s]).toString("base64");
    return Buffer.from(`untrusted comment: sig\n${raw}\ntrusted comment: ${tc}\n${g.toString("base64")}\n`).toString("base64");
  };
  const k = mkKey(), other = mkKey();
  const exe = Buffer.from("installer-bytes");
  const repo = "o/r", tag = "v0.2.0-beta.2", name = "Conflux_0.2.0-beta.2_x64-setup.exe";
  const good = () => ({
    version: "0.2.0-beta.2", notes: "n", pub_date: "x",
    platforms: { "windows-x86_64": { signature: mkSig(k, exe), url: `https://github.com/${repo}/releases/download/${tag}/${name}` } },
  });
  const run = (m, over = {}) => checkManifest({ manifest: m, pubkey: k.pub, repo, tag, current: undefined, readInstaller: () => exe, ...over });
  const win = (m) => m.platforms["windows-x86_64"];

  assert(run(good()).length === 0, "good manifest rejected: " + run(good()));
  let m = good(); win(m).signature = mkSig(k, exe, "Ed");
  assert(run(m).length === 0, "legacy Ed signature rejected");
  assert(run(good(), { current: { version: "0.2.0-beta.1" } }).length === 0, "beta.2 > beta.1 rejected");
  assert(run(good(), { current: { version: "0.2.0-beta.2" } }).length === 1, "equal version accepted");
  assert(run(good(), { current: { version: "0.2.0" } }).length === 1, "downgrade to prerelease of 0.2.0 accepted");
  assert(run(good(), { current: { version: "0.2.0-beta.10" } }).length === 1, "beta.10 numeric ordering wrong");
  m = good(); m.version = "0.2.0-beta.3";
  assert(run(m).length > 0, "version != tag accepted");
  m = good(); win(m).url = `https://github.com/evil/r/releases/download/${tag}/${name}`;
  assert(run(m).length === 1, "foreign repo url accepted");
  m = good(); win(m).url = `https://github.com/${repo}/releases/download/v9.9.9/${name}`;
  assert(run(m).length === 1, "foreign tag url accepted");
  m = good(); win(m).url = `https://github.com/${repo}/releases/download/${tag}/../x.exe`;
  assert(run(m).length === 1, "path traversal url accepted");
  m = good(); win(m).signature = mkSig(other, exe);
  assert(run(m).length === 1, "signature from another key accepted");
  m = good(); win(m).signature = mkSig(k, Buffer.from("different"));
  assert(run(m).length === 1, "signature over other bytes accepted");
  assert(run(good(), { readInstaller: () => Buffer.from("tampered") }).length === 1, "tampered installer accepted");
  m = good(); delete win(m).signature;
  assert(run(m).length === 1, "missing signature accepted");
  console.log("verify-updater-manifest self-test: ok");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes("--self-test")) { selfTest(); process.exit(0); }
  const opt = (n) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : undefined; };
  const need = (n) => opt(n) ?? (console.error(`missing ${n}`), process.exit(2));
  const manifest = JSON.parse(readFileSync(need("--manifest"), "utf8"));
  const conf = JSON.parse(readFileSync(need("--conf"), "utf8"));
  const dir = need("--installer-dir");
  const currentPath = opt("--current");
  const errs = checkManifest({
    manifest,
    pubkey: conf.plugins.updater.pubkey,
    repo: need("--repo"),
    tag: need("--tag"),
    current: currentPath ? JSON.parse(readFileSync(currentPath, "utf8")) : undefined,
    readInstaller: (name) => readFileSync(join(dir, basename(name))),
  });
  if (errs.length) { for (const e of errs) console.error("::error::" + e); process.exit(1); }
  console.log(`updater manifest ${manifest.version}: signature, version order and urls OK`);
}
