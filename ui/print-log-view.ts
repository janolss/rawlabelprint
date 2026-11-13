/** Pure helpers for Recent prints UI (kept separate for unit tests). */

import { friendlyModelLabel } from "./printer-utils.ts";
import type { PrintLogEntry } from "./types.ts";

export function escapeHtml(value: unknown): string {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** Normalize stored printer labels that still contain raw discovery model text. */
export function formatPrinterName(name: string | undefined): string {
  const raw = (name || "").trim();
  if (!raw) return "";
  if (!/ZTC|ZPL|-\d+dpi/i.test(raw)) return raw;
  const withoutHost = raw.replace(/\s+\([^)]*\)\s*$/, "");
  return friendlyModelLabel(withoutHost) || withoutHost;
}

type DateCtor = new (ms: number) => { toLocaleString(): string };

export function formatLogTime(ms: number | null | undefined, now: DateCtor = Date): string {
  if (!ms) return "—";
  try {
    return new now(ms).toLocaleString();
  } catch {
    return String(ms);
  }
}

export function buildPrintLogHtml(
  entries: PrintLogEntry[] | null | undefined,
  { formatTime = formatLogTime }: { formatTime?: (ms: number) => string } = {}
): string {
  if (!entries?.length) {
    return `<div class="printer-card muted">No print requests captured yet</div>`;
  }
  return entries
    .map((entry) => {
      const status = entry.ok
        ? `<span class="badge ok">OK</span>`
        : `<span class="badge bad">Error</span>`;
      const size = entry.truncated
        ? `${entry.dataBytes} bytes (truncated)`
        : `${entry.dataBytes} bytes`;
      const error = entry.error
        ? `<p class="print-log-error">${escapeHtml(entry.error)}</p>`
        : "";
      return `
      <div class="print-log-entry">
        <div class="print-log-meta">
          ${status}
          <strong>${escapeHtml(entry.route || "?")}</strong>
          <span>${escapeHtml(formatTime(entry.timestampMs))}</span>
          <span>${escapeHtml(formatPrinterName(entry.printerName) || "—")} · ${escapeHtml(entry.printerAddress || "")}</span>
          <span>${escapeHtml(size)}</span>
          <button type="button" class="print-log-resend" data-resend-id="${escapeHtml(entry.id)}">Send again</button>
        </div>
        <pre>${escapeHtml(entry.dataPreview || "")}</pre>
        ${error}
      </div>`;
    })
    .join("");
}
