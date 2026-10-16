import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const clientRoot = fileURLToPath(new URL("..", import.meta.url));

function runModule(source: string): { status: number | null; stderr: string; stdout: string } {
  const result = spawnSync(
    process.execPath,
    ["--import", "tsx", "--input-type=module", "-e", source],
    { cwd: clientRoot, encoding: "utf8" }
  );
  return {
    status: result.status,
    stderr: result.stderr ?? "",
    stdout: result.stdout ?? "",
  };
}

test("index does not attach globals when document is undefined", () => {
  const result = runModule(`
    delete globalThis.BrowserPrint;
    delete globalThis.Zebra;
    await import("./src/index.ts");
    if (globalThis.BrowserPrint !== undefined) {
      console.error("BrowserPrint was set");
      process.exit(1);
    }
    if (globalThis.Zebra !== undefined) {
      console.error("Zebra was set");
      process.exit(2);
    }
  `);
  assert.equal(result.status, 0, result.stderr);
});

test("index attaches globals when document exists and they are missing", () => {
  const result = runModule(`
    globalThis.document = {};
    delete globalThis.BrowserPrint;
    delete globalThis.Zebra;
    await import("./src/index.ts");
    if (typeof globalThis.BrowserPrint?.getLocalDevices !== "function") {
      console.error("BrowserPrint missing");
      process.exit(1);
    }
    if (typeof globalThis.Zebra?.Printer !== "function") {
      console.error("Zebra missing");
      process.exit(2);
    }
  `);
  assert.equal(result.status, 0, result.stderr);
});

test("index does not overwrite an existing BrowserPrint global", () => {
  const result = runModule(`
    globalThis.document = {};
    const sentinel = { marker: "keep" };
    globalThis.BrowserPrint = sentinel;
    delete globalThis.Zebra;
    await import("./src/index.ts");
    if (globalThis.BrowserPrint !== sentinel) {
      console.error("BrowserPrint was overwritten");
      process.exit(1);
    }
    if (typeof globalThis.Zebra?.Printer !== "function") {
      console.error("Zebra was not set");
      process.exit(2);
    }
  `);
  assert.equal(result.status, 0, result.stderr);
});
