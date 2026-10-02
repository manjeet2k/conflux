const NATIVE_HOST = "com.conflux.desktop";

const DEFAULT_SETTINGS = {
  interceptDownloads: false,
  minSizeMb: 10,
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

async function getCookiesForUrl(url) {
  try {
    const cookies = await chrome.cookies.getAll({ url });
    if (!cookies || cookies.length === 0) return null;
    return cookies.map((c) => `${c.name}=${c.value}`).join("; ");
  } catch (err) {
    console.debug("Failed to retrieve cookies for URL:", url, err);
    return null;
  }
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
    chrome.runtime.sendNativeMessage(NATIVE_HOST, payload, (response) => {
      if (chrome.runtime.lastError) {
        const err = chrome.runtime.lastError.message;
        console.warn("Conflux native messaging error:", err);
        showNotification(
          "Conflux Download Manager",
          "Could not communicate with Conflux Desktop. Please ensure Conflux is installed and running."
        );
        reject(new Error(err));
      } else {
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

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId === "conflux-download-link") {
    const targetUrl = info.linkUrl || info.srcUrl;
    if (!targetUrl) return;

    const referer = tab?.url || "";
    const cookies = await getCookiesForUrl(targetUrl);

    const payload = {
      version: 1,
      action: "download",
      url: targetUrl,
      referer: referer || null,
      user_agent: navigator.userAgent,
      cookies: cookies || null,
      filename: null,
      suggested_filename: null
    };

    try {
      await sendDownloadToConflux(payload);
    } catch (e) {
      console.error("Failed to forward download to Conflux:", e);
    }
  }
});

// Automatic download interception (opt-in)
chrome.downloads.onCreated.addListener(async (downloadItem) => {
  const settings = await getSettings();
  if (!settings.interceptDownloads) return;

  const url = downloadItem.url || downloadItem.finalUrl;
  if (!url || url.startsWith("blob:") || url.startsWith("data:") || url.startsWith("about:")) {
    return;
  }

  // Check excluded domains
  try {
    const parsed = new URL(url);
    if (settings.excludedDomains.some((d) => parsed.hostname === d || parsed.hostname.endsWith("." + d))) {
      return;
    }
  } catch {
    return;
  }

  // Match file extension
  const candidate = (downloadItem.filename || url).toLowerCase();
  const matchedExt = settings.fileExtensions.some((ext) => candidate.endsWith(ext.toLowerCase()));
  if (!matchedExt) {
    return;
  }

  // Check minimum file size threshold (skip files smaller than minSizeMb if size is known)
  if (settings.minSizeMb > 0 && downloadItem.totalBytes > 0) {
    const minBytes = settings.minSizeMb * 1024 * 1024;
    if (downloadItem.totalBytes < minBytes) {
      return;
    }
  }

  // Cancel and erase browser download
  chrome.downloads.cancel(downloadItem.id, () => {
    chrome.downloads.erase({ id: downloadItem.id });
  });

  const cookies = await getCookiesForUrl(url);
  const extractedName = downloadItem.filename ? downloadItem.filename.split(/[\\/]/).pop() : null;
  const payload = {
    version: 1,
    action: "download",
    url: url,
    referer: downloadItem.referrer || null,
    user_agent: navigator.userAgent,
    cookies: cookies || null,
    filename: extractedName,
    suggested_filename: extractedName,
    total_bytes: downloadItem.totalBytes > 0 ? downloadItem.totalBytes : null
  };

  try {
    await sendDownloadToConflux(payload);
  } catch (e) {
    console.error("Failed to auto-intercept download to Conflux:", e);
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
