/** Pure HTML builders for the Saved printers section. */

import { escapeHtml } from "./print-log-view.ts";
import { displayName } from "./printer-utils.ts";
import type { PrinterInfo, UiPrinterStatus } from "./types.ts";

export function statusBadgeHtml(
  address: string,
  statuses: Map<string, UiPrinterStatus>
): string {
  const entry = statuses.get(address);
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

export function applyStatusBadgeDom(
  el: HTMLElement,
  entry: UiPrinterStatus | undefined
): void {
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

export function buildSavedPrintersHtml(
  list: PrinterInfo[] | null | undefined,
  defaultAddress: string | null,
  statuses: Map<string, UiPrinterStatus>
): string {
  if (!list?.length) {
    return `<div class="printer-card muted">No saved printers yet. Search on LAN or add manually.</div>`;
  }

  return list
    .map((p) => {
      const isDefault = p.address === defaultAddress;
      const nameValue = p.name?.trim() ? p.name : "";
      return `
      <div class="saved-card" data-address="${escapeHtml(p.address)}">
        <div class="saved-header">
          <strong>${escapeHtml(displayName(p))}</strong>
          ${isDefault ? `<span class="badge ok">Default</span>` : ""}
          ${statusBadgeHtml(p.address, statuses)}
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
}
