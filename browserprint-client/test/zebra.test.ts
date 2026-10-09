import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { createBrowserPrint } from "../src/api";
import {
  createZebra,
  PrinterConfiguration,
  PrinterInfo,
  PrinterStatus,
} from "../src/zebra";

const originalFetch = globalThis.fetch;
const BASE = "http://127.0.0.1:9100/";

afterEach(() => {
  globalThis.fetch = originalFetch;
});

const HS_READY =
  "\x02030,0,0,0956,000,0,0,0,000,0,0,0\x03\r\n" +
  "\x02000,0,0,0,1,2,4,0,00000000,1,000\x03\r\n" +
  "\x021234,0\x03\r\n";

const HI = "\x02ZD421-203dpi,V84.20.18Z,8,8176KB\x03\r\n";

const HH =
  "\x02\r\n" +
  "  +10.0               DARKNESS\r\n" +
  "  6.0 IPS             PRINT SPEED\r\n" +
  "  +000                TEAR OFF\r\n" +
  "  832                 PRINT WIDTH\r\n" +
  "  1218                LABEL LENGTH\r\n" +
  "  V84.20.18Z <-       FIRMWARE\r\n" +
  "  7.0                 LINK-OS VERSION\r\n\x03\r\n";

/** Scripted printer behind the agent's /write and /read endpoints. */
function fakePrinter(replies: Record<string, string>) {
  const written: string[] = [];
  let pending = "";
  let reads = 0;
  globalThis.fetch = async (input, init) => {
    const url = String(input);
    if (url.endsWith("/write")) {
      const body = JSON.parse(String(init?.body)) as { data: string };
      written.push(body.data);
      const key = Object.keys(replies).find((k) => body.data.includes(k));
      pending = key ? replies[key] : "";
      return new Response("{}", { status: 200 });
    }
    if (url.endsWith("/read")) {
      reads += 1;
      const out = pending;
      pending = "";
      return new Response(out, { status: 200 });
    }
    return new Response("not found", { status: 404 });
  };
  return { written, reads: () => reads };
}

function setup(replies: Record<string, string>) {
  const bp = createBrowserPrint(BASE);
  const zebra = createZebra(bp, { pollIntervalMs: 20 });
  const fake = fakePrinter(replies);
  const printer = new zebra.Printer(
    { uid: "u1", name: "P", connection: "network", deviceType: "printer" },
    { autoLoadConfiguration: false }
  );
  return { bp, zebra, printer, fake };
}

test("Status parses ready printer", () => {
  const s = new PrinterStatus(HS_READY);
  assert.equal(s.offline, false);
  assert.equal(s.isPrinterReady(), true);
  assert.equal(s.getMessage(), "Ready");
  assert.equal(s.formatsInBuffer, 0);
});

test("Status flags: paper out, paused, head open, ribbon out", () => {
  const hs = (l1b: string, l1c: string, l2c: string, l2d: string) =>
    `\x02030,${l1b},${l1c},0956,000,0,0,0,000,0,0,0\x03\r\n` +
    `\x02000,0,${l2c},${l2d},1,2,4,0,00000000,1,000\x03\r\n\x021234,0\x03\r\n`;
  assert.equal(new PrinterStatus(hs("1", "0", "0", "0")).getMessage(), "Paper Out");
  assert.equal(new PrinterStatus(hs("0", "1", "0", "0")).getMessage(), "Paused");
  assert.equal(new PrinterStatus(hs("0", "0", "1", "0")).getMessage(), "Head Open");
  assert.equal(new PrinterStatus(hs("0", "0", "0", "1")).getMessage(), "Ribbon Out");
});

test("Status message priority: offline > paperOut > headOpen > ribbonOut > paused", () => {
  const s = new PrinterStatus(HS_READY);
  s.paused = true;
  s.ribbonOut = true;
  assert.equal(s.getMessage(), "Ribbon Out");
  s.headOpen = true;
  assert.equal(s.getMessage(), "Head Open");
  s.paperOut = true;
  assert.equal(s.getMessage(), "Paper Out");
});

test("Status tolerates LF-only line endings", () => {
  const lf = HS_READY.replace(/\r\n/g, "\n").replace(
    "\x02000,0,0,0,1",
    "\x02000,0,1,0,1"
  );
  assert.equal(new PrinterStatus(lf).headOpen, true);
});

test("Status without STX/ETX is offline", () => {
  for (const raw of ["", "garbage", undefined]) {
    const s = new PrinterStatus(raw);
    assert.equal(s.offline, true);
    assert.equal(s.getMessage(), "Offline");
    assert.equal(s.isPrinterReady(), false);
  }
});

test("Info parses model and firmware, rejects invalid", () => {
  const info = new PrinterInfo(HI);
  assert.equal(info.model, "ZD421-203dpi");
  assert.equal(info.firmware, "V84.20.18Z");
  assert.throws(() => new PrinterInfo(""), /Invalid Response/);
  assert.throws(() => new PrinterInfo("nope"), /Invalid Response/);
});

test("Configuration parses report and tolerates missing keys", () => {
  const cfg = new PrinterConfiguration(HH);
  assert.equal(cfg.darkness, 10);
  assert.equal(cfg.printSpeed, 6);
  assert.equal(cfg.printWidth, 832);
  assert.equal(cfg.labelLength, 1218);
  assert.equal(cfg.firmwareVersion, "V84.20.18Z");
  assert.equal(cfg.linkOSVersion, "7.0");

  const sparse = new PrinterConfiguration("\x02\r\n  832                 PRINT WIDTH\r\n\x03");
  assert.equal(sparse.printWidth, 832);
  assert.ok(Number.isNaN(sparse.printSpeed));
  assert.equal(sparse.firmwareVersion, "");
  assert.equal(sparse.linkOSVersion, "0");
  assert.throws(() => new PrinterConfiguration("x"), /Invalid Response/);
});

test("getStatus: callback and Promise forms", async () => {
  const { printer, fake } = setup({ "~hs": HS_READY });
  const viaPromise = await printer.getStatus();
  assert.equal(viaPromise.isPrinterReady(), true);
  const viaCb = await new Promise<PrinterStatus>((resolve, reject) =>
    printer.getStatus(resolve, reject)
  );
  assert.equal(viaCb.getMessage(), "Ready");
  assert.deepEqual(fake.written, ["~hs\r\n", "~hs\r\n"]);
});

test("getStatus with silent printer resolves offline instead of rejecting", async () => {
  const { printer } = setup({});
  const status = await printer.getStatus();
  assert.equal(status.offline, true);
});

test("concurrent getStatus calls share one round trip", async () => {
  const { printer, fake } = setup({ "~hs": HS_READY });
  const first = printer.getStatus();
  const [a, b, c] = await Promise.all([first, printer.getStatus(), printer.getStatus()]);
  assert.equal(a.getMessage(), "Ready");
  assert.equal(b.getMessage(), "Ready");
  assert.equal(c.getMessage(), "Ready");
  // The first runs immediately; the other two join one queued request.
  assert.equal(fake.written.length, 2);
});

test("isPrinterReady resolves with message, rejects with reason", async () => {
  const ok = setup({ "~hs": HS_READY });
  assert.equal(await ok.printer.isPrinterReady(), "Ready");
  const offline = setup({});
  await assert.rejects(offline.printer.isPrinterReady(), (e) => e === "Offline");
});

test("isPrinterReady forwards transport errors to the error callback", async () => {
  const { printer } = setup({});
  globalThis.fetch = async () => new Response("boom", { status: 500 });
  const err = await new Promise<string>((resolve) =>
    printer.isPrinterReady(
      () => resolve("unexpected success"),
      (e) => resolve(String(e))
    )
  );
  assert.equal(err, "boom");
});

test("getInfo and getConfiguration", async () => {
  const { printer, fake } = setup({ "~hi": HI, "^HH": HH });
  const info = await printer.getInfo();
  assert.equal(info.model, "ZD421-203dpi");
  const cfg = await printer.getConfiguration();
  assert.equal(cfg.printWidth, 832);
  assert.equal(printer.configuration?.labelLength, 1218);
  assert.deepEqual(fake.written, ["~hi\r\n", "^XA^HH^XZ"]);
});

test("getInfo rejects with 'Invalid Response' and the queue keeps going", async () => {
  const { printer } = setup({ "~hi": "", "~hs": HS_READY });
  const failing = printer.getInfo();
  const next = printer.getStatus();
  await assert.rejects(failing, (e) => e === "Invalid Response");
  assert.equal((await next).getMessage(), "Ready");
});

test("callback errors without an error handler use defaultErrorCallback", async () => {
  const { bp, printer } = setup({});
  globalThis.fetch = async () => new Response("boom", { status: 500 });
  const errors: string[] = [];
  bp.defaultErrorCallback = (message) => errors.push(message);
  printer.getInfo(() => {});
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(errors, ["boom"]);
});

test("SGD get/set/setThenGet", async () => {
  const { printer, fake } = setup({
    'getvar "device.host_status"': '"ready"',
    'setvar "ezpl.print_width"': "",
    'getvar "ezpl.print_width"': '"832"',
  });
  assert.equal(await printer.getSGD("device.host_status"), '"ready"');
  await printer.setSGD("ezpl.print_width", "832");
  assert.equal(await printer.setThenGetSGD("ezpl.print_width", "832"), '"832"');
  assert.deepEqual(fake.written, [
    '! U1 getvar "device.host_status"\r\n',
    '! U1 setvar "ezpl.print_width" "832"\r\n',
    '! U1 setvar "ezpl.print_width" "832"\r\n',
    '! U1 getvar "ezpl.print_width"\r\n',
  ]);
});

test("query sends arbitrary command and returns reply", async () => {
  const { printer } = setup({ "~hq": "\x02MODEL\x03" });
  assert.equal(await printer.query("~hq"), "\x02MODEL\x03");
});

test("clearRequestQueue rejects queued requests but not the running one", async () => {
  const { printer } = setup({ "~hi": HI, "~hs": HS_READY });
  const running = printer.getInfo();
  const queued = printer.getStatus();
  printer.clearRequestQueue();
  await assert.rejects(queued, (e) => e === "Request cancelled");
  assert.equal((await running).firmware, "V84.20.18Z");
});

test("printImageAsLabel loads config, sets fitTo/action and calls /convert", async () => {
  const { bp, printer } = setup({ "^HH": HH });
  const seen: { options?: Record<string, unknown> } = {};
  bp.convert = (_res, _dev, options, ok) => {
    seen.options = options as Record<string, unknown>;
    ok?.({ done: true });
  };
  const userOptions = { fromFormat: "png" } as Record<string, unknown>;
  const result = await printer.printImageAsLabel("x.png", userOptions);
  assert.deepEqual(result, { done: true });
  assert.equal(seen.options?.action, "print");
  assert.deepEqual(seen.options?.fitTo, { width: 832, height: 1218 });
  assert.equal(userOptions.action, undefined, "caller options must not be mutated");
});

test("getConvertedResource / storeConvertedResource use return / store", async () => {
  const { bp, printer } = setup({ "^HH": HH });
  const actions: unknown[] = [];
  bp.convert = (_r, _d, options, ok) => {
    actions.push((options as { action?: string }).action);
    ok?.("ok");
  };
  await printer.getConvertedResource("a.png");
  await printer.storeConvertedResource("a.png");
  assert.deepEqual(actions, ["return", "store"]);
});

test("convert errors reach the error callback", async () => {
  const { bp, printer } = setup({ "^HH": HH });
  bp.convert = (_r, _d, _o, _ok, err) => err?.("Image/PDF conversion is not supported.");
  await assert.rejects(printer.printImageAsLabel("a.png"), /not supported/);
});

test("watch reports first status, changes, and offline after threshold", async () => {
  const bp = createBrowserPrint(BASE);
  const zebra = createZebra(bp, { pollIntervalMs: 10 });
  let reply = HS_READY;
  let silent = false;
  globalThis.fetch = async (input) => {
    const url = String(input);
    if (url.endsWith("/read")) return new Response(silent ? "" : reply, { status: 200 });
    return new Response("{}", { status: 200 });
  };
  const device = { uid: "w1", name: "W", connection: "network", deviceType: "printer" };
  const events: Array<[unknown, PrinterStatus]> = [];
  zebra.watch(device, (prev, cur) => events.push([prev, cur]), 2);
  const until = async (n: number) => {
    for (let i = 0; i < 200 && events.length < n; i++) await new Promise((r) => setTimeout(r, 10));
  };
  await until(1);
  assert.equal(events[0][0], "", "first change reports an empty previous status");
  assert.equal(events[0][1].getMessage(), "Ready");

  reply = HS_READY.replace("\x02000,0,0,0,1", "\x02000,0,1,0,1");
  await until(2);
  assert.equal(events[1][1].getMessage(), "Head Open");

  silent = true;
  await until(3);
  assert.equal(events[2][1].getMessage(), "Offline");

  zebra.stopWatching(device);
  const count = events.length;
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(events.length, count, "no events after stopWatching");
});
