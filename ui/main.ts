import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { buildOverviewHtml } from "./overview-view.ts";
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
  ThemePreference,
  UiPrinterStatus,
  UpdateStatus,
} from "./types.ts";

function requireEl<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`Missing element #${id}`);
  return el as T;
}

const els = {
  listenAddress: requireEl<HTMLInputElement>("listenAddress"),
  listenAddressDisplay: requireEl<HTMLElement>("listenAddressDisplay"),
  port: requireEl<HTMLInputElement>("port"),
  browserPrintCompatible: requireEl<HTMLInputElement>("browserPrintCompatible"),
  debugLogging: requireEl<HTMLInputElement>("debugLogging"),
  allowLocalScripts: requireEl<HTMLInputElement>("allowLocalScripts"),
  httpStatus: requireEl<HTMLElement>("httpStatus"),
  saveHttpBtn: requireEl<HTMLButtonElement>("saveHttpBtn"),
  copyApiUrlBtn: requireEl<HTMLButtonElement>("copyApiUrlBtn"),
  savedList: requireEl<HTMLElement>("savedList"),
  searchBtn: requireEl<HTMLButtonElement>("searchBtn"),
  searchResults: requireEl<HTMLElement>("searchResults"),
  toggleManualBtn: requireEl<HTMLButtonElement>("toggleManualBtn"),
  manualForm: requireEl<HTMLElement>("manualForm"),
  manualName: requireEl<HTMLInputElement>("manualName"),
  manualAddress: requireEl<HTMLInputElement>("manualAddress"),
  manualPort: requireEl<HTMLInputElement>("manualPort"),
  manualMakeDefault: requireEl<HTMLInputElement>("manualMakeDefault"),
  addManualBtn: requireEl<HTMLButtonElement>("addManualBtn"),
  virtualName: requireEl<HTMLInputElement>("virtualName"),
  virtualCapturePath: requireEl<HTMLInputElement>("virtualCapturePath"),
  virtualMakeDefault: requireEl<HTMLInputElement>("virtualMakeDefault"),
  addVirtualBtn: requireEl<HTMLButtonElement>("addVirtualBtn"),
  launchAtLogin: requireEl<HTMLInputElement>("launchAtLogin"),
  themeSystem: requireEl<HTMLInputElement>("themeSystem"),
  themeLight: requireEl<HTMLInputElement>("themeLight"),
  themeDark: requireEl<HTMLInputElement>("themeDark"),
  apiUrl: requireEl<HTMLElement>("apiUrl"),
  printLogList: requireEl<HTMLElement>("printLogList"),
  refreshPrintLogBtn: requireEl<HTMLButtonElement>("refreshPrintLogBtn"),
  clearPrintLogBtn: requireEl<HTMLButtonElement>("clearPrintLogBtn"),
  originPendingList: requireEl<HTMLElement>("originPendingList"),
  originAllowedList: requireEl<HTMLElement>("originAllowedList"),
  originDeniedList: requireEl<HTMLElement>("originDeniedList"),
  exportPrintersBtn: requireEl<HTMLButtonElement>("exportPrintersBtn"),
  importPrintersBtn: requireEl<HTMLButtonElement>("importPrintersBtn"),
  importPrintersFile: requireEl<HTMLInputElement>("importPrintersFile"),
  transferStatus: requireEl<HTMLElement>("transferStatus"),
  overviewRoot: requireEl<HTMLElement>("overviewRoot"),
  navPrintersBadge: requireEl<HTMLElement>("navPrintersBadge"),
  navSitesBadge: requireEl<HTMLElement>("navSitesBadge"),
};

let currentConfig: AppConfig | null = null;
let lastDiscovered: PrinterInfo[] = [];
let lastPrintLog: PrintLogEntry[] = [];
let lastOriginPerms: OriginPermissions = { allowed: [], pending: [], denied: [] };
let lastHttpStatusText = "…";
let lastUpdateStatus: UpdateStatus | null = null;
let updateChecking = false;
const printerStatuses = new Map<string, UiPrinterStatus>();
const statusCheckedAt = new Map<string, number>();
const editingAddresses = new Set<string>();
let statusRefreshSeq = 0;
let savedPortValue = "9100";
let themePreference: ThemePreference = "system";
let systemThemeMql: MediaQueryList | null = null;
const STATUS_DEBOUNCE_MS = 30_000;
const STATUS_CHECK_CONCURRENCY = 4;

function normalizeTheme(value: unknown): ThemePreference {
  return value === "light" || value === "dark" || value === "system" ? value : "system";
}

function resolveTheme(preference: ThemePreference): "light" | "dark" {
  if (preference === "light" || preference === "dark") return preference;
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function onSystemThemeChange(): void {
  if (themePreference === "system") {
    document.documentElement.dataset.theme = resolveTheme("system");
  }
}

/** Apply Settings appearance; listens to OS changes only while preference is system. */
function applyTheme(preference: ThemePreference): void {
  themePreference = preference;
  document.documentElement.dataset.theme = resolveTheme(preference);

  if (!systemThemeMql) {
    systemThemeMql = matchMedia("(prefers-color-scheme: dark)");
  }
  systemThemeMql.removeEventListener("change", onSystemThemeChange);
  if (preference === "system") {
    systemThemeMql.addEventListener("change", onSystemThemeChange);
  }
}

function selectedTheme(): ThemePreference {
  if (els.themeLight.checked) return "light";
  if (els.themeDark.checked) return "dark";
  return "system";
}

function setThemeRadios(preference: ThemePreference): void {
  els.themeSystem.checked = preference === "system";
  els.themeLight.checked = preference === "light";
  els.themeDark.checked = preference === "dark";
}

function defaultAddress(): string | null {
  return currentConfig?.defaultPrinter?.address ?? null;
}

function hasDefaultPrinter(): boolean {
  return !!currentConfig?.defaultPrinter;
}

function syncMakeDefaultCheckboxes(): void {
  const checked = !hasDefaultPrinter();
  els.manualMakeDefault.checked = checked;
  els.virtualMakeDefault.checked = checked;
}

function savedAddressSet(): Set<string> {
  return new Set((currentConfig?.addedPrinters || []).map((p) => p.address));
}

function updateApiUrl(): void {
  const host = els.listenAddress.value.trim() || "127.0.0.1";
  const port = els.port.value || "9100";
  els.apiUrl.textContent = `http://${host}:${port}/`;
  els.listenAddressDisplay.textContent = host;
}

function updateSaveHttpBtn(): void {
  els.saveHttpBtn.disabled = els.port.value === savedPortValue;
}

async function refreshHttpStatus(): Promise<void> {
  try {
    const status = await invoke<string>("get_http_status");
    lastHttpStatusText = status;
    els.httpStatus.textContent = `Status: ${status}`;
  } catch (e) {
    lastHttpStatusText = `error (${e})`;
    els.httpStatus.textContent = `Status: error (${e})`;
  }
  renderOverview();
}

function updateStatusBadgeInDom(address: string): void {
  const el = [...els.savedList.querySelectorAll<HTMLElement>(".status-badge")].find(
    (badge) => badge.dataset.statusAddress === address
  );
  if (!el) return;
  applyStatusBadgeDom(el, printerStatuses.get(address));
  updateNavBadges();
  renderOverview();
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
    statusCheckedAt.set(address, Date.now());
  } catch (e) {
    printerStatuses.set(address, {
      kind: "offline",
      detail: String(e),
    });
    statusCheckedAt.set(address, Date.now());
  }
  updateStatusBadgeInDom(address);
}

async function refreshAllPrinterStatuses(respectDebounce = false): Promise<void> {
  const now = Date.now();
  const printers = (currentConfig?.addedPrinters || []).filter((printer) => {
    if (!respectDebounce) return true;
    const checkedAt = statusCheckedAt.get(printer.address) ?? 0;
    return now - checkedAt >= STATUS_DEBOUNCE_MS;
  });
  if (!printers.length) return;
  const seq = ++statusRefreshSeq;
  for (const p of printers) {
    printerStatuses.set(p.address, { kind: "checking" });
    updateStatusBadgeInDom(p.address);
  }
  let nextPrinter = 0;
  const workers = Array.from(
    { length: Math.min(STATUS_CHECK_CONCURRENCY, printers.length) },
    async () => {
      while (nextPrinter < printers.length) {
        const printer = printers[nextPrinter++];
        try {
          const result = await checkPrinterStatus(printer);
          if (seq !== statusRefreshSeq) return;
          printerStatuses.set(printer.address, result);
          statusCheckedAt.set(printer.address, Date.now());
        } catch (e) {
          if (seq !== statusRefreshSeq) return;
          printerStatuses.set(printer.address, {
            kind: "offline",
            detail: String(e),
          });
          statusCheckedAt.set(printer.address, Date.now());
        }
        if (seq === statusRefreshSeq) {
          updateStatusBadgeInDom(printer.address);
        }
      }
    }
  );
  await Promise.all(workers);
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

function updateNavBadges(): void {
  const pending = lastOriginPerms.pending.length;
  if (pending > 0) {
    els.navSitesBadge.textContent = String(pending);
    els.navSitesBadge.classList.remove("hidden", "bad");
    els.navSitesBadge.classList.add("warn");
  } else {
    els.navSitesBadge.textContent = "";
    els.navSitesBadge.classList.add("hidden");
  }

  const def = defaultAddress();
  const offline = def ? printerStatuses.get(def)?.kind === "offline" : false;
  if (offline) {
    els.navPrintersBadge.textContent = "!";
    els.navPrintersBadge.classList.remove("hidden", "warn");
    els.navPrintersBadge.classList.add("bad");
  } else {
    els.navPrintersBadge.textContent = "";
    els.navPrintersBadge.classList.add("hidden");
  }
}

function renderOverview(): void {
  const defAddr = defaultAddress();
  els.overviewRoot.innerHTML = buildOverviewHtml({
    httpStatus: lastHttpStatusText,
    defaultPrinter: currentConfig?.defaultPrinter ?? null,
    defaultStatus: defAddr ? printerStatuses.get(defAddr) : undefined,
    perms: lastOriginPerms,
    lastLog: lastPrintLog[0] ?? null,
    update: lastUpdateStatus,
    updateChecking,
  });
}

async function refreshUpdateStatus(): Promise<void> {
  lastUpdateStatus = await invoke<UpdateStatus>("get_update_status");
  renderOverview();
}

async function checkForUpdates(): Promise<void> {
  updateChecking = true;
  renderOverview();
  try {
    lastUpdateStatus = await invoke<UpdateStatus>("check_for_updates");
  } catch (e) {
    console.error(e);
  } finally {
    updateChecking = false;
    renderOverview();
  }
}

function renderSaved(list: PrinterInfo[] | null | undefined): void {
  els.savedList.innerHTML = buildSavedPrintersHtml(
    list,
    defaultAddress(),
    printerStatuses,
    editingAddresses
  );

  els.savedList.querySelectorAll<HTMLElement>(".saved-card").forEach((card) => {
    const originalAddress = card.dataset.address;
    bindStatusBadgeClicks(card);

    card.querySelector<HTMLButtonElement>('[data-action="edit"]')?.addEventListener(
      "click",
      () => {
        if (!originalAddress) return;
        if (editingAddresses.has(originalAddress)) {
          editingAddresses.delete(originalAddress);
        } else {
          editingAddresses.add(originalAddress);
        }
        renderSaved(currentConfig?.addedPrinters || []);
      }
    );

    card
      .querySelector<HTMLButtonElement>('[data-action="recheck"]')
      ?.addEventListener("click", () => {
        if (originalAddress) void refreshPrinterStatus(originalAddress);
      });

    card
      .querySelector<HTMLButtonElement>('[data-action="test"]')
      ?.addEventListener("click", async (e) => {
        const btn = e.currentTarget as HTMLButtonElement;
        const printer = findSavedPrinter(originalAddress);
        const statusEl = card.querySelector<HTMLElement>("[data-test-status]");
        if (!printer || !statusEl) return;
        btn.disabled = true;
        statusEl.innerHTML = `<span class="badge neutral">Printing…</span>`;
        try {
          await invoke("test_print", { printer });
          statusEl.innerHTML = `<span class="badge ok">Test print sent</span>`;
          await refreshPrintLog();
        } catch (err) {
          statusEl.innerHTML = `<span class="badge bad">${escapeHtml(String(err))}</span>`;
        } finally {
          btn.disabled = false;
        }
      });

    const saveBtn = card.querySelector<HTMLButtonElement>('[data-action="save"]');
    saveBtn?.addEventListener("click", async (e) => {
      const btn = e.currentTarget as HTMLButtonElement;
      btn.disabled = true;
      try {
        const virtual = card.dataset.connection === "virtual";
        currentConfig = await invoke<AppConfig>("update_printer", {
          originalAddress,
          name: fieldInput(card, "name").value.trim(),
          address: virtual ? originalAddress : fieldInput(card, "address").value.trim(),
          printPort: (() => {
            const portEl = card.querySelector<HTMLInputElement>('[data-field="printPort"]');
            if (!portEl) return 0;
            return Number(portEl.value) || 9100;
          })(),
          capturePath: virtual
            ? card.querySelector<HTMLInputElement>('[data-field="capturePath"]')?.value.trim() ??
              ""
            : null,
        });
        if (originalAddress) editingAddresses.delete(originalAddress);
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
        if (originalAddress) {
          printerStatuses.delete(originalAddress);
          editingAddresses.delete(originalAddress);
        }
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
  updateNavBadges();
}

function applyConfig(config: AppConfig): void {
  currentConfig = config;
  els.listenAddress.value = "127.0.0.1";
  els.port.value = String(config.port ?? 9100);
  savedPortValue = els.port.value;
  updateSaveHttpBtn();
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
  els.debugLogging.checked = !!config.debugLogging;
  els.allowLocalScripts.checked = !!config.allowLocalScripts;
  const theme = normalizeTheme(config.theme);
  setThemeRadios(theme);
  applyTheme(theme);
  renderSaved(config.addedPrinters || []);
  syncMakeDefaultCheckboxes();
  updateApiUrl();
  renderOverview();
}

async function persistSettings(): Promise<AppConfig> {
  return invoke<AppConfig>("save_settings", {
    listenAddress: "127.0.0.1",
    port: Number(els.port.value) || 9100,
    launchAtLogin: els.launchAtLogin.checked,
    browserPrintCompatible: els.browserPrintCompatible.checked,
    debugLogging: els.debugLogging.checked,
    allowLocalScripts: els.allowLocalScripts.checked,
    theme: selectedTheme(),
  });
}

function renderPrintLog(entries: PrintLogEntry[]): void {
  lastPrintLog = entries;
  els.printLogList.innerHTML = buildPrintLogHtml(entries);
  renderOverview();
}

function renderOriginPermissions(perms: OriginPermissions): void {
  lastOriginPerms = perms;
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

  const denied = perms.denied ?? [];
  if (!denied.length) {
    els.originDeniedList.innerHTML = "";
  } else {
    els.originDeniedList.innerHTML = denied
      .map(
        (origin) => `
      <div class="origin-row" data-origin="${escapeHtml(origin)}">
        <span class="badge bad">Denied</span>
        <span class="origin-url" title="${escapeHtml(origin)}">${escapeHtml(origin)}</span>
        <div class="button-row inline">
          <button type="button" data-origin-action="forget">Remove</button>
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
  updateNavBadges();
  renderOverview();
}

async function refreshOriginPermissions(): Promise<void> {
  try {
    const perms = await invoke<OriginPermissions>("get_origin_permissions");
    renderOriginPermissions(perms);
  } catch (e) {
    els.originPendingList.innerHTML = "";
    els.originDeniedList.innerHTML = "";
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

function showPane(name: string): void {
  document.querySelectorAll<HTMLElement>(".nav-item").forEach((item) => {
    const active = item.dataset.pane === name;
    item.classList.toggle("active", active);
    if (active) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  });
  document.querySelectorAll<HTMLElement>(".pane").forEach((pane) => {
    const active = pane.dataset.pane === name;
    pane.classList.toggle("active", active);
    pane.hidden = !active;
  });
  if (name === "prints") {
    refreshPrintLog().catch((e) => console.error(e));
  }
}

function highlightPendingOrigin(origin?: string): void {
  const rows = [...els.originPendingList.querySelectorAll<HTMLElement>(".origin-row")];
  const target = origin
    ? rows.find((row) => row.dataset.origin === origin)
    : rows[0];
  if (!target) return;
  target.classList.add("highlight");
  target.scrollIntoView({ behavior: "smooth", block: "nearest" });
  window.setTimeout(() => target.classList.remove("highlight"), 2500);
}

async function load(): Promise<void> {
  const config = await invoke<AppConfig>("get_config");
  applyConfig(config);
  await refreshHttpStatus();
  await refreshAllPrinterStatuses();
  await refreshPrintLog();
  await refreshOriginPermissions();
  await refreshUpdateStatus();
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
    updateSaveHttpBtn();
  }
});

els.copyApiUrlBtn.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(els.apiUrl.textContent || "");
    els.copyApiUrlBtn.textContent = "Copied";
    window.setTimeout(() => {
      els.copyApiUrlBtn.textContent = "Copy";
    }, 1500);
  } catch (e) {
    console.error(e);
  }
});

function downloadJson(filename: string, value: unknown): void {
  const blob = new Blob([JSON.stringify(value, null, 2) + "\n"], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

els.exportPrintersBtn.addEventListener("click", async () => {
  els.transferStatus.textContent = "";
  try {
    const config = await invoke<AppConfig>("get_config");
    downloadJson("rawlabelprint-printers.json", {
      defaultPrinter: config.defaultPrinter ?? null,
      addedPrinters: config.addedPrinters,
      allowedOrigins: config.allowedOrigins ?? [],
      deniedOrigins: config.deniedOrigins ?? [],
    });
    els.transferStatus.textContent = "Exported printers and websites.";
  } catch (e) {
    els.transferStatus.textContent = `Status: error (${e})`;
  }
});

els.importPrintersBtn.addEventListener("click", () => {
  els.importPrintersFile.value = "";
  els.importPrintersFile.click();
});

els.importPrintersFile.addEventListener("change", async () => {
  const file = els.importPrintersFile.files?.[0];
  if (!file) return;
  if (!confirm("Replace saved printers and website lists with this file?")) {
    els.importPrintersFile.value = "";
    return;
  }
  els.transferStatus.textContent = "";
  try {
    const payload = await file.text();
    currentConfig = await invoke<AppConfig>("import_printers_and_websites", { payload });
    applyConfig(currentConfig);
    await refreshAllPrinterStatuses();
    await refreshOriginPermissions();
    els.transferStatus.textContent = "Imported printers and websites.";
  } catch (e) {
    els.transferStatus.textContent = `Status: error (${e})`;
  } finally {
    els.importPrintersFile.value = "";
  }
});

els.launchAtLogin.addEventListener("change", async () => {
  if (import.meta.env.DEV) {
    els.launchAtLogin.checked = !!currentConfig?.launchAtLogin;
    return;
  }
  try {
    currentConfig = await persistSettings();
    applyConfig(currentConfig);
  } catch (e) {
    console.error(e);
    if (currentConfig) applyConfig(currentConfig);
    alert(`Could not update startup preference: ${e}`);
  }
});

for (const radio of [els.themeSystem, els.themeLight, els.themeDark]) {
  radio.addEventListener("change", async () => {
    if (!radio.checked) return;
    const next = selectedTheme();
    applyTheme(next);
    try {
      currentConfig = await persistSettings();
      applyConfig(currentConfig);
    } catch (e) {
      console.error(e);
      if (currentConfig) applyConfig(currentConfig);
      alert(`Could not update appearance: ${e}`);
    }
  });
}

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

els.allowLocalScripts.addEventListener("change", async () => {
  try {
    currentConfig = await persistSettings();
  } catch (e) {
    console.error(e);
    if (currentConfig) applyConfig(currentConfig);
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
          : action === "forget"
            ? "remove_denied_origin"
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

function bindOriginListClicks(list: HTMLElement): void {
  list.addEventListener("click", (e) => {
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
}

bindOriginListClicks(els.originPendingList);
bindOriginListClicks(els.originDeniedList);
bindOriginListClicks(els.originAllowedList);

void listen("origin-pending", (event) => {
  showPane("sites");
  refreshOriginPermissions()
    .then(() => {
      const origin = typeof event.payload === "string" ? event.payload : undefined;
      highlightPendingOrigin(origin);
    })
    .catch((e) => console.error(e));
});

void listen<UpdateStatus>("update-available", (event) => {
  lastUpdateStatus = event.payload;
  renderOverview();
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

els.toggleManualBtn.addEventListener("click", () => {
  const open = els.manualForm.classList.toggle("hidden") === false;
  els.toggleManualBtn.textContent = open ? "Hide manual form" : "Add manually";
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
    els.searchResults.innerHTML = buildSearchResultsHtml(
      lastDiscovered,
      savedAddressSet(),
      !hasDefaultPrinter()
    );

    els.searchResults.querySelectorAll<HTMLButtonElement>("[data-save-idx]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const idx = Number(btn.dataset.saveIdx);
        const printer = lastDiscovered[idx];
        const makeDefault =
          els.searchResults.querySelector<HTMLInputElement>(
            `[data-make-default-idx="${idx}"]`
          )?.checked ?? false;
        currentConfig = await invoke<AppConfig>("save_printer", {
          printer,
          makeDefault,
        });
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

els.addVirtualBtn.addEventListener("click", async () => {
  const name = els.virtualName.value.trim();
  if (!name) {
    alert("Name is required.");
    return;
  }
  els.addVirtualBtn.disabled = true;
  try {
    currentConfig = await invoke<AppConfig>("add_virtual_printer", {
      name,
      capturePath: els.virtualCapturePath.value.trim(),
      makeDefault: els.virtualMakeDefault.checked,
    });
    applyConfig(currentConfig);
    els.virtualName.value = "";
    els.virtualCapturePath.value = "";
    await refreshAllPrinterStatuses();
  } catch (e) {
    alert(String(e));
  } finally {
    els.addVirtualBtn.disabled = false;
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
      makeDefault: els.manualMakeDefault.checked,
    });
    applyConfig(currentConfig);
    els.manualName.value = "";
    els.manualAddress.value = "";
    els.manualPort.value = "9100";
    els.manualForm.classList.add("hidden");
    els.toggleManualBtn.textContent = "Add manually";
    await refreshAllPrinterStatuses();
  } catch (e) {
    alert(String(e));
  } finally {
    els.addManualBtn.disabled = false;
  }
});

els.port.addEventListener("input", () => {
  updateApiUrl();
  updateSaveHttpBtn();
});

document.querySelectorAll<HTMLElement>(".nav-item").forEach((item) => {
  item.addEventListener("click", () => {
    const name = item.dataset.pane;
    if (name) showPane(name);
  });
});

els.overviewRoot.addEventListener("click", (e) => {
  const target = e.target;
  if (!(target instanceof Element)) return;
  const btn = target.closest<HTMLButtonElement>("[data-overview-action]");
  if (!btn) return;
  const action = btn.dataset.overviewAction;
  if (action === "goto-printers") showPane("printers");
  if (action === "goto-sites") showPane("sites");
  if (action === "goto-prints") showPane("prints");
  if (action === "copy-api") {
    void navigator.clipboard.writeText(els.apiUrl.textContent || "").catch(console.error);
  }
  if (action === "recheck-default") {
    const addr = defaultAddress();
    if (addr) void refreshPrinterStatus(addr);
  }
  if (action === "check-update") void checkForUpdates();
  if (action === "open-release") {
    invoke("open_release_page").catch((err) => console.error(err));
  }
  if (action === "install-update") {
    invoke("install_update").catch((err) => alert(String(err)));
  }
});

getCurrentWindow()
  .onFocusChanged(({ payload: focused }) => {
    if (focused) {
      void refreshAllPrinterStatuses(true);
      refreshPrintLog().catch((e) => console.error(e));
    }
  })
  .catch((e) => console.error(e));

load().catch((e) => {
  els.httpStatus.textContent = `Failed to load: ${e}`;
});
