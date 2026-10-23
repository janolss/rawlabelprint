import assert from "node:assert/strict";
import test from "node:test";
import { buildOverviewHtml } from "./overview-view.ts";
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

const other: PrinterInfo = {
  name: "Backup",
  model: "ZD420",
  address: "10.0.0.9",
  printPort: 9100,
  configPort: 80,
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
  assert.match(html, /data-action="recheck"/);
  assert.match(html, /data-action="test"/);
  assert.match(html, /data-action="edit"/);
  assert.match(html, /data-action="save"/);
  assert.doesNotMatch(html, /Set as default/);
});

test("buildSavedPrintersHtml sorts default first", () => {
  const html = buildSavedPrintersHtml([sample, other], "10.0.0.9", new Map());
  const defaultPos = html.indexOf("Backup");
  const otherPos = html.indexOf("Shipping");
  assert.ok(defaultPos >= 0 && otherPos >= 0);
  assert.ok(defaultPos < otherPos);
});

test("buildSavedPrintersHtml edit mode expands fields", () => {
  const html = buildSavedPrintersHtml([sample], null, new Map(), new Set(["10.0.0.5"]));
  assert.match(html, /Done/);
  assert.doesNotMatch(html, /saved-edit hidden/);
  assert.match(html, /data-action="default"/);
});

test("buildSavedPrintersHtml omits address and port for a virtual printer", () => {
  const html = buildSavedPrintersHtml(
    [
      {
        ...sample,
        name: "Preview",
        model: "",
        address: "Preview",
        connection: "virtual",
        printPort: 0,
        configPort: 0,
        capturePath: "/tmp/labels.zpl",
      },
    ],
    null,
    new Map(),
    new Set(["Preview"])
  );
  assert.match(html, /Preview/);
  assert.match(html, />Virtual</);
  assert.match(html, /data-field="capturePath"/);
  assert.match(html, /\/tmp\/labels\.zpl/);
  assert.doesNotMatch(html, /data-field="address"/);
  assert.doesNotMatch(html, /data-field="printPort"/);
  assert.match(html, /data-action="default"/);
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

  const html = buildSearchResultsHtml([sample], new Set(["10.0.0.5"]), true);
  assert.match(html, /data-save-idx="0"/);
  assert.match(html, />Update</);
  assert.match(html, />Saved</);
  assert.match(html, /Make default/);
  assert.match(html, /checked/);
  assert.match(html, /Open web config/);
  assert.match(html, /data-test-idx="0"/);
  assert.match(html, /data-config-url="http:\/\/10\.0\.0\.5:80"/);
  assert.match(html, /SN1/);
  assert.match(html, />Network</);
  assert.doesNotMatch(html, /<a href="#" data-config-url/);

  const fresh = buildSearchResultsHtml([sample], new Set(), false);
  assert.match(fresh, />Save</);
  assert.doesNotMatch(fresh, />Saved</);
  assert.doesNotMatch(fresh, /checked/);

  const usbHtml = buildSearchResultsHtml([
    {
      ...sample,
      connection: "usb",
      address: "/dev/ttyACM0",
      printPort: 0,
      configPort: 0,
    },
  ]);
  assert.match(usbHtml, />USB</);
  assert.doesNotMatch(usbHtml, /data-config-url/);
  assert.match(usbHtml, /\/dev\/ttyACM0/);
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

test("buildOverviewHtml warns when default is offline", () => {
  const html = buildOverviewHtml({
    httpStatus: "listening on 127.0.0.1:9100",
    defaultPrinter: sample,
    defaultStatus: { kind: "offline", detail: "unreachable" },
    perms: { allowed: ["https://a.example"], pending: ["https://b.example"], denied: [] },
    lastLog: null,
  });
  assert.match(html, /is offline/);
  assert.match(html, /data-overview-action="recheck-default"/);
  assert.match(html, /data-overview-action="goto-printers"/);
  assert.match(html, /1 pending/);
  assert.match(html, /data-overview-action="goto-sites"/);
});

test("buildOverviewHtml warns when no default", () => {
  const html = buildOverviewHtml({
    httpStatus: "listening",
    defaultPrinter: null,
    defaultStatus: undefined,
    perms: { allowed: [], pending: [], denied: [] },
    lastLog: {
      id: 1,
      timestampMs: 0,
      route: "/",
      printerName: "Shipping",
      printerAddress: "10.0.0.5",
      printPort: 9100,
      dataPreview: "^XA",
      dataBytes: 3,
      truncated: false,
      ok: true,
    },
  });
  assert.match(html, /No default printer/);
  assert.match(html, /Shipping/);
  assert.match(html, /badge ok/);
});

test("buildOverviewHtml version row states", () => {
  const base = {
    httpStatus: "listening",
    defaultPrinter: sample,
    defaultStatus: { kind: "online" } as UiPrinterStatus,
    perms: { allowed: [], pending: [], denied: [] },
    lastLog: null,
  };

  const available = buildOverviewHtml({
    ...base,
    update: { current: "0.4.0", latest: "0.5.0", available: true, checkedAtMs: 1 },
  });
  assert.match(available, /0\.5\.0 available/);
  assert.match(available, /data-overview-action="open-release"/);
  assert.doesNotMatch(available, /check-update/);

  const upToDate = buildOverviewHtml({
    ...base,
    update: { current: "0.4.0", latest: "0.4.0", available: false, checkedAtMs: 1 },
  });
  assert.match(upToDate, /Up to date/);
  assert.match(upToDate, /data-overview-action="check-update"/);

  const failed = buildOverviewHtml({
    ...base,
    update: { current: "0.4.0", available: false, checkedAtMs: 1, error: "<offline>" },
  });
  assert.match(failed, /Check failed/);
  assert.match(failed, /&lt;offline&gt;/);

  const ready = buildOverviewHtml({
    ...base,
    update: {
      current: "0.4.0",
      latest: "0.5.0",
      available: true,
      checkedAtMs: 1,
      readyToInstall: true,
    },
  });
  assert.match(ready, /0\.5\.0 ready to install/);
  assert.match(ready, /data-overview-action="install-update"/);
  assert.doesNotMatch(ready, /open-release/);

  const checking = buildOverviewHtml({ ...base, update: null, updateChecking: true });
  assert.match(checking, /Checking…/);
  assert.match(checking, /check-update" disabled/);
});
