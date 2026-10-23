/** Pure HTML builder for the Overview pane. */

import { escapeHtml } from "./print-log-view.ts";
import { displayName, isUsbPrinter, isVirtualPrinter } from "./printer-utils.ts";
import type {
  OriginPermissions,
  PrinterInfo,
  PrintLogEntry,
  UiPrinterStatus,
} from "./types.ts";

export interface OverviewModel {
  httpStatus: string;
  defaultPrinter: PrinterInfo | null;
  defaultStatus: UiPrinterStatus | undefined;
  perms: OriginPermissions;
  lastLog: PrintLogEntry | null;
}

function statusBadge(status: UiPrinterStatus | undefined): string {
  const kind = status?.kind ?? "checking";
  if (kind === "online") return `<span class="badge ok">Online</span>`;
  if (kind === "offline") {
    const detail = status?.detail ? escapeHtml(status.detail) : "";
    return `<span class="badge bad" title="${detail}">Offline</span>`;
  }
  return `<span class="badge neutral">…</span>`;
}

function printerLocation(p: PrinterInfo): string {
  if (isVirtualPrinter(p)) return "Virtual";
  if (isUsbPrinter(p)) return escapeHtml(p.address);
  return `${escapeHtml(p.address)}:${escapeHtml(p.printPort)}`;
}

function formatLogTime(ms: number): string {
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return String(ms);
  }
}

export function buildOverviewHtml(model: OverviewModel): string {
  const { httpStatus, defaultPrinter, defaultStatus, perms, lastLog } = model;
  const missingDefault = !defaultPrinter;
  const offlineDefault = defaultStatus?.kind === "offline";

  let alert = "";
  if (missingDefault) {
    alert = `
      <div class="overview-alert bad">
        <p>No default printer is set.</p>
        <div class="button-row inline">
          <button type="button" class="primary" data-overview-action="goto-printers">Add a printer</button>
        </div>
      </div>`;
  } else if (offlineDefault) {
    alert = `
      <div class="overview-alert">
        <p>Default printer ${escapeHtml(displayName(defaultPrinter))} is offline.</p>
        <div class="button-row inline">
          <button type="button" data-overview-action="recheck-default">Recheck</button>
          <button type="button" data-overview-action="goto-printers">Change default</button>
        </div>
      </div>`;
  }

  const defaultValue = defaultPrinter
    ? `${escapeHtml(displayName(defaultPrinter))} · ${printerLocation(defaultPrinter)}`
    : "None";

  const sitesParts = [
    `${perms.allowed.length} allowed`,
    perms.pending.length ? `${perms.pending.length} pending` : null,
    (perms.denied?.length ?? 0) ? `${perms.denied.length} denied` : null,
  ].filter(Boolean);

  const lastLogValue = lastLog
    ? `${escapeHtml(lastLog.printerName || lastLog.printerAddress)} · ${escapeHtml(formatLogTime(lastLog.timestampMs))}`
    : "—";

  const sitesAction =
    perms.pending.length > 0
      ? `<button type="button" data-overview-action="goto-sites">Review</button>`
      : "";

  return `
    <section class="block">
      <h2>Overview</h2>
      ${alert}
      <div class="overview-grid">
        <div class="overview-row">
          <span class="label">Server</span>
          <span class="value">${escapeHtml(httpStatus || "…")}</span>
          <button type="button" class="linkish" data-overview-action="copy-api">Copy API URL</button>
        </div>
        <div class="overview-row">
          <span class="label">Default</span>
          <span class="value">${defaultValue} ${defaultPrinter ? statusBadge(defaultStatus) : ""}</span>
          <span></span>
        </div>
        <div class="overview-row">
          <span class="label">Websites</span>
          <span class="value">${escapeHtml(sitesParts.join(" · "))}</span>
          ${sitesAction || "<span></span>"}
        </div>
        <div class="overview-row">
          <span class="label">Last print</span>
          <span class="value">${lastLogValue}${
            lastLog
              ? lastLog.ok
                ? ' <span class="badge ok">OK</span>'
                : ' <span class="badge bad">Failed</span>'
              : ""
          }</span>
          <button type="button" class="linkish" data-overview-action="goto-prints">Open</button>
        </div>
      </div>
    </section>`;
}
