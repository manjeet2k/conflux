# Chrome Web Store & Firefox Add-ons (AMO) Listing Guide

This document contains the official store listing copy, category settings, permissions justifications, and asset specifications for publishing the **Conflux Download Accelerator** browser extension.

---

## 1. Extension Metadata

* **Name:** Conflux Download Accelerator
* **Summary / Short Description (up to 132 chars):**
  Seamlessly hand off downloads from your browser to the multi-interface Conflux Download Accelerator for maximum speed.
* **Category:**
  - Chrome Web Store: **Productivity** / **Workflow & Planning**
  - Firefox AMO: **Download Management**
* **Primary Language:** English
* **Pricing:** Free / Open Source

---

## 2. Detailed Description (Store Listing Body)

```markdown
Conflux Download Accelerator companion extension for Google Chrome, Microsoft Edge, Brave, and Mozilla Firefox.

Conflux is a high-performance Windows download manager that aggregates bandwidth across all available network interfaces (Wi-Fi, Ethernet, and phone tethering) simultaneously.

This companion extension seamlessly bridges your web browser and the Conflux Desktop application, giving you instant one-click and automatic download capture.

KEY FEATURES

• One-Click Context Menu: Right-click any link, video stream, or audio file and choose "Download with Conflux" to immediately send it to the desktop accelerator.
• Request Context: Forwards the Referer and User-Agent to Conflux so servers that validate them accept the hand-off.
• Minimal Permissions: No access to website data, cookies, or your tabs.
• Automatic Download Interception (Opt-in): Intercepts browser downloads for designated file types (.zip, .iso, .exe, .tar.gz, .mp4, etc.) and routes them to Conflux.
• Filter Rules: Configure custom file extensions, minimum file size thresholds, and domain exclusions in the extension options.
• Fast Native Messaging: Communicates directly with the Conflux Desktop application using the OS-level Native Messaging API without background network servers or open listening ports.

PRIVACY FIRST

• 100% Local: All data is sent exclusively to the Conflux application installed on your PC. No data is sent to external servers or third parties.
• Zero Telemetry: No analytics, tracking, or advertisements.
• Open Source: Fully open source under MIT / Apache-2.0 licenses.

REQUIREMENTS

• Requires Conflux Desktop for Windows 10 / 11 installed on your computer.
• Download Conflux Desktop at: https://github.com/manjeet2k/conflux
```

---

## 3. Chrome Web Store Permissions Justification

When submitting to the Chrome Web Store Developer Dashboard, you will be asked to justify each requested permission:

1. **`downloads`**
   - *Justification:* Required to detect when the user starts downloading a file (`chrome.downloads.onCreated`), verify whether it matches the user's interception filters, and cancel the browser download so it can be accelerated by Conflux.
2. **`nativeMessaging`**
   - *Justification:* Required to communicate with the locally installed Conflux desktop application via the OS-level standard input/output pipe.
3. **`contextMenus`**
   - *Justification:* Required to provide the right-click "Download with Conflux" menu item on links, audio, and video elements.
4. **`storage`**
   - *Justification:* Required to save user preferences (interception toggle state, file extension filters, minimum file size threshold, and excluded domains) in synced extension storage.
5. **`notifications`**
   - *Justification:* Required to notify the user if communication with Conflux Desktop fails or if the desktop app needs to be launched.

*Host permissions: none requested. The extension does not read page content, cookies, or tab URLs.*

---

## 4. Privacy Policy URL

Submit this URL in the developer dashboard:
`https://manjeet2k.github.io/conflux/privacy.html`

Single-purpose statement:
*"The single purpose of this extension is to forward user-selected download links and their referrer and User-Agent from the browser to the locally installed Conflux Desktop download manager."*

---

## 5. Visual Asset Specifications

* **Store Icon:** 128x128 PNG (already available at `icons/icon-128.png`).
* **Promo Tile (Small):** 440x280 PNG (optional for CWS, recommended).
* **Screenshots:** At least one screenshot (1280x800 or 640x400 PNG) showing the extension popup and context menu in action.

---

## 6. Store Package Targets

Run `node scripts/package-extension.mjs` to generate store-compliant archives under `target/extension-dist/`:

* **`conflux-browser-firefox.zip`**: Optimized for Mozilla Firefox Add-ons (AMO). Manifest uses `background.scripts`, sets `strict_min_version: "142.0"`, and specifies `data_collection_permissions: { required: ["none"] }`. Passes AMO validator with 0 errors and 0 warnings.
* **`conflux-browser-chrome.zip`**: Optimized for Chrome Web Store and Microsoft Edge Add-ons. Manifest uses `background.service_worker` and strips Gecko-specific configuration.
* **`conflux-browser-store.zip`**: Universal cross-browser package with dual background declarations.

