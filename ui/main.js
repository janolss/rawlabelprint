import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

const els = {
  listenAddress: document.getElementById("listenAddress"),
  port: document.getElementById("port"),
  httpStatus: document.getElementById("httpStatus"),
  saveHttpBtn: document.getElementById("saveHttpBtn"),
  defaultPrinter: document.getElementById("defaultPrinter"),
  searchBtn: document.getElementById("searchBtn"),
  testStatusBtn: document.getElementById("testStatusBtn"),
  testPrintBtn: document.getElementById("testPrintBtn"),
  searchResults: document.getElementById("searchResults"),
  manualName: document.getElementById("manualName"),
  manualAddress: document.getElementById("manualAddress"),
  manualPort: document.getElementById("manualPort"),
  addManualBtn: document.getElementById("addManualBtn"),
  addedList: document.getElementById("addedList"),
  launchAtLogin: document.getElementById("launchAtLogin"),
  apiUrl: document.getElementById("apiUrl"),
};

let currentConfig = null;
let lastDiscovered = [];

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function displayName(p) {
  if (p.name && p.name.trim()) return p.name;
  if (p.model) return `${p.model} (${p.address})`;
  return p.address;
}

function updateApiUrl() {
  const host = els.listenAddress.value.trim() || "127.0.0.1";
  const port = els.port.value || "9100";
  els.apiUrl.textContent = `http://${host}:${port}/`;
}

async function refreshHttpStatus() {
  try {
    const status = await invoke("get_http_status");
    els.httpStatus.textContent = `Status: ${status}`;
  } catch (e) {
    els.httpStatus.textContent = `Status: error (${e})`;
  }
}

function renderDefaultPrinter(printer, statusHtml = "") {
  if (!printer) {
    els.defaultPrinter.classList.add("muted");
    els.defaultPrinter.textContent = "No default printer selected";
    return;
  }
  els.defaultPrinter.classList.remove("muted");
  els.defaultPrinter.innerHTML = `
    <strong>${escapeHtml(displayName(printer))}</strong><br />
    Model: ${escapeHtml(printer.model)}<br />
    IP: ${escapeHtml(printer.address)}<br />
    Print port: ${escapeHtml(printer.printPort)}
    ${statusHtml}
  `;
}

function renderAdded(list) {
  if (!list?.length) {
    els.addedList.innerHTML = "";
    return;
  }
  els.addedList.innerHTML = list
    .map(
      (p) => `
      <div class="added-item">
        <span>${escapeHtml(displayName(p))} — ${escapeHtml(p.address)}:${escapeHtml(p.printPort)}</span>
        <button type="button" data-remove="${escapeHtml(p.address)}">Remove</button>
      </div>`
    )
    .join("");

  els.addedList.querySelectorAll("[data-remove]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      currentConfig = await invoke("remove_added_printer", { address: btn.dataset.remove });
      applyConfig(currentConfig);
    });
  });
}

function applyConfig(config) {
  currentConfig = config;
  els.listenAddress.value = config.listenAddress ?? "127.0.0.1";
  els.port.value = config.port ?? 9100;
  els.launchAtLogin.checked = !!config.launchAtLogin;
  renderDefaultPrinter(config.defaultPrinter);
  renderAdded(config.addedPrinters || []);
  updateApiUrl();
}

async function load() {
  const config = await invoke("get_config");
  applyConfig(config);
  await refreshHttpStatus();
}

els.saveHttpBtn.addEventListener("click", async () => {
  els.saveHttpBtn.disabled = true;
  try {
    currentConfig = await invoke("save_settings", {
      listenAddress: els.listenAddress.value.trim() || "127.0.0.1",
      port: Number(els.port.value) || 9100,
      launchAtLogin: els.launchAtLogin.checked,
    });
    applyConfig(currentConfig);
    await refreshHttpStatus();
  } catch (e) {
    els.httpStatus.textContent = `Status: error (${e})`;
  } finally {
    els.saveHttpBtn.disabled = false;
  }
});

els.launchAtLogin.addEventListener("change", async () => {
  try {
    currentConfig = await invoke("save_settings", {
      listenAddress: els.listenAddress.value.trim() || "127.0.0.1",
      port: Number(els.port.value) || 9100,
      launchAtLogin: els.launchAtLogin.checked,
    });
  } catch (e) {
    console.error(e);
  }
});

els.searchBtn.addEventListener("click", async () => {
  els.searchBtn.disabled = true;
  els.searchResults.classList.remove("hidden");
  els.searchResults.innerHTML = `<div class="printer-card muted">Searching on UDP 4201 (≈5s)…</div>`;
  try {
    lastDiscovered = await invoke("discover_printers");
    if (!lastDiscovered.length) {
      els.searchResults.innerHTML = `<div class="printer-card muted">No printers found</div>`;
      return;
    }
    els.searchResults.innerHTML = lastDiscovered
      .map(
        (p, idx) => `
      <div class="result-card">
        <h3>
          <a href="#" data-config-url="http://${escapeHtml(p.address)}:${escapeHtml(p.configPort)}">
            ${escapeHtml(p.model || p.address)}
          </a>
        </h3>
        <p class="result-meta">
          IP: ${escapeHtml(p.address)}<br />
          Firmware: ${escapeHtml(p.firmware || "—")}<br />
          Serial: ${escapeHtml(p.serialNumber || "—")}<br />
          Print port: ${escapeHtml(p.printPort)}
        </p>
        <div class="button-row">
          <button type="button" data-test-idx="${idx}">Test connection</button>
          <button type="button" class="primary" data-set-idx="${idx}">Set as default</button>
        </div>
        <div class="conn-status" data-status-idx="${idx}"></div>
      </div>`
      )
      .join("");

    els.searchResults.querySelectorAll("[data-set-idx]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const printer = lastDiscovered[Number(btn.dataset.setIdx)];
        currentConfig = await invoke("set_default_printer", { printer });
        applyConfig(currentConfig);
        els.searchResults.classList.add("hidden");
      });
    });

    els.searchResults.querySelectorAll("[data-test-idx]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const idx = Number(btn.dataset.testIdx);
        const printer = lastDiscovered[idx];
        const statusEl = els.searchResults.querySelector(`[data-status-idx="${idx}"]`);
        btn.disabled = true;
        statusEl.innerHTML = `<span class="badge neutral">Testing…</span>`;
        try {
          const status = await invoke("check_printer_status", { printer });
          statusEl.innerHTML = statusBadge(status);
        } catch (e) {
          statusEl.innerHTML = `<span class="badge bad">${escapeHtml(String(e))}</span>`;
        } finally {
          btn.disabled = false;
        }
      });
    });

    els.searchResults.querySelectorAll("[data-config-url]").forEach((link) => {
      link.addEventListener("click", async (e) => {
        e.preventDefault();
        try {
          await openUrl(link.dataset.configUrl);
        } catch (err) {
          console.error(err);
        }
      });
    });
  } catch (e) {
    els.searchResults.innerHTML = `<div class="printer-card"><span class="badge bad">${escapeHtml(String(e))}</span></div>`;
  } finally {
    els.searchBtn.disabled = false;
  }
});

function statusBadge(status) {
  if (!status) return "";
  if (status.errorMessages?.length) {
    return `<div style="margin-top:6px"><span class="badge bad">Offline</span> ${escapeHtml(status.errorMessages.join(", "))}</div>`;
  }
  if (status.status === "online") {
    const warn = status.warningMessages?.length
      ? ` <span class="badge warn">${escapeHtml(status.warningMessages.join(", "))}</span>`
      : "";
    return `<div style="margin-top:6px"><span class="badge ok">Online</span>${warn}</div>`;
  }
  return `<div style="margin-top:6px"><span class="badge bad">Offline</span></div>`;
}

els.testStatusBtn.addEventListener("click", async () => {
  if (!currentConfig?.defaultPrinter) return;
  els.testStatusBtn.disabled = true;
  renderDefaultPrinter(
    currentConfig.defaultPrinter,
    `<div style="margin-top:6px"><span class="badge neutral">Testing…</span></div>`
  );
  try {
    const status = await invoke("check_printer_status", {
      printer: currentConfig.defaultPrinter,
    });
    renderDefaultPrinter(currentConfig.defaultPrinter, statusBadge(status));
  } catch (e) {
    renderDefaultPrinter(
      currentConfig.defaultPrinter,
      `<div style="margin-top:6px"><span class="badge bad">${escapeHtml(String(e))}</span></div>`
    );
  } finally {
    els.testStatusBtn.disabled = false;
  }
});

els.testPrintBtn.addEventListener("click", async () => {
  if (!currentConfig?.defaultPrinter) {
    alert("Select a default printer first.");
    return;
  }
  els.testPrintBtn.disabled = true;
  try {
    await invoke("test_print", { printer: currentConfig.defaultPrinter });
    alert("Test label sent.");
  } catch (e) {
    alert(`Print failed: ${e}`);
  } finally {
    els.testPrintBtn.disabled = false;
  }
});

els.addManualBtn.addEventListener("click", async () => {
  const address = els.manualAddress.value.trim();
  if (!address) {
    alert("Address is required.");
    return;
  }
  els.addManualBtn.disabled = true;
  try {
    currentConfig = await invoke("add_manual_printer", {
      name: els.manualName.value.trim(),
      address,
      printPort: Number(els.manualPort.value) || 9100,
    });
    applyConfig(currentConfig);
    els.manualName.value = "";
    els.manualAddress.value = "";
    els.manualPort.value = 9100;
  } catch (e) {
    alert(String(e));
  } finally {
    els.addManualBtn.disabled = false;
  }
});

els.listenAddress.addEventListener("input", updateApiUrl);
els.port.addEventListener("input", updateApiUrl);

load().catch((e) => {
  els.httpStatus.textContent = `Failed to load: ${e}`;
});
