/** Default request timeout for localhost agent calls. */
export const DEFAULT_TIMEOUT_MS = 15_000;

/**
 * Resolve Browser Print agent base URL.
 * Matches Zebra BrowserPrint-3.1.250: Safari on https pages uses :9101.
 */
export function resolveBaseUrl(
  userAgent: string | undefined = globalThis.navigator?.userAgent,
  protocol: string | undefined = globalThis.location?.protocol
): string {
  const ua = userAgent ?? "";
  const isSafari = /^((?!chrome|android).)*safari/i.test(ua);
  if (isSafari && protocol === "https:") {
    return "https://127.0.0.1:9101/";
  }
  return "http://127.0.0.1:9100/";
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) {
    if (err.name === "AbortError") {
      return "Request timed out";
    }
    return err.message || String(err);
  }
  return String(err);
}

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
export async function requestText(
  url: string,
  options: RequestOptions = {}
): Promise<string> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(url, {
      method: options.method ?? "GET",
      body: options.body,
      headers: options.headers,
      signal: controller.signal,
    });

    const text = await response.text();

    if (options.raw) {
      return text;
    }

    // Mirror original XHR client: only status 200 is success.
    if (response.status !== 200) {
      throw new Error(text || `HTTP ${response.status}`);
    }

    return text;
  } catch (err) {
    throw new Error(errorMessage(err));
  } finally {
    clearTimeout(timer);
  }
}

/**
 * fetch a URL as Blob (for loadFileFromUrl / convert URL resources).
 */
export async function requestBlob(
  url: string,
  options: { timeoutMs?: number } = {}
): Promise<Blob> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(url, {
      method: "GET",
      signal: controller.signal,
    });
    if (response.status !== 200) {
      const text = await response.text().catch(() => "");
      throw new Error(text || `HTTP ${response.status}`);
    }
    return await response.blob();
  } catch (err) {
    throw new Error(errorMessage(err));
  } finally {
    clearTimeout(timer);
  }
}

/** Safe JSON.parse that throws a clear Error. */
export function parseJson<T>(text: string): T {
  try {
    return JSON.parse(text) as T;
  } catch {
    throw new Error("Invalid JSON response from Browser Print agent");
  }
}

/** Invoke a success callback, swallowing consumer throws. */
export function invokeSuccess<T>(
  cb: ((value: T) => void) | undefined,
  value: T
): void {
  try {
    cb?.(value);
  } catch {
    // Consumer errors must not break the client.
  }
}

/** Invoke an error callback with a string message. */
export function invokeError(
  cb: ((error: string) => void) | undefined,
  fallback: ((error: string) => void) | undefined,
  error: unknown
): void {
  const message = errorMessage(error);
  const target = cb ?? fallback;
  try {
    if (target) {
      target(message);
    } else {
      console.error("BrowserPrint error (no errorCallback):", message);
    }
  } catch {
    // ignore
  }
}
