/** Mirrors Rust serde camelCase models used by Tauri commands. */

export interface PrinterInfo {
  name?: string | null;
  model: string;
  firmware?: string;
  serialNumber?: string;
  address: string;
  port?: number;
  printPort: number;
  configPort: number;
  /** `"network"` (TCP), `"usb"` (CDC/serial), or `"virtual"`. Defaults to network. */
  connection?: string;
  /** Append-only capture file for a virtual printer. */
  capturePath?: string | null;
}

export interface AppConfig {
  listenAddress: string;
  port: number;
  defaultPrinter?: PrinterInfo | null;
  addedPrinters: PrinterInfo[];
  launchAtLogin: boolean;
  browserPrintCompatible: boolean;
  debugLogging: boolean;
  allowLocalScripts: boolean;
  allowedOrigins?: string[];
  deniedOrigins?: string[];
}

export interface OriginPermissions {
  allowed: string[];
  pending: string[];
  denied: string[];
}

export interface PrinterStatus {
  printPort: number;
  configPort: number;
  status: string;
  errorMessages: string[];
  warningMessages: string[];
  detail?: string | null;
}

export interface PrintLogEntry {
  id: number;
  timestampMs: number;
  route: string;
  printerName: string;
  printerAddress: string;
  printPort: number;
  dataPreview: string;
  dataBytes: number;
  truncated: boolean;
  ok: boolean;
  error?: string | null;
}

export interface UpdateStatus {
  current: string;
  latest?: string | null;
  url?: string | null;
  available: boolean;
  checkedAtMs?: number | null;
  error?: string | null;
  /** Windows only: installer downloaded and verified, waiting for restart. */
  readyToInstall?: boolean;
}

export type PrinterStatusKind = "checking" | "online" | "offline";

export interface UiPrinterStatus {
  kind: PrinterStatusKind;
  detail?: string;
}
