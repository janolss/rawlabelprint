import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { createBrowserPrint } from "../src/api";
import {
  DEFAULT_TIMEOUT_MS,
  parseJson,
  requestBlob,
  requestText,
  resolveBaseUrl,
} from "../src/http";

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

test("resolveBaseUrl defaults to http://127.0.0.1:9100/", () => {
  assert.equal(resolveBaseUrl("Mozilla/5.0 Chrome/120", "http:"), "http://127.0.0.1:9100/");
  assert.equal(resolveBaseUrl("Mozilla/5.0 Chrome/120", "https:"), "http://127.0.0.1:9100/");
});

test("resolveBaseUrl uses https://127.0.0.1:9101/ for Safari on https", () => {
  const safari =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
  assert.equal(resolveBaseUrl(safari, "https:"), "https://127.0.0.1:9101/");
  assert.equal(resolveBaseUrl(safari, "http:"), "http://127.0.0.1:9100/");
});

test("parseJson throws a clear error on invalid JSON", () => {
  assert.throws(() => parseJson("{"), /Invalid JSON response/);
  assert.deepEqual(parseJson<{ a: number }>('{"a":1}'), { a: 1 });
});

test("requestText resolves body on HTTP 200", async () => {
  globalThis.fetch = async () =>
    new Response("ok", { status: 200 }) as unknown as Response;

  const text = await requestText("http://127.0.0.1:9100/available");
  assert.equal(text, "ok");
});

test("requestText rejects non-200 with body message", async () => {
  globalThis.fetch = async () =>
    new Response("Device not found", { status: 404 }) as unknown as Response;

  await assert.rejects(
    () => requestText("http://127.0.0.1:9100/write"),
    /Device not found/
  );
});

test("requestText does not log when debug is off", async () => {
  let calls = 0;
  const original = console.log;
  console.log = () => {
    calls += 1;
  };
  try {
    globalThis.fetch = async () => new Response("ok", { status: 200 });
    await requestText("http://127.0.0.1:9100/available", {
      method: "GET",
      debug: false,
    });
    assert.equal(calls, 0);
  } finally {
    console.log = original;
  }
});

test("requestText debug logs method, url, status, and truncated text", async () => {
  const logs: string[] = [];
  const original = console.log;
  console.log = (...args: unknown[]) => {
    logs.push(args.map(String).join(" "));
  };
  try {
    const payload = "x".repeat(600);
    globalThis.fetch = async () => new Response("ok", { status: 200 });
    await requestText("http://127.0.0.1:9100/write", {
      method: "POST",
      body: payload,
      debug: true,
    });
    assert.equal(logs.length, 1);
    assert.match(logs[0], /BrowserPrint POST http:\/\/127\.0\.0\.1:9100\/write -> 200/);
    assert.match(logs[0], /truncated 600 chars/);
    assert.equal(logs[0].includes("x".repeat(513)), false);
  } finally {
    console.log = original;
  }
});

test("requestText debug logs multipart as a byte length", async () => {
  const logs: string[] = [];
  const original = console.log;
  console.log = (...args: unknown[]) => {
    logs.push(args.map(String).join(" "));
  };
  try {
    const form = new FormData();
    form.append("json", "{}");
    form.append("blob", new Blob(["^XA^XZ"]));
    globalThis.fetch = async () => new Response("{}", { status: 200 });
    await requestText("http://127.0.0.1:9100/write", {
      method: "POST",
      body: form,
      debug: true,
    });
    assert.match(logs[0], /body=\d+ bytes/);
    assert.equal(logs[0].includes("^XA"), false);
  } finally {
    console.log = original;
  }
});

test("requestText debug logs timeouts and keeps the error string", async () => {
  const logs: string[] = [];
  const original = console.log;
  console.log = (...args: unknown[]) => {
    logs.push(args.map(String).join(" "));
  };
  try {
    globalThis.fetch = async (_input, init) => {
      const signal = init?.signal;
      return await new Promise<Response>((_resolve, reject) => {
        signal?.addEventListener("abort", () => {
          const err = new Error("Aborted");
          err.name = "AbortError";
          reject(err);
        });
      });
    };
    await assert.rejects(
      () =>
        requestText("http://127.0.0.1:9100/write", {
          method: "POST",
          body: "abc",
          timeoutMs: 20,
          debug: true,
        }),
      /Request timed out/
    );
    assert.match(logs[0], /POST http:\/\/127\.0\.0\.1:9100\/write -> Request timed out/);
    assert.match(logs[0], /body="abc"/);
  } finally {
    console.log = original;
  }
});

test("requestBlob debug logs status and byte length", async () => {
  const logs: string[] = [];
  const original = console.log;
  console.log = (...args: unknown[]) => {
    logs.push(args.map(String).join(" "));
  };
  try {
    globalThis.fetch = async () => new Response("hello", { status: 200 });
    const blob = await requestBlob("http://127.0.0.1:9100/file", { debug: true });
    assert.equal(blob.size, 5);
    assert.match(logs[0], /GET http:\/\/127\.0\.0\.1:9100\/file -> 200 body=5 bytes/);
  } finally {
    console.log = original;
  }
});

test("createBrowserPrint keeps the string argument and defaults debug to false", () => {
  const bp = createBrowserPrint("http://127.0.0.1:9100/");
  assert.equal(bp.debug, false);
  assert.equal(bp.allowedUrlOrigins, null);
  const traced = createBrowserPrint({ debug: true });
  assert.equal(traced.debug, true);
});

test("requestText maps abort to timeout message", async () => {
  globalThis.fetch = async (_input, init) => {
    const signal = init?.signal;
    return await new Promise<Response>((_resolve, reject) => {
      signal?.addEventListener("abort", () => {
        const err = new Error("Aborted");
        err.name = "AbortError";
        reject(err);
      });
    });
  };

  await assert.rejects(
    () => requestText("http://127.0.0.1:9100/write", { timeoutMs: 20 }),
    /Request timed out/
  );
  assert.ok(DEFAULT_TIMEOUT_MS >= 1000);
});
