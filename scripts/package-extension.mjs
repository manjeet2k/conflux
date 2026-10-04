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

// 1. Universal store manifest (strip 'key', dual background fallback, full gecko settings)
const universalManifest = { ...manifest };
delete universalManifest.key;

// 2. Firefox AMO manifest (strip 'key', pure scripts background for zero-warning AMO validation)
const firefoxManifest = { ...manifest };
delete firefoxManifest.key;
firefoxManifest.background = {
  scripts: ["background.js"],
  type: "module"
};

// 3. Chrome Web Store manifest (strip 'key', service_worker background, omit browser_specific_settings)
const chromeManifest = { ...manifest };
delete chromeManifest.key;
delete chromeManifest.browser_specific_settings;
chromeManifest.background = {
  service_worker: "background.js",
  type: "module"
};

const targetsConfig = {
  [path.join(DIST_DIR, "conflux-browser.zip")]: null, // raw dev manifest
  [path.join(DIST_DIR, "conflux-browser-store.zip")]: JSON.stringify(universalManifest, null, 2) + "\n",
  [path.join(DIST_DIR, "conflux-browser-firefox.zip")]: JSON.stringify(firefoxManifest, null, 2) + "\n",
  [path.join(DIST_DIR, "conflux-browser-chrome.zip")]: JSON.stringify(chromeManifest, null, 2) + "\n",
  [path.join(DIST_DIR, `conflux-browser-v${manifest.version}.zip`)]: JSON.stringify(universalManifest, null, 2) + "\n",
};

// Optional extra destination directory (e.g. CONFLUX_DEV_DIST=/mnt/c/Users/.../Downloads)
const extraDist = process.env.CONFLUX_DEV_DIST;
if (extraDist && fs.existsSync(extraDist)) {
  targetsConfig[path.join(extraDist, "conflux-browser.zip")] = null;
  targetsConfig[path.join(extraDist, "conflux-browser-firefox.zip")] = JSON.stringify(firefoxManifest, null, 2) + "\n";
  targetsConfig[path.join(extraDist, "conflux-browser-chrome.zip")] = JSON.stringify(chromeManifest, null, 2) + "\n";
  targetsConfig[path.join(extraDist, "conflux-browser-store-upload.zip")] = JSON.stringify(universalManifest, null, 2) + "\n";
}

// Use Python's built-in zipfile to produce reproducible zip files across platforms
const pyScript = `
import os, zipfile, sys, json

dist_dir = sys.argv[1]
ext_dir = sys.argv[2]
configs = json.loads(sys.argv[3])

for target, manifest_content in configs.items():
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as zf:
        for root, dirs, files in os.walk(ext_dir):
            for file in sorted(files):
                if file.endswith(".md"):
                    continue
                file_path = os.path.join(root, file)
                arcname = os.path.relpath(file_path, ext_dir)
                if arcname == "manifest.json":
                    if manifest_content is not None:
                        zf.writestr("manifest.json", manifest_content)
                    else:
                        zf.write(file_path, arcname)
                else:
                    zf.write(file_path, arcname)
    print(f"Packaged: {target}")
`;

execFileSync("python3", ["-c", pyScript, DIST_DIR, EXT_DIR, JSON.stringify(targetsConfig)], {
  stdio: "inherit",
});

console.log("\nExtension packages created successfully:");
console.log("  - conflux-browser-firefox.zip (for Mozilla Firefox Add-ons AMO)");
console.log("  - conflux-browser-chrome.zip  (for Google Chrome Web Store)");
console.log("  - conflux-browser-store.zip   (universal cross-browser store bundle)");

