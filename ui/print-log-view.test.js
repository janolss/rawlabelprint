import assert from "node:assert/strict";
import test from "node:test";
import {
  buildPrintLogHtml,
  escapeHtml,
  formatLogTime,
} from "./print-log-view.js";

test("escapeHtml encodes markup", () => {
  assert.equal(escapeHtml(`<b>&"`), "&lt;b&gt;&amp;&quot;");
});

test("formatLogTime falls back for missing values", () => {
  assert.equal(formatLogTime(0), "—");
  assert.equal(formatLogTime(null), "—");
});

test("buildPrintLogHtml shows empty state", () => {
  const html = buildPrintLogHtml([]);
  assert.match(html, /No print requests captured yet/);
  assert.doesNotMatch(html, /Send again/);
});

test("buildPrintLogHtml includes Send again and payload", () => {
  const html = buildPrintLogHtml(
    [
      {
        id: 7,
        ok: true,
        route: "/write",
        timestampMs: 1_700_000_000_000,
        printerName: "ZD421",
        printerAddress: "192.168.1.50",
        dataBytes: 12,
        truncated: false,
        dataPreview: "^XA^FDHi^FS^XZ",
      },
    ],
    { formatTime: () => "fixed-time" }
  );

  assert.match(html, /data-resend-id="7"/);
  assert.match(html, />Send again</);
  assert.match(html, /badge ok/);
  assert.match(html, /\/write/);
  assert.match(html, /fixed-time/);
  assert.match(html, /ZD421/);
  assert.match(html, /\^XA\^FDHi\^FS\^XZ/);
});

test("buildPrintLogHtml shows error state", () => {
  const html = buildPrintLogHtml(
    [
      {
        id: 2,
        ok: false,
        route: "/",
        timestampMs: 1,
        printerName: "A",
        printerAddress: "10.0.0.1",
        dataBytes: 100,
        truncated: true,
        dataPreview: "partial",
        error: "Could not connect",
      },
    ],
    { formatTime: () => "t" }
  );

  assert.match(html, /badge bad/);
  assert.match(html, /100 bytes \(truncated\)/);
  assert.match(html, /Could not connect/);
  assert.match(html, /data-resend-id="2"/);
});
