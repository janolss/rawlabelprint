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
import type { BrowserPrintAPI, ConvertOptions, DeviceInfo, DeviceLike, ErrorCallback, SuccessCallback } from "./types";
/** Result of `~HS`. Property names match Zebra's `Zebra.Printer.Status`. */
export declare class PrinterStatus {
    raw: string;
    offline: boolean;
    paperOut: boolean;
    paused: boolean;
    headOpen: boolean;
    ribbonOut: boolean;
    /** Additional `~HS` fields (not present in Zebra's class). */
    labelLengthDots?: number;
    formatsInBuffer?: number;
    bufferFull: boolean;
    partialFormatInProgress: boolean;
    corruptRam: boolean;
    underTemperature: boolean;
    overTemperature: boolean;
    labelsRemaining?: number;
    constructor(raw?: string);
    /** Zebra-compatible: true when the raw string has "1" at the given character offset. */
    isFlagSet(index: number): boolean;
    isPrinterReady(): boolean;
    getMessage(): string;
}
/** Result of `~HI`: `STX model,firmware,dpi-code,memory ETX`. */
export declare class PrinterInfo {
    raw: string;
    model: string;
    firmware: string;
    /** Remaining comma-separated fields (not present in Zebra's class). */
    extra: string[];
    constructor(raw: string);
}
/** Result of `^HH` (configuration label report). */
export declare class PrinterConfiguration {
    raw: string;
    settings: Record<string, string>;
    darkness: number;
    printSpeed: number;
    printWidth: number;
    labelLength: number;
    firmwareVersion: string;
    linkOSVersion: string;
    constructor(raw: string);
}
export interface PrinterOptions {
    /**
     * Load `^HH` in the background after construction so `printer.configuration` fills in
     * (Zebra does this unconditionally). Default true.
     */
    autoLoadConfiguration?: boolean;
}
export interface ZebraAPI {
    Printer: ZebraPrinterConstructor;
    watch(device: DeviceInfo | DeviceLike, onchange: (previous: PrinterStatus | "", current: PrinterStatus) => void, errorsForOffline?: number): void;
    stopWatching(device: DeviceInfo | DeviceLike): void;
}
export interface ZebraPrinterConstructor {
    new (info: DeviceInfo, options?: PrinterOptions): ZebraPrinter;
    Status: typeof PrinterStatus;
    Info: typeof PrinterInfo;
    Configuration: typeof PrinterConfiguration;
}
export interface ZebraPrinter extends DeviceLike {
    configuration?: PrinterConfiguration;
    clearRequestQueue(): void;
    getStatus(): Promise<PrinterStatus>;
    getStatus(ok: SuccessCallback<PrinterStatus>, fail?: ErrorCallback): void;
    isPrinterReady(): Promise<string>;
    isPrinterReady(ok: SuccessCallback<string>, fail?: ErrorCallback): void;
    getInfo(): Promise<PrinterInfo>;
    getInfo(ok: SuccessCallback<PrinterInfo>, fail?: ErrorCallback): void;
    getConfiguration(): Promise<PrinterConfiguration>;
    getConfiguration(ok: SuccessCallback<PrinterConfiguration>, fail?: ErrorCallback): void;
    getSGD(name: string): Promise<string>;
    getSGD(name: string, ok: SuccessCallback<string>, fail?: ErrorCallback): void;
    setSGD(name: string, value: string): Promise<string>;
    setSGD(name: string, value: string, ok: SuccessCallback<string>, fail?: ErrorCallback): void;
    setThenGetSGD(name: string, value: string): Promise<string>;
    setThenGetSGD(name: string, value: string, ok: SuccessCallback<string>, fail?: ErrorCallback): void;
    query(command: string): Promise<string>;
    query(command: string, ok: SuccessCallback<string>, fail?: ErrorCallback): void;
    printImageAsLabel(resource: string | Blob, options?: ConvertOptions): Promise<unknown>;
    printImageAsLabel(resource: string | Blob, options: ConvertOptions | undefined, ok: SuccessCallback<unknown>, fail?: ErrorCallback): void;
    getConvertedResource(resource: string | Blob, options?: ConvertOptions): Promise<unknown>;
    getConvertedResource(resource: string | Blob, options: ConvertOptions | undefined, ok: SuccessCallback<unknown>, fail?: ErrorCallback): void;
    storeConvertedResource(resource: string | Blob, options?: ConvertOptions): Promise<unknown>;
    storeConvertedResource(resource: string | Blob, options: ConvertOptions | undefined, ok: SuccessCallback<unknown>, fail?: ErrorCallback): void;
}
export interface ZebraConfig {
    /** Status polling interval used by `Zebra.watch` (default 2000 ms). */
    pollIntervalMs?: number;
}
export declare function createZebra(api: BrowserPrintAPI, config?: ZebraConfig): ZebraAPI;
