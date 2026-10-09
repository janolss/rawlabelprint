import { createBrowserPrint } from "./api";
import { DEFAULT_TIMEOUT_MS, resolveBaseUrl } from "./http";
import type { BrowserPrintAPI } from "./types";
import { createZebra } from "./zebra";
import type { ZebraAPI } from "./zebra";

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
export type {
  PrinterConfiguration,
  PrinterInfo,
  PrinterOptions,
  PrinterStatus,
  ZebraAPI,
  ZebraPrinter,
  ZebraPrinterConstructor,
} from "./zebra";

/** Singleton matching Zebra's global `BrowserPrint` object. */
const BrowserPrint: BrowserPrintAPI = createBrowserPrint();

/** Zebra helper layer (`Zebra.Printer`, `Zebra.watch`), bound to the singleton above. */
const Zebra: ZebraAPI = createZebra(BrowserPrint);

// Attach helpers used by ESM consumers / tests without making them IIFE named exports.
Object.assign(BrowserPrint, {
  createBrowserPrint,
  DEFAULT_TIMEOUT_MS,
  resolveBaseUrl,
  Zebra,
});

export default BrowserPrint;

declare global {
  interface Window {
    BrowserPrint: BrowserPrintAPI;
    Zebra: ZebraAPI;
  }
}

// Ensure script-tag / IIFE consumers always see window.BrowserPrint and window.Zebra.
if (typeof globalThis !== "undefined") {
  const g = globalThis as typeof globalThis & {
    BrowserPrint?: BrowserPrintAPI;
    Zebra?: ZebraAPI;
  };
  g.BrowserPrint = BrowserPrint;
  g.Zebra = Zebra;
}
