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
• Session & Auth Preservation: Automatically forwards session cookies, Referer, and User-Agent headers to Conflux so authenticated downloads (Google Drive, cloud drives, member portals) succeed without 403 Forbidden or 401 Unauthorized errors.
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
3. **`cookies` & `<all_urls>` (host_permissions)**
   - *Justification:* Required to retrieve session cookies for the specific download URL (`chrome.cookies.getAll`) so protected or authenticated downloads (e.g. cloud storage services) do not fail with HTTP 403 Forbidden errors when handed off to Conflux.
4. **`contextMenus`**
   - *Justification:* Required to provide the right-click "Download with Conflux" menu item on links, audio, and video elements.
5. **`storage`**
   - *Justification:* Required to save user preferences (interception toggle state, file extension filters, minimum file size threshold, and excluded domains) in synced extension storage.
6. **`notifications`**
   - *Justification:* Required to notify the user if communication with Conflux Desktop fails or if the desktop app needs to be launched.

---

## 4. Privacy Policy URL

Submit this URL in the developer dashboard:
`https://manjeet2k.github.io/conflux/privacy.html`

Single-purpose statement:
*"The single purpose of this extension is to forward user-selected download links and associated session metadata from the browser to the locally installed Conflux Desktop download manager."*

---

## 5. Visual Asset Specifications

* **Store Icon:** 128x128 PNG (already available at `icons/icon-128.png`).
* **Promo Tile (Small):** 440x280 PNG (optional for CWS, recommended).
* **Screenshots:** At least one screenshot (1280x800 or 640x400 PNG) showing the extension popup and context menu in action.
