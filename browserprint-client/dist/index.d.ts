import type { BrowserPrintAPI } from "./types";
import type { ZebraAPI } from "./zebra";
export type { ApplicationConfigurationData, BindableField, BrowserPrintAPI, ConvertOptions, DeviceInfo, DeviceLike, ErrorCallback, LocalDevicesMap, ScanOptions, SuccessCallback, } from "./types";
export type { PrinterConfiguration, PrinterInfo, PrinterOptions, PrinterStatus, ZebraAPI, ZebraPrinter, ZebraPrinterConstructor, } from "./zebra";
/** Singleton matching Zebra's global `BrowserPrint` object. */
declare const BrowserPrint: BrowserPrintAPI;
export default BrowserPrint;
declare global {
    interface Window {
        BrowserPrint: BrowserPrintAPI;
        Zebra: ZebraAPI;
    }
}
