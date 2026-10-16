import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { createBrowserPrint } from "../src/api";
import {
  deviceKey,
  devicePayload,
  MAX_READ_ACCUMULATED_BYTES,
} from "../src/device";

const originalFetch = globalThis.fetch;
const BASE = "http://127.0.0.1:9100/";

afterEach(() => {
  globalThis.fetch = originalFetch;
});

test("devicePayload only includes data fields", () => {
  const payload = devicePayload({
    name: "P",
    uid: "u1",
    connection: "network",
    deviceType: "printer",
    version: 2,
    provider: "prov",
    manufacturer: "Zebra Technologies",
  });
  assert.deepEqual(payload, {
    name: "P",
    uid: "u1",
    connection: "network",
    deviceType: "printer",
    version: 2,
    provider: "prov",
    manufacturer: "Zebra Technologies",
  });
});

test("bluetooth devices get readRetries = 1", () => {
  const bp = createBrowserPrint(BASE);
  const bt = new bp.Device({
    name: "BT",
    uid: "bt1",
    connection: "bluetooth",
    deviceType: "printer",
  });
  const net = new bp.Device({
    name: "NET",
    uid: "n1",
    connection: "network",
    deviceType: "printer",
  });
  assert.equal(bt.readRetries, 1);
  assert.equal(net.readRetries, 0);
});

test("sendThenRead chains write then read", async () => {
  const bp = createBrowserPrint(BASE);
  const urls: string[] = [];
  globalThis.fetch = async (input, init) => {
    urls.push(`${init?.method ?? "GET"} ${String(input)}`);
    if (String(input).endsWith("/write")) {
      return new Response("sent", { status: 200 });
    }
    return new Response("read-data", { status: 200 });
  };

  const device = new bp.Device({
    name: "P",
    uid: "u1",
    connection: "network",
    deviceType: "printer",
  });

  const text = await new Promise<string>((resolve, reject) => {
    device.sendThenRead(
      "^XA^HH^XZ",
      resolve,
      (err) => reject(new Error(err))
    );
  });

  assert.equal(text, "read-data");
  assert.deepEqual(urls, [
    `POST ${BASE}write`,
    `POST ${BASE}read`,
  ]);
});

test("deviceKey uses uid when present, else name|connection|deviceType", () => {
  assert.equal(
    deviceKey({ uid: "u1", name: "A", connection: "network", deviceType: "printer" }),
    "u1"
  );
  assert.equal(
    deviceKey({ uid: "", name: "A", connection: "network", deviceType: "printer" }),
    "A|network|printer"
  );
  assert.equal(
    deviceKey({ name: "A", connection: "network", deviceType: "printer" }),
    "A|network|printer"
  );
});

test("readUntilStringReceived errors when accumulated data exceeds the limit", async () => {
  const bp = createBrowserPrint(BASE);
  const chunk = "x".repeat(Math.floor(MAX_READ_ACCUMULATED_BYTES / 2) + 1);
  let reads = 0;
  globalThis.fetch = async (input) => {
    if (String(input).endsWith("/read")) {
      reads += 1;
      return new Response(chunk, { status: 200 });
    }
    return new Response("{}", { status: 200 });
  };
  const device = new bp.Device({
    name: "P",
    uid: "u-overflow",
    connection: "network",
    deviceType: "printer",
  });

  const message = await new Promise<string>((resolve) => {
    device.readUntilStringReceived(
      "never-matches",
      () => resolve("unexpected-success"),
      (err) => resolve(err)
    );
  });

  assert.match(message, /Read buffer exceeded maximum size/);
  assert.equal(reads, 2);
});

test("readOnInterval keys timers by uid (two devices do not clobber)", async () => {
  const bp = createBrowserPrint(BASE);
  let reads = 0;
  globalThis.fetch = async () => {
    reads += 1;
    return new Response("x", { status: 200 });
  };

  const a = new bp.Device({
    name: "A",
    uid: "uid-a",
    connection: "network",
    deviceType: "printer",
  });
  const b = new bp.Device({
    name: "B",
    uid: "uid-b",
    connection: "network",
    deviceType: "printer",
  });

  bp.readOnInterval(a, () => {}, 50);
  bp.readOnInterval(b, () => {}, 50);

  await new Promise((r) => setTimeout(r, 80));
  bp.stopReadOnInterval(a);
  bp.stopReadOnInterval(b);

  // Both devices should have been able to schedule reads independently.
  assert.ok(reads >= 2, `expected >= 2 reads, got ${reads}`);
});
