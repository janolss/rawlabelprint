import { createBrowserPrint } from "./api";
import { DEFAULT_TIMEOUT_MS, resolveBaseUrl } from "./http";
import type { BrowserPrintAPI } from "./types";

export type {
  ApplicationConfigurationData,
  BindableField,
  BrowserPrintAPI,
  ConvertOptions,
  DeviceInfo,
  DeviceLike,
  ErrorCallback,
  LocalDevicesMap,
  ScanOptions,
  SuccessCallback,
} from "./types";

/** Singleton matching Zebra's global `BrowserPrint` object. */
const BrowserPrint: BrowserPrintAPI = createBrowserPrint();

// Attach helpers used by ESM consumers / tests without making them IIFE named exports.
Object.assign(BrowserPrint, {
  createBrowserPrint,
  DEFAULT_TIMEOUT_MS,
  resolveBaseUrl,
});

export default BrowserPrint;

declare global {
  interface Window {
    BrowserPrint: BrowserPrintAPI;
  }
}

// Ensure script-tag / IIFE consumers always see window.BrowserPrint.
if (typeof globalThis !== "undefined") {
  (globalThis as typeof globalThis & { BrowserPrint?: BrowserPrintAPI }).BrowserPrint =
    BrowserPrint;
}
