import type { BrowserPrintAPI, DeviceInfo } from "./types";
/** Serializable device payload for /write and /read. */
export declare function devicePayload(device: {
    name?: string;
    uid?: string;
    connection?: string;
    deviceType?: string;
    version?: number;
    provider?: string;
    manufacturer?: string;
}): Record<string, unknown>;
export declare function createDeviceClass(api: BrowserPrintAPI, baseUrl: string): new (info: DeviceInfo) => InstanceType<BrowserPrintAPI["Device"]>;
