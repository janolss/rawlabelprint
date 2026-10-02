/** Default request timeout for localhost agent calls. */
export declare const DEFAULT_TIMEOUT_MS = 15000;
/**
 * Resolve Browser Print agent base URL.
 * Matches Zebra BrowserPrint-3.1.250: Safari on https pages uses :9101.
 */
export declare function resolveBaseUrl(userAgent?: string | undefined, protocol?: string | undefined): string;
export type RequestOptions = {
    method?: string;
    body?: BodyInit | null;
    headers?: HeadersInit;
    timeoutMs?: number;
    /** When true, return response body even for non-OK status (caller handles). */
    raw?: boolean;
};
/**
 * fetch wrapper with timeout and consistent string errors.
 * Resolves with response text on HTTP 200 (Zebra client only treats 200 as success).
 */
export declare function requestText(url: string, options?: RequestOptions): Promise<string>;
/**
 * fetch a URL as Blob (for loadFileFromUrl / convert URL resources).
 */
export declare function requestBlob(url: string, options?: {
    timeoutMs?: number;
}): Promise<Blob>;
/** Safe JSON.parse that throws a clear Error. */
export declare function parseJson<T>(text: string): T;
/** Invoke a success callback, swallowing consumer throws. */
export declare function invokeSuccess<T>(cb: ((value: T) => void) | undefined, value: T): void;
/** Invoke an error callback with a string message. */
export declare function invokeError(cb: ((error: string) => void) | undefined, fallback: ((error: string) => void) | undefined, error: unknown): void;
