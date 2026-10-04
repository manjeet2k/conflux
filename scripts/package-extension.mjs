#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const EXT_DIR = path.join(ROOT, "extensions", "conflux-browser");
const DIST_DIR = path.join(ROOT, "target", "extension-dist");

fs.mkdirSync(DIST_DIR, { recursive: true });

// Read source manifest
const rawManifest = fs.readFileSync(path.join(EXT_DIR, "manifest.json"), "utf8");
const manifest = JSON.parse(rawManifest);

// Prepare store manifest (strip 'key' field required by Chrome Web Store and Firefox AMO)
const storeManifest = { ...manifest };
delete storeManifest.key;

const storeManifestContent = JSON.stringify(storeManifest, null, 2) + "\n";

// Use Python's built-in zipfile to produce reproducible zip files across platforms
const pyScript = `
import os, zipfile, sys

dist_dir = sys.argv[1]
ext_dir = sys.argv[2]
store_manifest = sys.argv[3]

targets = [
    os.path.join(dist_dir, "conflux-browser.zip"),
    os.path.join(dist_dir, "conflux-browser-store.zip"),
    os.path.join(dist_dir, f"conflux-browser-v{${JSON.stringify(manifest.version)}}.zip"),
]

# If Windows Downloads exists in WSL2, copy there too
win_downloads = "/mnt/c/Users/pc/Downloads"
if os.path.isdir(win_downloads):
    targets.append(os.path.join(win_downloads, "conflux-browser.zip"))
    targets.append(os.path.join(win_downloads, "conflux-browser-store-upload.zip"))

for target in targets:
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as zf:
        for root, dirs, files in os.walk(ext_dir):
            for file in sorted(files):
                if file.endswith(".md"):
                    continue
                file_path = os.path.join(root, file)
                arcname = os.path.relpath(file_path, ext_dir)
                if arcname == "manifest.json":
                    zf.writestr("manifest.json", store_manifest)
                else:
                    zf.write(file_path, arcname)
    print(f"Packaged: {target}")
`;

execFileSync("python3", ["-c", pyScript, DIST_DIR, EXT_DIR, storeManifestContent], {
  stdio: "inherit",
});

console.log("\nExtension store packages created successfully without 'key' field.");
