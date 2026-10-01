// Refreshes the download links from GitHub's public API, so the page points at the newest
// release even before the site is redeployed. The links baked in at deploy time
// (scripts/build-site.mjs) stay in place if this fails (offline, rate limit, API change).
"use strict";

(async () => {
  const repo = "manjeet2k/conflux";
  try {
    const res = await fetch(`https://api.github.com/repos/${repo}/releases?per_page=10`, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) return;
    const releases = await res.json();
    // Newest published release with a version tag (betas included; the fixed `updater-beta`
    // release only holds the update manifest and is skipped).
    const rel = releases.find((r) => !r.draft && /^v\d+\.\d+\.\d+/.test(r.tag_name));
    if (!rel) return;
    const version = rel.tag_name.slice(1);
    const asset = (suffix) =>
      rel.assets.find((a) => a.name === `Conflux_${version}_${suffix}`);
    const setup = asset("x64-setup.exe");
    if (!setup) return; // never point the button at a release without an installer

    const set = (key, fn) => document.querySelectorAll(`[data-dl="${key}"]`).forEach(fn);
    set("setup", (el) => (el.href = setup.browser_download_url));
    const offline = asset("x64-offline-setup.exe");
    if (offline) set("offline", (el) => (el.href = offline.browser_download_url));
    const sums = rel.assets.find((a) => a.name === "SHA256SUMS.txt");
    if (sums) set("sums", (el) => (el.href = sums.browser_download_url));
    set("notes", (el) => (el.href = rel.html_url));
    set("version", (el) => (el.textContent = `${rel.prerelease || version.includes("-") ? "Beta" : "Version"} ${version}`));
    set("version-short", (el) => (el.textContent = version));
    set("size", (el) => (el.textContent = `${(setup.size / 1048576).toFixed(1)} MB`));
  } catch {
    // Keep the baked-in links.
  }
})();
