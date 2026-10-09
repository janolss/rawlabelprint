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
