import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";

const els = {
  listenAddress: document.getElementById("listenAddress"),
  port: document.getElementById("port"),
  browserPrintCompatible: document.getElementById("browserPrintCompatible"),
  httpStatus: document.getElementById("httpStatus"),
  saveHttpBtn: document.getElementById("saveHttpBtn"),
  savedList: document.getElementById("savedList"),
  searchBtn: document.getElementById("searchBtn"),
  searchResults: document.getElementById("searchResults"),
  manualName: document.getElementById("manualName"),
  manualAddress: document.getElementById("manualAddress"),
  manualPort: document.getElementById("manualPort"),
  addManualBtn: document.getElementById("addManualBtn"),
  launchAtLogin: document.getElementById("launchAtLogin"),
  apiUrl: document.getElementById("apiUrl"),
};

let currentConfig = null;
let lastDiscovered = [];
/** @type {Map<string, { kind: "checking" | "online" | "offline"; detail?: string }>} */
const printerStatuses = new Map();
let statusRefreshSeq = 0;

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

function defaultAddress() {
  return currentConfig?.defaultPrinter?.address ?? null;
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

function statusBadgeHtml(address) {
  const entry = printerStatuses.get(address);
  const kind = entry?.kind ?? "checking";
  const detail = entry?.detail ? escapeHtml(entry.detail) : "";
  if (kind === "online") {
    return `<span class="badge ok status-badge" data-status-address="${escapeHtml(address)}" role="button" tabindex="0" title="Click to recheck">Online</span>`;
  }
  if (kind === "offline") {
    return `<span class="badge bad status-badge" data-status-address="${escapeHtml(address)}" role="button" tabindex="0" title="${detail || "Click to recheck"}">Offline</span>`;
  }
  return `<span class="badge neutral status-badge" data-status-address="${escapeHtml(address)}" role="button" tabindex="0" title="Checking…">…</span>`;
}

function updateStatusBadgeInDom(address) {
  const el = [...els.savedList.querySelectorAll(".status-badge")].find(
    (badge) => badge.dataset.statusAddress === address
  );
  if (!el) return;
  const entry = printerStatuses.get(address);
  const kind = entry?.kind ?? "checking";
  el.classList.remove("ok", "bad", "neutral");
  if (kind === "online") {
    el.classList.add("ok");
    el.textContent = "Online";
    el.title = "Click to recheck";
  } else if (kind === "offline") {
    el.classList.add("bad");
    el.textContent = "Offline";
    el.title = entry?.detail || "Click to recheck";
  } else {
    el.classList.add("neutral");
    el.textContent = "…";
    el.title = "Checking…";
  }
}

function findSavedPrinter(address) {
  return (currentConfig?.addedPrinters || []).find((p) => p.address === address);
}

async function checkPrinterStatus(printer) {
  const status = await invoke("check_printer_status", { printer });
  if (status.status === "online") {
    return {
      kind: "online",
      detail: status.detail || undefined,
    };
  }
  return {
    kind: "offline",
    detail:
      status.detail ||
      status.errorMessages?.join(", ") ||
      "unreachable",
  };
}

async function refreshPrinterStatus(address) {
  const printer = findSavedPrinter(address);
  if (!printer) return;
  printerStatuses.set(address, { kind: "checking" });
  updateStatusBadgeInDom(address);
  try {
    const result = await checkPrinterStatus(printer);
    printerStatuses.set(address, result);
  } catch (e) {
    printerStatuses.set(address, {
      kind: "offline",
      detail: String(e),
    });
  }
  updateStatusBadgeInDom(address);
}

async function refreshAllPrinterStatuses() {
  const printers = currentConfig?.addedPrinters || [];
  if (!printers.length) return;
  const seq = ++statusRefreshSeq;
  for (const p of printers) {
    printerStatuses.set(p.address, { kind: "checking" });
    updateStatusBadgeInDom(p.address);
  }
  await Promise.all(
    printers.map(async (printer) => {
      try {
        const result = await checkPrinterStatus(printer);
        if (seq !== statusRefreshSeq) return;
        printerStatuses.set(printer.address, result);
      } catch (e) {
        if (seq !== statusRefreshSeq) return;
        printerStatuses.set(printer.address, {
          kind: "offline",
          detail: String(e),
        });
      }
      if (seq === statusRefreshSeq) {
        updateStatusBadgeInDom(printer.address);
      }
    })
  );
}

function bindStatusBadgeClicks(card) {
  card.querySelectorAll(".status-badge").forEach((badge) => {
    const run = (e) => {
      e.preventDefault();
      const address = badge.dataset.statusAddress;
      if (address) refreshPrinterStatus(address);
    };
    badge.addEventListener("click", run);
    badge.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") run(e);
    });
  });
}

function renderSaved(list) {
  if (!list?.length) {
    els.savedList.innerHTML = `<div class="printer-card muted">No saved printers yet. Search on LAN or add manually.</div>`;
    return;
  }

  const def = defaultAddress();
  els.savedList.innerHTML = list
    .map((p) => {
      const isDefault = p.address === def;
      const nameValue = p.name?.trim() ? p.name : "";
      return `
      <div class="saved-card" data-address="${escapeHtml(p.address)}">
        <div class="saved-header">
          <strong>${escapeHtml(displayName(p))}</strong>
          ${isDefault ? `<span class="badge ok">Default</span>` : ""}
          ${statusBadgeHtml(p.address)}
          ${p.model && p.model !== "Manual" ? `<span class="badge neutral">${escapeHtml(p.model)}</span>` : ""}
        </div>
        <div class="row">
          <label>Name</label>
          <input type="text" data-field="name" value="${escapeHtml(nameValue)}" placeholder="${escapeHtml(p.model || p.address)}" />
        </div>
        <div class="row">
          <label>Address</label>
          <input type="text" data-field="address" value="${escapeHtml(p.address)}" />
        </div>
        <div class="row">
          <label>Print port</label>
          <input type="number" data-field="printPort" min="1" max="65535" value="${escapeHtml(p.printPort)}" />
        </div>
        <div class="button-row">
          <button type="button" data-action="save">Save changes</button>
          ${isDefault ? "" : `<button type="button" class="primary" data-action="default">Set as default</button>`}
          <button type="button" data-action="remove">Remove</button>
        </div>
      </div>`;
    })
    .join("");

  els.savedList.querySelectorAll(".saved-card").forEach((card) => {
    const originalAddress = card.dataset.address;
    bindStatusBadgeClicks(card);

    card.querySelector('[data-action="save"]').addEventListener("click", async (e) => {
      const btn = e.currentTarget;
      btn.disabled = true;
      try {
        currentConfig = await invoke("update_printer", {
          originalAddress,
          name: card.querySelector('[data-field="name"]').value.trim(),
          address: card.querySelector('[data-field="address"]').value.trim(),
          printPort: Number(card.querySelector('[data-field="printPort"]').value) || 9100,
        });
        applyConfig(currentConfig);
        await refreshAllPrinterStatuses();
      } catch (err) {
        alert(String(err));
      } finally {
        btn.disabled = false;
      }
    });

    const defaultBtn = card.querySelector('[data-action="default"]');
    if (defaultBtn) {
      defaultBtn.addEventListener("click", async () => {
        const printer = findSavedPrinter(originalAddress);
        if (!printer) return;
        currentConfig = await invoke("set_default_printer", { printer });
        applyConfig(currentConfig);
      });
    }

    card.querySelector('[data-action="remove"]').addEventListener("click", async (e) => {
      const btn = e.currentTarget;
      if (btn.dataset.confirm !== "1") {
        btn.dataset.confirm = "1";
        btn.dataset.originalLabel = btn.textContent;
        btn.textContent = "Confirm?";
        btn.classList.add("danger");
        const reset = () => {
          if (btn.dataset.confirm !== "1") return;
          btn.dataset.confirm = "0";
          btn.textContent = btn.dataset.originalLabel || "Remove";
          btn.classList.remove("danger");
        };
        setTimeout(reset, 4000);
        return;
      }
      btn.disabled = true;
      try {
        currentConfig = await invoke("remove_added_printer", { address: originalAddress });
        printerStatuses.delete(originalAddress);
        applyConfig(currentConfig);
      } catch (err) {
        btn.disabled = false;
        btn.dataset.confirm = "0";
        btn.textContent = btn.dataset.originalLabel || "Remove";
        btn.classList.remove("danger");
        console.error(err);
      }
    });
  });
}

function applyConfig(config) {
  currentConfig = config;
  els.listenAddress.value = config.listenAddress ?? "127.0.0.1";
  els.port.value = config.port ?? 9100;
  els.launchAtLogin.checked = !!config.launchAtLogin;
  els.browserPrintCompatible.checked = config.browserPrintCompatible !== false;
  renderSaved(config.addedPrinters || []);
  updateApiUrl();
}

async function persistSettings() {
  return invoke("save_settings", {
    listenAddress: els.listenAddress.value.trim() || "127.0.0.1",
    port: Number(els.port.value) || 9100,
    launchAtLogin: els.launchAtLogin.checked,
    browserPrintCompatible: els.browserPrintCompatible.checked,
  });
}

async function load() {
  const config = await invoke("get_config");
  applyConfig(config);
  await refreshHttpStatus();
  await refreshAllPrinterStatuses();
}

els.saveHttpBtn.addEventListener("click", async () => {
  els.saveHttpBtn.disabled = true;
  try {
    currentConfig = await persistSettings();
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
    currentConfig = await persistSettings();
  } catch (e) {
    console.error(e);
  }
});

els.browserPrintCompatible.addEventListener("change", async () => {
  try {
    currentConfig = await persistSettings();
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
          <button type="button" class="primary" data-save-idx="${idx}">Save &amp; set default</button>
        </div>
        <div class="conn-status" data-status-idx="${idx}"></div>
      </div>`
      )
      .join("");

    els.searchResults.querySelectorAll("[data-save-idx]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const printer = lastDiscovered[Number(btn.dataset.saveIdx)];
        currentConfig = await invoke("set_default_printer", { printer });
        applyConfig(currentConfig);
        els.searchResults.classList.add("hidden");
        await refreshAllPrinterStatuses();
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
  const detail = status.detail ? ` ${escapeHtml(status.detail)}` : "";
  if (status.errorMessages?.length) {
    return `<div style="margin-top:6px"><span class="badge bad">Offline</span> ${escapeHtml(status.errorMessages.join(", "))}${detail ? `<div class="muted" style="margin-top:4px">${detail}</div>` : ""}</div>`;
  }
  if (status.status === "online") {
    const warn = status.warningMessages?.length
      ? ` <span class="badge warn">${escapeHtml(status.warningMessages.join(", "))}</span>`
      : "";
    return `<div style="margin-top:6px"><span class="badge ok">Online</span>${warn}</div>`;
  }
  return `<div style="margin-top:6px"><span class="badge bad">Offline</span>${detail ? `<div class="muted" style="margin-top:4px">${detail}</div>` : ""}</div>`;
}

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
    await refreshAllPrinterStatuses();
  } catch (e) {
    alert(String(e));
  } finally {
    els.addManualBtn.disabled = false;
  }
});

els.port.addEventListener("input", updateApiUrl);

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    const name = tab.dataset.tab;
    document.querySelectorAll(".tab").forEach((t) => {
      const active = t.dataset.tab === name;
      t.classList.toggle("active", active);
      t.setAttribute("aria-selected", active ? "true" : "false");
    });
    document.querySelectorAll(".tab-panel").forEach((panel) => {
      const active = panel.dataset.panel === name;
      panel.classList.toggle("active", active);
      panel.hidden = !active;
    });
  });
});

getCurrentWindow()
  .onFocusChanged(({ payload: focused }) => {
    if (focused) refreshAllPrinterStatuses();
  })
  .catch((e) => console.error(e));

load().catch((e) => {
  els.httpStatus.textContent = `Failed to load: ${e}`;
});
