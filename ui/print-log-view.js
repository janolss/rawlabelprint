/** Pure helpers for Recent prints UI (kept separate for unit tests). */

export function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function formatLogTime(ms, now = Date) {
  if (!ms) return "—";
  try {
    return new now(ms).toLocaleString();
  } catch {
    return String(ms);
  }
}

export function buildPrintLogHtml(entries, { formatTime = formatLogTime } = {}) {
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
          <span>${escapeHtml(entry.printerName || "—")} · ${escapeHtml(entry.printerAddress || "")}</span>
          <span>${escapeHtml(size)}</span>
          <button type="button" class="print-log-resend" data-resend-id="${escapeHtml(entry.id)}">Send again</button>
        </div>
        <pre>${escapeHtml(entry.dataPreview || "")}</pre>
        ${error}
      </div>`;
    })
    .join("");
}
