import type { BrowserPrintAPI } from "./types";
export type { ApplicationConfigurationData, BindableField, BrowserPrintAPI, ConvertOptions, DeviceInfo, DeviceLike, ErrorCallback, LocalDevicesMap, ScanOptions, SuccessCallback, } from "./types";
/** Singleton matching Zebra's global `BrowserPrint` object. */
declare const BrowserPrint: BrowserPrintAPI;
export default BrowserPrint;
declare global {
    interface Window {
        BrowserPrint: BrowserPrintAPI;
    }
}
