/** Pure HTML builders for the Saved printers section. */

import { escapeHtml } from "./print-log-view.ts";
import { displayName, isUsbPrinter, isVirtualPrinter } from "./printer-utils.ts";
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

function sortDefaultFirst(
  list: PrinterInfo[],
  defaultAddress: string | null
): PrinterInfo[] {
  if (!defaultAddress) return [...list];
  return [...list].sort((a, b) => {
    const aDef = a.address === defaultAddress ? 0 : 1;
    const bDef = b.address === defaultAddress ? 0 : 1;
    return aDef - bDef;
  });
}

function locationLabel(p: PrinterInfo): string {
  if (isVirtualPrinter(p)) {
    return p.capturePath?.trim() ? escapeHtml(p.capturePath) : "Virtual";
  }
  if (isUsbPrinter(p)) return escapeHtml(p.address);
  return `${escapeHtml(p.address)}:${escapeHtml(p.printPort)}`;
}

export function buildSavedPrintersHtml(
  list: PrinterInfo[] | null | undefined,
  defaultAddress: string | null,
  statuses: Map<string, UiPrinterStatus>,
  editingAddresses: Set<string> = new Set()
): string {
  if (!list?.length) {
    return `<div class="printer-card muted">No saved printers yet. Search on LAN/USB or add manually.</div>`;
  }

  return sortDefaultFirst(list, defaultAddress)
    .map((p) => {
      const isDefault = p.address === defaultAddress;
      const nameValue = p.name?.trim() ? p.name : "";
      const usb = isUsbPrinter(p);
      const virtual = isVirtualPrinter(p);
      const kind = virtual ? "Virtual" : usb ? "USB" : "Network";
      const editing = editingAddresses.has(p.address);
      const showModel =
        !!p.model &&
        p.model !== "Manual" &&
        p.model.trim() !== (p.name?.trim() || "");
      const portRow =
        usb || virtual
          ? ""
          : `<div class="row">
          <label>Print port</label>
          <input type="number" data-field="printPort" min="1" max="65535" value="${escapeHtml(p.printPort)}" />
        </div>`;
      const locationRow = virtual
        ? `<div class="row">
          <label>Capture file</label>
          <input type="text" data-field="capturePath" value="${escapeHtml(p.capturePath || "")}" placeholder="Optional path" />
        </div>`
        : `<div class="row">
          <label>${usb ? "Device" : "Address"}</label>
          <input type="text" data-field="address" value="${escapeHtml(p.address)}" ${usb ? "readonly" : ""} />
        </div>`;
      return `
      <div class="saved-card" data-address="${escapeHtml(p.address)}" data-connection="${escapeHtml(p.connection || "network")}">
        <div class="saved-header">
          <div class="saved-title">
            <strong>${escapeHtml(displayName(p))}</strong>
            <span class="saved-meta"> · ${locationLabel(p)}</span>
          </div>
          ${isDefault ? `<span class="badge ok">Default</span>` : ""}
          <span class="badge neutral">${kind}</span>
          ${statusBadgeHtml(p.address, statuses)}
          ${showModel ? `<span class="badge neutral">${escapeHtml(p.model)}</span>` : ""}
          <div class="saved-actions">
            <button type="button" data-action="recheck" title="Recheck status">↻</button>
            <button type="button" data-action="test">Test print</button>
            <button type="button" data-action="edit">${editing ? "Done" : "Edit"}</button>
          </div>
        </div>
        <div class="saved-test-status" data-test-status></div>
        <div class="saved-edit${editing ? "" : " hidden"}">
          <div class="row">
            <label>Name</label>
            <input type="text" data-field="name" value="${escapeHtml(nameValue)}" placeholder="${escapeHtml(p.model || p.address)}" />
          </div>
          ${locationRow}
          ${portRow}
          <div class="button-row">
            <button type="button" class="primary" data-action="save">Save changes</button>
            ${isDefault ? "" : `<button type="button" data-action="default">Set as default</button>`}
            <button type="button" data-action="remove">Remove</button>
          </div>
        </div>
      </div>`;
    })
    .join("");
}
