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

document.addEventListener("DOMContentLoaded", () => {
  const interceptDownloads = document.getElementById("interceptDownloads");
  const fileExtensions = document.getElementById("fileExtensions");
  const excludedDomains = document.getElementById("excludedDomains");
  const saveBtn = document.getElementById("saveBtn");
  const resetBtn = document.getElementById("resetBtn");
  const saveStatus = document.getElementById("saveStatus");
  const testConnectionBtn = document.getElementById("testConnectionBtn");
  const testStatus = document.getElementById("testStatus");

  function populate(settings) {
    interceptDownloads.checked = !!settings.interceptDownloads;
    fileExtensions.value = (settings.fileExtensions || []).join(", ");
    excludedDomains.value = (settings.excludedDomains || []).join("\n");
  }

  // Load settings
  chrome.storage.sync.get(DEFAULT_SETTINGS, (data) => {
    populate(data);
  });

  // Save settings
  saveBtn.addEventListener("click", () => {
    const rawExts = fileExtensions.value
      .split(",")
      .map((s) => s.trim().toLowerCase())
      .filter((s) => s.length > 0)
      .map((s) => (s.startsWith(".") ? s : "." + s));

    const rawDomains = excludedDomains.value
      .split("\n")
      .map((s) => s.trim().toLowerCase())
      .filter((s) => s.length > 0);

    const updated = {
      interceptDownloads: interceptDownloads.checked,
      fileExtensions: rawExts,
      excludedDomains: rawDomains
    };

    chrome.storage.sync.set(updated, () => {
      saveStatus.textContent = "Settings saved successfully.";
      saveStatus.className = "status-msg success";
      setTimeout(() => {
        saveStatus.style.display = "none";
      }, 3000);
    });
  });

  // Reset settings
  resetBtn.addEventListener("click", () => {
    if (confirm("Restore all settings to default values?")) {
      chrome.storage.sync.set(DEFAULT_SETTINGS, () => {
        populate(DEFAULT_SETTINGS);
        saveStatus.textContent = "Default settings restored.";
        saveStatus.className = "status-msg success";
        setTimeout(() => {
          saveStatus.style.display = "none";
        }, 3000);
      });
    }
  });

  // Test connection
  testConnectionBtn.addEventListener("click", () => {
    testStatus.textContent = "Testing connection to Conflux Desktop...";
    testStatus.className = "status-msg";
    testStatus.style.display = "block";

    chrome.runtime.sendMessage({ action: "pingNativeHost" }, (res) => {
      if (chrome.runtime.lastError || !res || !res.connected) {
        const err = chrome.runtime.lastError ? chrome.runtime.lastError.message : (res?.error || "Unknown error");
        testStatus.textContent = `Connection failed: ${err}. Please ensure Conflux Desktop is installed.`;
        testStatus.className = "status-msg error";
      } else {
        testStatus.textContent = "Successfully connected to Conflux Desktop Native Host!";
        testStatus.className = "status-msg success";
      }
    });
  });
});
