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

export function buildSearchResultsHtml(printers: PrinterInfo[]): string {
  return printers
    .map((p, idx) => {
      const usb = isUsbPrinter(p);
      const title = escapeHtml(p.model || p.address);
      const heading = usb
        ? `<h3>${title}</h3>`
        : `<h3>
          <a href="#" data-config-url="http://${escapeHtml(p.address)}:${escapeHtml(p.configPort)}">
            ${title}
          </a>
        </h3>`;
      const meta = usb
        ? `Connection: USB<br />
          Device: ${escapeHtml(p.address)}<br />
          Serial: ${escapeHtml(p.serialNumber || "—")}`
        : `IP: ${escapeHtml(p.address)}<br />
          Firmware: ${escapeHtml(p.firmware || "—")}<br />
          Serial: ${escapeHtml(p.serialNumber || "—")}<br />
          Print port: ${escapeHtml(p.printPort)}`;
      return `
      <div class="result-card">
        ${heading}
        <p class="result-meta">
          <span class="badge neutral">${usb ? "USB" : "Network"}</span><br />
          ${meta}
        </p>
        <div class="button-row">
          <button type="button" data-test-idx="${idx}">Test connection</button>
          <button type="button" class="primary" data-save-idx="${idx}">Save &amp; set default</button>
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
