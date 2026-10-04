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
    let closed = false;
    const closePopup = () => {
      if (!closed) {
        closed = true;
        window.close();
      }
    };

    // Route protocol navigation through the active tab so Chrome displays the
    // confirmation prompt in the center of the browser window (under the Omnibox),
    // rather than anchoring to the extension icon in the toolbar where it overflows.
    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      const activeTab = tabs && tabs[0];
      if (activeTab && activeTab.id) {
        chrome.tabs.update(activeTab.id, { url: "conflux://open" }, () => {
          if (chrome.runtime.lastError) {
            console.warn(
              "[Conflux] tabs.update error, falling back to tabs.create:",
              chrome.runtime.lastError.message
            );
            chrome.tabs.create({ url: "conflux://open" }, () => {
              closePopup();
            });
          } else {
            closePopup();
          }
        });
      } else {
        chrome.tabs.create({ url: "conflux://open" }, () => {
          closePopup();
        });
      }
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
