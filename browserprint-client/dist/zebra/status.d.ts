/**
 * Parsers for Zebra host responses: `~HS` (status), `~HI` (info), `^HH` (configuration).
 * Field-based `~HS` parsing so `\n` vs `\r\n` line endings do not matter.
 */
export declare const STX = "\u0002";
export declare const ETX = "\u0003";
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
