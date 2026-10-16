import type { BrowserPrintAPI } from "./types";
export type CreateBrowserPrintOptions = {
    baseUrl?: string;
    debug?: boolean;
    /** Opt-in origin allowlist for URL fetches / sendUrl. Null/empty = unrestricted. */
    allowedUrlOrigins?: string[] | null;
};
export declare function createBrowserPrint(baseUrlOrOptions?: string | CreateBrowserPrintOptions): BrowserPrintAPI;
