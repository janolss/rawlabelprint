import type { BrowserPrintAPI, DeviceInfo, DeviceLike } from "./types";
/** Cap for accumulated read data in readUntilStringReceived / readAllAvailable. */
export declare const MAX_READ_ACCUMULATED_BYTES: number;
/** Stable key for read intervals and watch maps. */
export declare function deviceKey(device: DeviceInfo | DeviceLike): string;
/** Validate agent JSON before hydrating a Device instance. */
export declare function asDeviceInfo(item: unknown): DeviceInfo;
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
