/** Pure HTML builders for LAN/USB search results. */

import { escapeHtml } from "./print-log-view.ts";
import { isUsbPrinter } from "./printer-utils.ts";
import type { PrinterInfo, PrinterStatus } from "./types.ts";

export function buildSearchingHtml(): string {
  return `<div class="printer-card muted">Searching LAN (UDP 4201) and USB…</div>`;
}

export function buildNoPrintersHtml(): string {
  return `<div class="printer-card muted">No printers found</div>`;
}

export function buildSearchErrorHtml(error: unknown): string {
  return `<div class="printer-card"><span class="badge bad">${escapeHtml(String(error))}</span></div>`;
}

export function buildSearchResultsHtml(
  printers: PrinterInfo[],
  savedAddresses: Set<string> = new Set(),
  makeDefaultChecked = false
): string {
  return printers
    .map((p, idx) => {
      const usb = isUsbPrinter(p);
      const title = escapeHtml(p.model || p.address);
      const alreadySaved = savedAddresses.has(p.address);
      const heading = `<h3>${title}</h3>`;
      const configLink = usb
        ? ""
        : `<a class="result-config-link" href="#" data-config-url="http://${escapeHtml(p.address)}:${escapeHtml(p.configPort)}">Open web config</a>`;
      const meta = usb
        ? `Connection: USB<br />
          Device: ${escapeHtml(p.address)}<br />
          Serial: ${escapeHtml(p.serialNumber || "—")}`
        : `IP: ${escapeHtml(p.address)}<br />
          Firmware: ${escapeHtml(p.firmware || "—")}<br />
          Serial: ${escapeHtml(p.serialNumber || "—")}<br />
          Print port: ${escapeHtml(p.printPort)}`;
      const saveLabel = alreadySaved ? "Update" : "Save";
      return `
      <div class="result-card">
        ${heading}
        <p class="result-meta">
          <span class="badge neutral">${usb ? "USB" : "Network"}</span>
          ${alreadySaved ? `<span class="badge ok">Saved</span>` : ""}
          <br />
          ${meta}
          ${configLink ? `<br />${configLink}` : ""}
        </p>
        <label class="checkbox">
          <input type="checkbox" data-make-default-idx="${idx}" ${makeDefaultChecked ? "checked" : ""} />
          Make default
        </label>
        <div class="button-row">
          <button type="button" data-test-idx="${idx}">Test connection</button>
          <button type="button" class="primary" data-save-idx="${idx}">${saveLabel}</button>
        </div>
        <div class="conn-status" data-status-idx="${idx}"></div>
      </div>`;
    })
    .join("");
}

export function connectionStatusBadgeHtml(
  status: PrinterStatus | null | undefined
): string {
  if (!status) return "";
  const detail = status.detail ? ` ${escapeHtml(status.detail)}` : "";
  if (status.errorMessages?.length) {
    return `<div class="conn-status-line"><span class="badge bad">Offline</span> ${escapeHtml(status.errorMessages.join(", "))}${detail ? `<div class="muted conn-status-detail">${detail}</div>` : ""}</div>`;
  }
  if (status.status === "online") {
    const warn = status.warningMessages?.length
      ? ` <span class="badge warn">${escapeHtml(status.warningMessages.join(", "))}</span>`
      : "";
    return `<div class="conn-status-line"><span class="badge ok">Online</span>${warn}</div>`;
  }
  return `<div class="conn-status-line"><span class="badge bad">Offline</span>${detail ? `<div class="muted conn-status-detail">${detail}</div>` : ""}</div>`;
}
