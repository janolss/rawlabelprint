/**
 * Zebra.Printer: request queue, SGD, convert helpers.
 * setSGD is queued so it cannot interleave with a pending query.
 */
import type { BrowserPrintAPI } from "../types";
import type { ZebraPrinterConstructor } from "./types";
export declare function createPrinterClass(api: BrowserPrintAPI): ZebraPrinterConstructor;
