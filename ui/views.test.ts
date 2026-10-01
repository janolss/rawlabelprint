import assert from "node:assert/strict";
import test from "node:test";
import { displayName } from "./printer-utils.ts";
import { buildSavedPrintersHtml, statusBadgeHtml } from "./saved-printers-view.ts";
import {
  buildNoPrintersHtml,
  buildSearchErrorHtml,
  buildSearchResultsHtml,
  buildSearchingHtml,
  connectionStatusBadgeHtml,
} from "./search-results-view.ts";
import type { PrinterInfo, UiPrinterStatus } from "./types.ts";

const sample: PrinterInfo = {
  name: "Shipping",
  model: "ZD421",
  address: "10.0.0.5",
  printPort: 9100,
  configPort: 80,
  serialNumber: "SN1",
  firmware: "V1",
};

test("displayName prefers explicit name", () => {
  assert.equal(displayName(sample), "Shipping");
  assert.equal(displayName({ ...sample, name: null }), "ZD421 (10.0.0.5)");
});

test("buildSavedPrintersHtml empty state", () => {
  const html = buildSavedPrintersHtml([], null, new Map());
  assert.match(html, /No saved printers yet/);
});

test("buildSavedPrintersHtml includes default and status badges", () => {
  const statuses = new Map<string, UiPrinterStatus>([
    ["10.0.0.5", { kind: "online" }],
  ]);
  const html = buildSavedPrintersHtml([sample], "10.0.0.5", statuses);
  assert.match(html, /Shipping/);
  assert.match(html, /Default/);
  assert.match(html, />Online</);
  assert.match(html, /data-action="save"/);
  assert.doesNotMatch(html, /Set as default/);
});

test("statusBadgeHtml offline includes detail", () => {
  const statuses = new Map<string, UiPrinterStatus>([
    ["10.0.0.5", { kind: "offline", detail: "No route" }],
  ]);
  const html = statusBadgeHtml("10.0.0.5", statuses);
  assert.match(html, /Offline/);
  assert.match(html, /No route/);
});

test("search results builders", () => {
  assert.match(buildSearchingHtml(), /UDP 4201/);
  assert.match(buildNoPrintersHtml(), /No printers found/);
  assert.match(buildSearchErrorHtml("<boom>"), /&lt;boom&gt;/);

  const html = buildSearchResultsHtml([sample]);
  assert.match(html, /data-save-idx="0"/);
  assert.match(html, /data-test-idx="0"/);
  assert.match(html, /data-config-url="http:\/\/10\.0\.0\.5:80"/);
  assert.match(html, /SN1/);
});

test("connectionStatusBadgeHtml online and offline", () => {
  assert.match(
    connectionStatusBadgeHtml({
      printPort: 9100,
      configPort: 80,
      status: "online",
      errorMessages: [],
      warningMessages: ["head cold"],
    }),
    /Online/
  );
  assert.match(
    connectionStatusBadgeHtml({
      printPort: 9100,
      configPort: 80,
      status: "offline",
      errorMessages: ["down"],
      warningMessages: [],
      detail: "os 65",
    }),
    /Offline/
  );
});
