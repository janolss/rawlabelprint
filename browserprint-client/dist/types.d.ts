/** Callback invoked on successful BrowserPrint operations. */
export type SuccessCallback<T = string> = (result: T) => void;
/** Callback invoked on failed BrowserPrint operations (string message). */
export type ErrorCallback = (error: string) => void;
/** Raw device fields as returned by `/available` and `/default`. */
export interface DeviceInfo {
    name?: string;
    deviceType?: string;
    connection?: string;
    uid?: string;
    version?: number;
    provider?: string;
    manufacturer?: string;
}
/** Options passed to convert / convertAndSendFile. */
export interface ConvertOptions {
    action?: string;
    fromFormat?: string;
    [key: string]: unknown;
}
/** Options passed to scanImage. */
export interface ScanOptions {
    format?: string;
    [key: string]: unknown;
}
/** Shape returned by `/config` / ApplicationConfiguration. */
export interface ApplicationConfigurationData {
    application: {
        version: string;
        build_number: number;
        api_level: number;
        platform: string;
        supportedConversions: Record<string, unknown>;
    };
}
/** Map of device-type → devices, as returned by getLocalDevices without a type filter. */
export type LocalDevicesMap = Record<string, DeviceLike[]>;
/** Minimal field surface used by bindFieldToReadData. */
export interface BindableField {
    value: string;
}
/** Device instance / constructor surface (avoids circular imports in types). */
export interface DeviceLike extends DeviceInfo {
    readRetries: number;
    sendErrorCallback: ErrorCallback;
    sendFinishedCallback: SuccessCallback;
    readErrorCallback: ErrorCallback;
    readFinishedCallback: SuccessCallback;
    send(data: string, finished?: SuccessCallback, error?: ErrorCallback): void;
    sendUrl(url: string, finished?: SuccessCallback, error?: ErrorCallback, options?: Record<string, unknown>): void;
    sendFile(resource: string | Blob, finished?: SuccessCallback, error?: ErrorCallback): void;
    convertAndSendFile(resource: string | Blob, finished?: SuccessCallback, error?: ErrorCallback, options?: ConvertOptions): void;
    read(finished?: SuccessCallback, error?: ErrorCallback): void;
    readUntilStringReceived(needle: string, finished?: SuccessCallback, error?: ErrorCallback, retries?: number, accumulated?: string): void;
    readAllAvailable(finished?: SuccessCallback, error?: ErrorCallback, retries?: number): void;
    sendThenRead(data: string, finished?: SuccessCallback, error?: ErrorCallback): void;
    sendThenReadUntilStringReceived(data: string, needle: string, finished?: SuccessCallback, error?: ErrorCallback, retries?: number): void;
    sendThenReadAllAvailable(data: string, finished?: SuccessCallback, error?: ErrorCallback, retries?: number): void;
}
export interface BrowserPrintAPI {
    Device: new (info: DeviceInfo) => DeviceLike;
    defaultSuccessCallback: SuccessCallback;
    defaultErrorCallback: ErrorCallback;
    ApplicationConfiguration: new () => ApplicationConfigurationData;
    getLocalDevices(finished: SuccessCallback<LocalDevicesMap | DeviceLike[]>, error?: ErrorCallback, deviceType?: string): void;
    getDefaultDevice(deviceType: string | null | undefined, finished: SuccessCallback<DeviceLike | null>, error?: ErrorCallback): void;
    getApplicationConfiguration(finished: SuccessCallback<ApplicationConfigurationData | null>, error?: ErrorCallback): void;
    readOnInterval(device: DeviceLike, finished: SuccessCallback, intervalMs?: number): void;
    stopReadOnInterval(device: DeviceLike): void;
    bindFieldToReadData(device: DeviceLike, field: BindableField, intervalMs?: number, onUpdate?: () => void): void;
    loadFileFromUrl(url: string, finished?: SuccessCallback<Blob>, error?: ErrorCallback): void;
    convert(resource: string | Blob | null | undefined, device: DeviceLike | null | undefined, options: ConvertOptions | null | undefined, finished?: SuccessCallback<unknown>, error?: ErrorCallback): void;
    scanImage(resource: string | Blob | null | undefined, options: ScanOptions | null | undefined, finished?: SuccessCallback<unknown>, error?: ErrorCallback): void;
}
