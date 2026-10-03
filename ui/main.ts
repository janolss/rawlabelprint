import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { buildPrintLogHtml, escapeHtml } from "./print-log-view.ts";
import {
  applyStatusBadgeDom,
  buildSavedPrintersHtml,
} from "./saved-printers-view.ts";
import {
  buildNoPrintersHtml,
  buildSearchErrorHtml,
  buildSearchResultsHtml,
  buildSearchingHtml,
  connectionStatusBadgeHtml,
} from "./search-results-view.ts";
import type {
  AppConfig,
  OriginPermissions,
  PrinterInfo,
  PrinterStatus,
  PrintLogEntry,
  UiPrinterStatus,
} from "./types.ts";

function requireEl<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`Missing element #${id}`);
  return el as T;
}

const els = {
  listenAddress: requireEl<HTMLInputElement>("listenAddress"),
  port: requireEl<HTMLInputElement>("port"),
  browserPrintCompatible: requireEl<HTMLInputElement>("browserPrintCompatible"),
  debugLogging: requireEl<HTMLInputElement>("debugLogging"),
  httpStatus: requireEl<HTMLElement>("httpStatus"),
  saveHttpBtn: requireEl<HTMLButtonElement>("saveHttpBtn"),
  savedList: requireEl<HTMLElement>("savedList"),
  searchBtn: requireEl<HTMLButtonElement>("searchBtn"),
  searchResults: requireEl<HTMLElement>("searchResults"),
  manualName: requireEl<HTMLInputElement>("manualName"),
  manualAddress: requireEl<HTMLInputElement>("manualAddress"),
  manualPort: requireEl<HTMLInputElement>("manualPort"),
  addManualBtn: requireEl<HTMLButtonElement>("addManualBtn"),
  launchAtLogin: requireEl<HTMLInputElement>("launchAtLogin"),
  apiUrl: requireEl<HTMLElement>("apiUrl"),
  printLogList: requireEl<HTMLElement>("printLogList"),
  refreshPrintLogBtn: requireEl<HTMLButtonElement>("refreshPrintLogBtn"),
  clearPrintLogBtn: requireEl<HTMLButtonElement>("clearPrintLogBtn"),
  originPendingList: requireEl<HTMLElement>("originPendingList"),
  originAllowedList: requireEl<HTMLElement>("originAllowedList"),
};

let currentConfig: AppConfig | null = null;
let lastDiscovered: PrinterInfo[] = [];
const printerStatuses = new Map<string, UiPrinterStatus>();
let statusRefreshSeq = 0;

function defaultAddress(): string | null {
  return currentConfig?.defaultPrinter?.address ?? null;
}

function updateApiUrl(): void {
  const host = els.listenAddress.value.trim() || "127.0.0.1";
  const port = els.port.value || "9100";
  els.apiUrl.textContent = `http://${host}:${port}/`;
}

async function refreshHttpStatus(): Promise<void> {
  try {
    const status = await invoke<string>("get_http_status");
    els.httpStatus.textContent = `Status: ${status}`;
  } catch (e) {
    els.httpStatus.textContent = `Status: error (${e})`;
  }
}

function updateStatusBadgeInDom(address: string): void {
  const el = [...els.savedList.querySelectorAll<HTMLElement>(".status-badge")].find(
    (badge) => badge.dataset.statusAddress === address
  );
  if (!el) return;
  applyStatusBadgeDom(el, printerStatuses.get(address));
}

function findSavedPrinter(address: string | undefined): PrinterInfo | undefined {
  if (!address) return undefined;
  return (currentConfig?.addedPrinters || []).find((p) => p.address === address);
}

async function checkPrinterStatus(printer: PrinterInfo): Promise<UiPrinterStatus> {
  const status = await invoke<PrinterStatus>("check_printer_status", { printer });
  if (status.status === "online") {
    return {
      kind: "online",
      detail: status.detail || undefined,
    };
  }
  return {
    kind: "offline",
    detail: status.detail || status.errorMessages?.join(", ") || "unreachable",
  };
}

async function refreshPrinterStatus(address: string): Promise<void> {
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

async function refreshAllPrinterStatuses(): Promise<void> {
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

function bindStatusBadgeClicks(card: Element): void {
  card.querySelectorAll<HTMLElement>(".status-badge").forEach((badge) => {
    const run = (e: Event) => {
      e.preventDefault();
      const address = badge.dataset.statusAddress;
      if (address) void refreshPrinterStatus(address);
    };
    badge.addEventListener("click", run);
    badge.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") run(e);
    });
  });
}

function fieldInput(card: Element, field: string): HTMLInputElement {
  const input = card.querySelector<HTMLInputElement>(`[data-field="${field}"]`);
  if (!input) throw new Error(`Missing field ${field}`);
  return input;
}

function renderSaved(list: PrinterInfo[] | null | undefined): void {
  els.savedList.innerHTML = buildSavedPrintersHtml(
    list,
    defaultAddress(),
    printerStatuses
  );

  els.savedList.querySelectorAll<HTMLElement>(".saved-card").forEach((card) => {
    const originalAddress = card.dataset.address;
    bindStatusBadgeClicks(card);

    const saveBtn = card.querySelector<HTMLButtonElement>('[data-action="save"]');
    saveBtn?.addEventListener("click", async (e) => {
      const btn = e.currentTarget as HTMLButtonElement;
      btn.disabled = true;
      try {
        currentConfig = await invoke<AppConfig>("update_printer", {
          originalAddress,
          name: fieldInput(card, "name").value.trim(),
          address: fieldInput(card, "address").value.trim(),
          printPort: (() => {
            const portEl = card.querySelector<HTMLInputElement>('[data-field="printPort"]');
            if (!portEl) return 0;
            return Number(portEl.value) || 9100;
          })(),
        });
        applyConfig(currentConfig);
        await refreshAllPrinterStatuses();
      } catch (err) {
        alert(String(err));
      } finally {
        btn.disabled = false;
      }
    });

    const defaultBtn = card.querySelector<HTMLButtonElement>('[data-action="default"]');
    if (defaultBtn) {
      defaultBtn.addEventListener("click", async () => {
        const printer = findSavedPrinter(originalAddress);
        if (!printer) return;
        currentConfig = await invoke<AppConfig>("set_default_printer", { printer });
        applyConfig(currentConfig);
      });
    }

    const removeBtn = card.querySelector<HTMLButtonElement>('[data-action="remove"]');
    removeBtn?.addEventListener("click", async (e) => {
      const btn = e.currentTarget as HTMLButtonElement;
      if (btn.dataset.confirm !== "1") {
        btn.dataset.confirm = "1";
        btn.dataset.originalLabel = btn.textContent ?? "Remove";
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
        currentConfig = await invoke<AppConfig>("remove_added_printer", {
          address: originalAddress,
        });
        if (originalAddress) printerStatuses.delete(originalAddress);
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

function applyConfig(config: AppConfig): void {
  currentConfig = config;
  els.listenAddress.value = "127.0.0.1";
  els.port.value = String(config.port ?? 9100);
  els.launchAtLogin.checked = !!config.launchAtLogin;
  // Dev (`tauri dev`) must not register LaunchAgent against target/debug.
  if (import.meta.env.DEV) {
    els.launchAtLogin.disabled = true;
    els.launchAtLogin.title = "Disabled while running via tauri dev";
  } else {
    els.launchAtLogin.disabled = false;
    els.launchAtLogin.removeAttribute("title");
  }
  els.browserPrintCompatible.checked = config.browserPrintCompatible !== false;
  els.debugLogging.checked = config.debugLogging !== false;
  renderSaved(config.addedPrinters || []);
  updateApiUrl();
}

async function persistSettings(): Promise<AppConfig> {
  return invoke<AppConfig>("save_settings", {
    listenAddress: "127.0.0.1",
    port: Number(els.port.value) || 9100,
    launchAtLogin: els.launchAtLogin.checked,
    browserPrintCompatible: els.browserPrintCompatible.checked,
    debugLogging: els.debugLogging.checked,
  });
}

function renderPrintLog(entries: PrintLogEntry[]): void {
  els.printLogList.innerHTML = buildPrintLogHtml(entries);
}

function renderOriginPermissions(perms: OriginPermissions): void {
  if (!perms.pending.length) {
    els.originPendingList.innerHTML = "";
  } else {
    els.originPendingList.innerHTML = perms.pending
      .map(
        (origin) => `
      <div class="origin-row" data-origin="${escapeHtml(origin)}">
        <span class="badge warn">Pending</span>
        <span class="origin-url" title="${escapeHtml(origin)}">${escapeHtml(origin)}</span>
        <div class="button-row inline">
          <button type="button" data-origin-action="approve">Allow</button>
          <button type="button" data-origin-action="deny">Deny</button>
        </div>
      </div>`
      )
      .join("");
  }

  if (!perms.allowed.length) {
    els.originAllowedList.innerHTML =
      '<div class="printer-card muted">No websites approved yet</div>';
  } else {
    els.originAllowedList.innerHTML = perms.allowed
      .map(
        (origin) => `
      <div class="origin-row" data-origin="${escapeHtml(origin)}">
        <span class="badge ok">Allowed</span>
        <span class="origin-url" title="${escapeHtml(origin)}">${escapeHtml(origin)}</span>
        <div class="button-row inline">
          <button type="button" class="danger" data-origin-action="revoke">Remove</button>
        </div>
      </div>`
      )
      .join("");
  }
}

async function refreshOriginPermissions(): Promise<void> {
  try {
    const perms = await invoke<OriginPermissions>("get_origin_permissions");
    renderOriginPermissions(perms);
  } catch (e) {
    els.originPendingList.innerHTML = "";
    els.originAllowedList.innerHTML = `<div class="printer-card"><span class="badge bad">${escapeHtml(String(e))}</span></div>`;
  }
}

async function refreshPrintLog(): Promise<void> {
  try {
    const entries = await invoke<PrintLogEntry[]>("get_print_log");
    renderPrintLog(entries);
  } catch (e) {
    els.printLogList.innerHTML = `<div class="printer-card"><span class="badge bad">${escapeHtml(String(e))}</span></div>`;
  }
}

async function load(): Promise<void> {
  const config = await invoke<AppConfig>("get_config");
  applyConfig(config);
  await refreshHttpStatus();
  await refreshAllPrinterStatuses();
  await refreshPrintLog();
  await refreshOriginPermissions();
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
  if (import.meta.env.DEV) {
    els.launchAtLogin.checked = !!currentConfig?.launchAtLogin;
    return;
  }
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

els.debugLogging.addEventListener("change", async () => {
  try {
    currentConfig = await persistSettings();
  } catch (e) {
    console.error(e);
  }
});

els.refreshPrintLogBtn.addEventListener("click", () => {
  refreshPrintLog().catch((e) => console.error(e));
});

els.clearPrintLogBtn.addEventListener("click", async () => {
  els.clearPrintLogBtn.disabled = true;
  try {
    await invoke("clear_print_log");
    await refreshPrintLog();
  } catch (e) {
    console.error(e);
  } finally {
    els.clearPrintLogBtn.disabled = false;
  }
});

async function handleOriginAction(
  action: string,
  origin: string,
  btn: HTMLButtonElement
): Promise<void> {
  btn.disabled = true;
  try {
    const command =
      action === "approve"
        ? "approve_origin"
        : action === "deny"
          ? "deny_origin"
          : "revoke_origin";
    const perms = await invoke<OriginPermissions>(command, { origin });
    renderOriginPermissions(perms);
  } catch (e) {
    console.error(e);
    alert(String(e));
  } finally {
    btn.disabled = false;
  }
}

els.originPendingList.addEventListener("click", (e) => {
  const target = e.target;
  if (!(target instanceof Element)) return;
  const btn = target.closest<HTMLButtonElement>("[data-origin-action]");
  if (!btn) return;
  const row = btn.closest<HTMLElement>("[data-origin]");
  const origin = row?.dataset.origin;
  const action = btn.dataset.originAction;
  if (!origin || !action) return;
  void handleOriginAction(action, origin, btn);
});

els.originAllowedList.addEventListener("click", (e) => {
  const target = e.target;
  if (!(target instanceof Element)) return;
  const btn = target.closest<HTMLButtonElement>("[data-origin-action]");
  if (!btn) return;
  const row = btn.closest<HTMLElement>("[data-origin]");
  const origin = row?.dataset.origin;
  const action = btn.dataset.originAction;
  if (!origin || !action) return;
  void handleOriginAction(action, origin, btn);
});

void listen("origin-pending", () => {
  document.querySelector<HTMLButtonElement>('[data-tab="general"]')?.click();
  refreshOriginPermissions().catch((e) => console.error(e));
});

els.printLogList.addEventListener("click", async (e) => {
  const target = e.target;
  if (!(target instanceof Element)) return;
  const btn = target.closest<HTMLButtonElement>("[data-resend-id]");
  if (!btn) return;
  const id = Number(btn.dataset.resendId);
  if (!Number.isFinite(id)) return;
  btn.disabled = true;
  try {
    await invoke("resend_print_log", { id });
    await refreshPrintLog();
  } catch (err) {
    console.error(err);
    alert(`Resend failed: ${err}`);
  } finally {
    btn.disabled = false;
  }
});

els.searchBtn.addEventListener("click", async () => {
  els.searchBtn.disabled = true;
  els.searchResults.classList.remove("hidden");
  els.searchResults.innerHTML = buildSearchingHtml();
  try {
    lastDiscovered = await invoke<PrinterInfo[]>("discover_printers");
    if (!lastDiscovered.length) {
      els.searchResults.innerHTML = buildNoPrintersHtml();
      return;
    }
    els.searchResults.innerHTML = buildSearchResultsHtml(lastDiscovered);

    els.searchResults.querySelectorAll<HTMLButtonElement>("[data-save-idx]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const printer = lastDiscovered[Number(btn.dataset.saveIdx)];
        currentConfig = await invoke<AppConfig>("set_default_printer", { printer });
        applyConfig(currentConfig);
        els.searchResults.classList.add("hidden");
        await refreshAllPrinterStatuses();
      });
    });

    els.searchResults.querySelectorAll<HTMLButtonElement>("[data-test-idx]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const idx = Number(btn.dataset.testIdx);
        const printer = lastDiscovered[idx];
        const statusEl = els.searchResults.querySelector(`[data-status-idx="${idx}"]`);
        if (!statusEl) return;
        btn.disabled = true;
        statusEl.innerHTML = `<span class="badge neutral">Testing…</span>`;
        try {
          const status = await invoke<PrinterStatus>("check_printer_status", { printer });
          statusEl.innerHTML = connectionStatusBadgeHtml(status);
        } catch (e) {
          statusEl.innerHTML = `<span class="badge bad">${escapeHtml(String(e))}</span>`;
        } finally {
          btn.disabled = false;
        }
      });
    });

    els.searchResults.querySelectorAll<HTMLAnchorElement>("[data-config-url]").forEach((link) => {
      link.addEventListener("click", async (ev) => {
        ev.preventDefault();
        try {
          const url = link.dataset.configUrl;
          if (url) await openUrl(url);
        } catch (err) {
          console.error(err);
        }
      });
    });
  } catch (e) {
    els.searchResults.innerHTML = buildSearchErrorHtml(e);
  } finally {
    els.searchBtn.disabled = false;
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
    currentConfig = await invoke<AppConfig>("add_manual_printer", {
      name: els.manualName.value.trim(),
      address,
      printPort: Number(els.manualPort.value) || 9100,
    });
    applyConfig(currentConfig);
    els.manualName.value = "";
    els.manualAddress.value = "";
    els.manualPort.value = "9100";
    await refreshAllPrinterStatuses();
  } catch (e) {
    alert(String(e));
  } finally {
    els.addManualBtn.disabled = false;
  }
});

els.port.addEventListener("input", updateApiUrl);

document.querySelectorAll<HTMLElement>(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    const name = tab.dataset.tab;
    document.querySelectorAll<HTMLElement>(".tab").forEach((t) => {
      const active = t.dataset.tab === name;
      t.classList.toggle("active", active);
      t.setAttribute("aria-selected", active ? "true" : "false");
    });
    document.querySelectorAll<HTMLElement>(".tab-panel").forEach((panel) => {
      const active = panel.dataset.panel === name;
      panel.classList.toggle("active", active);
      panel.hidden = !active;
    });
    if (name === "general") {
      refreshPrintLog().catch((e) => console.error(e));
    }
  });
});

getCurrentWindow()
  .onFocusChanged(({ payload: focused }) => {
    if (focused) {
      void refreshAllPrinterStatuses();
      refreshPrintLog().catch((e) => console.error(e));
    }
  })
  .catch((e) => console.error(e));

load().catch((e) => {
  els.httpStatus.textContent = `Failed to load: ${e}`;
});
