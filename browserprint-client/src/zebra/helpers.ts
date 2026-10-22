import { invokeError, invokeSuccess } from "../http";
import type { ErrorCallback, SuccessCallback } from "../types";

/** Reject SGD name/value that would break out of quoted getvar/setvar. */
export function assertSgdToken(value: string, label: "name" | "value"): void {
  if (/["\x00-\x1f\x7f]/.test(value)) {
    throw new Error(`Invalid SGD ${label}`);
  }
}

export const CONVERT_FAILED = "Conversion is not supported by this Browser Print agent";

export function errorText(err: unknown): string {
  if (typeof err === "string") {
    return err;
  }
  if (err instanceof Error) {
    return err.message;
  }
  return String(err);
}

/**
 * Calls `run` with callbacks when any callback is given, else returns a Promise.
 * Mirrors Zebra's "callbacks or Promise" convention.
 */
export function dual<T>(
  run: (ok: (value: T) => void, fail: (reason: string) => void) => void,
  success: SuccessCallback<T> | undefined,
  error: ErrorCallback | undefined,
  fallbackError: ErrorCallback
): Promise<T> | undefined {
  if (!success && !error) {
    return new Promise<T>((resolve, reject) => run(resolve, reject));
  }
  run(
    (value) => invokeSuccess(success, value),
    (reason) => invokeError(error, fallbackError, reason)
  );
  return undefined;
}
