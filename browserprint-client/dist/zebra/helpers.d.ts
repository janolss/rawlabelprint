import type { ErrorCallback, SuccessCallback } from "../types";
/** Reject SGD name/value that would break out of quoted getvar/setvar. */
export declare function assertSgdToken(value: string, label: "name" | "value"): void;
export declare const CONVERT_FAILED = "Conversion is not supported by this Browser Print agent";
export declare function errorText(err: unknown): string;
/**
 * Calls `run` with callbacks when any callback is given, else returns a Promise.
 * Mirrors Zebra's "callbacks or Promise" convention.
 */
export declare function dual<T>(run: (ok: (value: T) => void, fail: (reason: string) => void) => void, success: SuccessCallback<T> | undefined, error: ErrorCallback | undefined, fallbackError: ErrorCallback): Promise<T> | undefined;
