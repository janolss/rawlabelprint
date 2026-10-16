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

// Browser drop-in: expose globals only in a document context, and only if unset.
if (typeof document !== "undefined") {
  const g = globalThis as typeof globalThis & {
    BrowserPrint?: BrowserPrintAPI;
    Zebra?: ZebraAPI;
  };
  if (g.BrowserPrint == null) {
    g.BrowserPrint = BrowserPrint;
  }
  if (g.Zebra == null) {
    g.Zebra = Zebra;
  }
}
