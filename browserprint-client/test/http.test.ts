import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import {
  DEFAULT_TIMEOUT_MS,
  parseJson,
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
