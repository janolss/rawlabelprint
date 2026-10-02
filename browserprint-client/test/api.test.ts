import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { createBrowserPrint } from "../src/api";

const originalFetch = globalThis.fetch;
const BASE = "http://127.0.0.1:9100/";

type FetchCall = {
  url: string;
  method: string;
  body: string | FormData | null | undefined;
};

function mockFetch(
  handler: (call: FetchCall) => Promise<Response> | Response
): FetchCall[] {
  const calls: FetchCall[] = [];
  globalThis.fetch = async (input, init) => {
    const url = String(input);
    const call: FetchCall = {
      url,
      method: (init?.method ?? "GET").toUpperCase(),
      body: init?.body as string | FormData | null | undefined,
    };
    calls.push(call);
    return handler(call);
  };
  return calls;
}

function once<T>(run: (resolve: (value: T) => void, reject: (err: Error) => void) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    run(resolve, reject);
  });
}

afterEach(() => {
  globalThis.fetch = originalFetch;
});

const sampleDevice = {
  deviceType: "printer",
  uid: "net:10.0.0.5:9100",
  name: "ZD421 (10.0.0.5)",
  connection: "network",
  version: 2,
  provider: "com.zebra.ds.webdriver.desktop.provider.DefaultDeviceProvider",
  manufacturer: "Zebra Technologies",
};

test("getLocalDevices hydrates Device instances", async () => {
  const bp = createBrowserPrint(BASE);
  mockFetch(() =>
    Response.json({
      printer: [sampleDevice],
      deviceList: [sampleDevice],
    })
  );

  const map = await once<Record<string, unknown>>((resolve, reject) => {
    bp.getLocalDevices(resolve, (e) => reject(new Error(e)));
  });

  assert.ok(Array.isArray(map.printer));
  const printers = map.printer as Array<{ uid?: string; send: unknown }>;
  assert.equal(printers.length, 1);
  assert.equal(printers[0].uid, sampleDevice.uid);
  assert.equal(typeof printers[0].send, "function");
});

test("getLocalDevices with type filter returns array", async () => {
  const bp = createBrowserPrint(BASE);
  mockFetch(() => Response.json({ printer: [sampleDevice] }));

  const list = await once<unknown[]>((resolve, reject) => {
    bp.getLocalDevices(resolve, (e) => reject(new Error(e)), "printer");
  });

  assert.equal(list.length, 1);
});

test("getDefaultDevice returns null on empty body", async () => {
  const bp = createBrowserPrint(BASE);
  mockFetch(() => new Response("", { status: 200 }));

  const device = await once<unknown>((resolve, reject) => {
    bp.getDefaultDevice("printer", resolve, (e) => reject(new Error(e)));
  });

  assert.equal(device, null);
});

test("getDefaultDevice returns Device on JSON body", async () => {
  const bp = createBrowserPrint(BASE);
  const calls = mockFetch(() => Response.json(sampleDevice));

  const device = await once<{ uid?: string }>((resolve, reject) => {
    bp.getDefaultDevice("printer", resolve, (e) => reject(new Error(e)));
  });

  assert.equal(device?.uid, sampleDevice.uid);
  assert.equal(calls[0].url, `${BASE}default?type=printer`);
});

test("getApplicationConfiguration parses /config", async () => {
  const bp = createBrowserPrint(BASE);
  mockFetch(() =>
    Response.json({
      application: {
        version: "0.2.0",
        build_number: 1,
        api_level: 2,
        platform: "linux",
        supportedConversions: {},
      },
    })
  );

  const cfg = await once<{ application: { api_level: number } } | null>(
    (resolve, reject) => {
      bp.getApplicationConfiguration(resolve, (e) => reject(new Error(e)));
    }
  );

  assert.equal(cfg?.application.api_level, 2);
});

test("invalid JSON from getLocalDevices hits error callback", async () => {
  const bp = createBrowserPrint(BASE);
  mockFetch(() => new Response("not-json", { status: 200 }));

  const message = await once<string>((resolve) => {
    bp.getLocalDevices(
      () => resolve("unexpected-success"),
      (err) => resolve(err)
    );
  });

  assert.match(message, /Invalid JSON/);
});

test("device.send posts JSON to /write", async () => {
  const bp = createBrowserPrint(BASE);
  const calls = mockFetch(() => new Response("ok", { status: 200 }));
  const device = new bp.Device(sampleDevice);

  const result = await once<string>((resolve, reject) => {
    device.send("^XA^XZ", resolve, (e) => reject(new Error(e)));
  });

  assert.equal(result, "ok");
  assert.equal(calls[0].url, `${BASE}write`);
  assert.equal(calls[0].method, "POST");
  assert.equal(typeof calls[0].body, "string");
  const parsed = JSON.parse(String(calls[0].body));
  assert.equal(parsed.data, "^XA^XZ");
  assert.equal(parsed.device.uid, sampleDevice.uid);
});

test("device.sendFile posts multipart FormData", async () => {
  const bp = createBrowserPrint(BASE);
  const calls = mockFetch(() => new Response("ok", { status: 200 }));
  const device = new bp.Device(sampleDevice);
  const blob = new Blob(["^XA^XZ"], { type: "text/plain" });

  await once<string>((resolve, reject) => {
    device.sendFile(blob, resolve, (e) => reject(new Error(e)));
  });

  assert.ok(calls[0].body instanceof FormData);
  const form = calls[0].body as FormData;
  const json = form.get("json");
  assert.equal(typeof json, "string");
  const meta = JSON.parse(String(json));
  assert.equal(meta.device.uid, sampleDevice.uid);
  assert.ok(form.get("blob") instanceof Blob);
});

test("device.read posts to /read", async () => {
  const bp = createBrowserPrint(BASE);
  const calls = mockFetch(() => new Response("PRINTER STATUS", { status: 200 }));
  const device = new bp.Device(sampleDevice);

  const text = await once<string>((resolve, reject) => {
    device.read(resolve, (e) => reject(new Error(e)));
  });

  assert.equal(text, "PRINTER STATUS");
  assert.equal(calls[0].url, `${BASE}read`);
});

test("convert posts multipart to /convert", async () => {
  const bp = createBrowserPrint(BASE);
  const calls = mockFetch(() =>
    Response.json({ blob: "unused", clientAction: "print" })
  );
  const device = new bp.Device(sampleDevice);
  const blob = new Blob(["png-bytes"], { type: "image/png" });

  const result = await once<Record<string, unknown>>((resolve, reject) => {
    bp.convert(blob, device, { action: "print" }, resolve, (e) =>
      reject(new Error(e))
    );
  });

  assert.equal(result.clientAction, "print");
  assert.equal(calls[0].url, `${BASE}convert`);
  assert.ok(calls[0].body instanceof FormData);
  const meta = JSON.parse(String((calls[0].body as FormData).get("json")));
  assert.equal(meta.options.fromFormat, "png");
  assert.equal(meta.device.uid, sampleDevice.uid);
});

test("convert without resource reports error", async () => {
  const bp = createBrowserPrint(BASE);

  const message = await once<string>((resolve) => {
    bp.convert(null, null, {}, undefined, (err) => resolve(err));
  });

  assert.equal(message, "Resource not specified");
});

test("agent down surfaces error callback (never silent)", async () => {
  const bp = createBrowserPrint(BASE);
  globalThis.fetch = async () => {
    throw new TypeError("fetch failed");
  };
  const device = new bp.Device(sampleDevice);

  const message = await once<string>((resolve) => {
    device.send("^XA^XZ", () => resolve("ok"), (err) => resolve(err));
  });

  assert.match(message, /fetch failed/);
});
