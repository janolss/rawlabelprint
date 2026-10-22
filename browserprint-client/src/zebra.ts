/**
 * Zebra helper layer (`Zebra.Printer`, `Zebra.watch`, `Zebra.stopWatching`).
 *
 * Provenance: this is an independent implementation written for RawLabelPrint. It reproduces the
 * *public behaviour* of Zebra's `BrowserPrint-Zebra-1.1.250` (method names, argument order,
 * callback/Promise duality, result shapes) but contains no Zebra source code. Wire formats come
 * from Zebra's published printer command documentation (`~HS`, `~HI`, `^HH`, SGD `getvar`/`setvar`).
 *
 * Deliberate differences (all backwards compatible):
 * - `Status`/`Info`/`Configuration` are static members from the start (Zebra only defines them
 *   after the first `Printer` is constructed).
 * - `~HS` is parsed by comma-separated field, not absolute character offset, so `\n` vs `\r\n`
 *   line endings do not matter. Extra fields are exposed as additional properties.
 * - `Configuration` tolerates missing keys (NaN / empty string) instead of failing the request.
 * - A failed request never causes the next queued request to be skipped.
 * - `setSGD` goes through the request queue so it cannot interleave with a pending query.
 * - `isPrinterReady` forwards transport errors to the error callback.
 * - The background configuration load stops after a bounded number of attempts and can be
 *   disabled with `new Zebra.Printer(device, { autoLoadConfiguration: false })`.
 * - `watch` only polls while something is watched, never overlaps polls for one printer, and
 *   accepts a plain `Device`.
 * - Conversion helpers copy the options object instead of mutating the caller's.
 */
export {
  createZebra,
  PrinterConfiguration,
  PrinterInfo,
  PrinterStatus,
} from "./zebra/index";
export type {
  PrinterOptions,
  ZebraAPI,
  ZebraConfig,
  ZebraPrinter,
  ZebraPrinterConstructor,
} from "./zebra/index";
