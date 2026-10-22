import type { ZebraAPI, ZebraPrinterConstructor } from "./types";
export declare function createWatchApi(Printer: ZebraPrinterConstructor, pollIntervalMs: number): Pick<ZebraAPI, "watch" | "stopWatching">;
