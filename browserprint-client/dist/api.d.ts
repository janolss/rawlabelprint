import type { BrowserPrintAPI } from "./types";
export type CreateBrowserPrintOptions = {
    baseUrl?: string;
    debug?: boolean;
};
export declare function createBrowserPrint(baseUrlOrOptions?: string | CreateBrowserPrintOptions): BrowserPrintAPI;
