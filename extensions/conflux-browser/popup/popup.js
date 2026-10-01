document.addEventListener("DOMContentLoaded", async () => {
  const statusBadge = document.getElementById("statusBadge");
  const statusDot = document.getElementById("statusDot");
  const statusText = document.getElementById("statusText");
  const interceptToggle = document.getElementById("interceptToggle");
  const openAppBtn = document.getElementById("openAppBtn");
  const optionsLink = document.getElementById("optionsLink");

  // Load current interception setting
  chrome.storage.sync.get({ interceptDownloads: false }, (data) => {
    interceptToggle.checked = !!data.interceptDownloads;
  });

  // Handle toggle change
  interceptToggle.addEventListener("change", () => {
    chrome.storage.sync.set({ interceptDownloads: interceptToggle.checked });
  });

  // Check connection to desktop host
  chrome.runtime.sendMessage({ action: "pingNativeHost" }, (res) => {
    if (chrome.runtime.lastError || !res || !res.connected) {
      statusDot.className = "dot error";
      statusText.textContent = "Disconnected";
      statusBadge.title = "Could not communicate with Conflux Desktop host.";
    } else {
      statusDot.className = "dot connected";
      statusText.textContent = "Connected";
      statusBadge.title = "Connected to Conflux Desktop Native Host.";
    }
  });

  // Open App Button
  openAppBtn.addEventListener("click", () => {
    chrome.runtime.sendMessage({ action: "openConflux" }, () => {
      // Fallback: try opening protocol URL directly
      window.location.href = "conflux://open";
    });
  });

  // Options Page Link
  optionsLink.addEventListener("click", () => {
    if (chrome.runtime.openOptionsPage) {
      chrome.runtime.openOptionsPage();
    } else {
      window.open(chrome.runtime.getURL("options/options.html"));
    }
  });
});
