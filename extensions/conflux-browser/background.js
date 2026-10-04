const NATIVE_HOST = "com.conflux.desktop";

const DEFAULT_SETTINGS = {
  interceptDownloads: false,
  minSizeMb: 0,
  fileExtensions: [
    ".zip", ".rar", ".7z", ".tar.gz", ".tgz", ".bz2", ".xz",
    ".iso", ".img", ".bin",
    ".exe", ".msi",
    ".mp4", ".mkv", ".mov", ".avi", ".flv",
    ".pdf", ".epub"
  ],
  excludedDomains: [
    "localhost",
    "127.0.0.1"
  ]
};

async function getSettings() {
  return new Promise((resolve) => {
    chrome.storage.sync.get(DEFAULT_SETTINGS, (stored) => {
      if (chrome.runtime.lastError || !stored) {
        chrome.storage.local.get(DEFAULT_SETTINGS, (localStored) => {
          resolve(localStored || DEFAULT_SETTINGS);
        });
      } else {
        resolve(stored);
      }
    });
  });
}

function showNotification(title, message) {
  if (chrome.notifications) {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/icon-128.png",
      title: title,
      message: message
    });
  }
}

function sendDownloadToConflux(payload) {
  return new Promise((resolve, reject) => {
    console.log("[Conflux] Sending download to desktop host:", payload.url);
    chrome.runtime.sendNativeMessage(NATIVE_HOST, payload, (response) => {
      if (chrome.runtime.lastError) {
        const err = chrome.runtime.lastError.message;
        console.warn("[Conflux] Native messaging error:", err);
        showNotification(
          "Conflux Download Manager",
          "Could not communicate with Conflux Desktop. Please ensure Conflux is installed and running."
        );
        reject(new Error(err));
      } else if (response && response.status === "error") {
        console.warn("[Conflux] Desktop returned error:", response.message);
        showNotification(
          "Conflux Download Manager",
          `Desktop rejected link: ${response.message || "Invalid URL"}`
        );
        reject(new Error(response.message || "Desktop rejected link"));
      } else {
        console.log("[Conflux] Native host responded successfully:", response);
        resolve(response);
      }
    });
  });
}

// Context Menu setup
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "conflux-download-link",
    title: "Download with Conflux",
    contexts: ["link", "video", "audio"]
  });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId === "conflux-download-link") {
    const targetUrl = info.linkUrl || info.srcUrl;
    console.log("[Conflux] Context menu clicked. Target URL:", targetUrl);
    if (!targetUrl) {
      showNotification("Conflux", "Please right-click directly on a download link or media file.");
      return;
    }

    const referer = info.pageUrl || "";
    let extractedName = null;
    try {
      const pathname = new URL(targetUrl).pathname;
      const leaf = pathname.split("/").pop();
      if (leaf && leaf.includes(".")) extractedName = leaf;
    } catch (_) {}

    const payload = {
      action: "download",
      url: targetUrl,
      referer: referer || null,
      user_agent: navigator.userAgent,
      filename: extractedName
    };

    try {
      await sendDownloadToConflux(payload);
    } catch (e) {
      console.error("[Conflux] Failed to forward download to Conflux:", e);
    }
  }
});

// Automatic download interception (opt-in)
chrome.downloads.onCreated.addListener(async (downloadItem) => {
  const settings = await getSettings();
  console.log("[Conflux] onCreated event:", downloadItem.url, "| Intercept enabled:", settings.interceptDownloads);
  if (!settings.interceptDownloads) {
    console.log("[Conflux] Automatic interception is disabled in settings. Skipping.");
    return;
  }

  const url = downloadItem.url || downloadItem.finalUrl;
  if (!url || url.startsWith("blob:") || url.startsWith("data:") || url.startsWith("about:")) {
    console.log("[Conflux] In-memory, data, or blob URL. Cannot download over network socket:", url);
    return;
  }

  // Check excluded domains
  try {
    const parsed = new URL(url);
    if (settings.excludedDomains.some((d) => parsed.hostname === d || parsed.hostname.endsWith("." + d))) {
      console.log("[Conflux] Excluded domain match:", parsed.hostname);
      return;
    }
  } catch {
    return;
  }

  // Match file extension against filename AND against URL pathname (stripping query parameters)
  let urlPath = "";
  try {
    const parsed = new URL(url);
    urlPath = parsed.pathname.toLowerCase();
  } catch {
    urlPath = url.toLowerCase();
  }

  const candidateName = (downloadItem.filename || "").toLowerCase();
  const matchedExt = settings.fileExtensions.some((ext) => {
    const e = ext.toLowerCase();
    return candidateName.endsWith(e) || urlPath.endsWith(e);
  });

  if (!matchedExt) {
    console.log("[Conflux] File extension does not match filters. Candidate:", candidateName, "urlPath:", urlPath);
    return;
  }

  // Check minimum file size threshold (skip files smaller than minSizeMb if size is known)
  if (settings.minSizeMb > 0 && downloadItem.totalBytes > 0) {
    const minBytes = settings.minSizeMb * 1024 * 1024;
    if (downloadItem.totalBytes < minBytes) {
      console.log(`[Conflux] File size ${downloadItem.totalBytes}B is below ${settings.minSizeMb}MB threshold. Skipping.`);
      return;
    }
  }

  const extractedName = downloadItem.filename
    ? downloadItem.filename.split(/[\\/]/).pop()
    : (urlPath.split("/").pop() || null);

  const payload = {
    action: "download",
    url: url,
    referer: downloadItem.referrer || null,
    user_agent: navigator.userAgent,
    filename: extractedName
  };

  try {
    await sendDownloadToConflux(payload);
    // Only drop the browser download once Conflux has accepted it
    chrome.downloads.cancel(downloadItem.id, () => {
      chrome.downloads.erase({ id: downloadItem.id });
    });
  } catch (e) {
    console.error("[Conflux] Failed to auto-intercept download to Conflux; leaving browser download running:", e);
  }
});

// Communication with popup & options pages
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.action === "pingNativeHost") {
    chrome.runtime.sendNativeMessage(NATIVE_HOST, { action: "ping" }, (response) => {
      if (chrome.runtime.lastError) {
        sendResponse({ connected: false, error: chrome.runtime.lastError.message });
      } else {
        sendResponse({ connected: true, response });
      }
    });
    return true; // Keep message channel open for async response
  }

  if (message.action === "openConflux") {
    chrome.runtime.sendNativeMessage(NATIVE_HOST, { action: "open" }, (response) => {
      sendResponse({ ok: !chrome.runtime.lastError, response });
    });
    return true;
  }
});
