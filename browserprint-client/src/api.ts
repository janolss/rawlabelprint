import { asDeviceInfo, createDeviceClass, deviceKey, devicePayload } from "./device";
import {
  assertUrlAllowed,
  invokeError,
  invokeSuccess,
  parseJson,
  requestBlob,
  requestText,
  resolveBaseUrl,
  withDebug,
} from "./http";
import type {
  ApplicationConfigurationData,
  BindableField,
  BrowserPrintAPI,
  ConvertOptions,
  DeviceLike,
  ErrorCallback,
  LocalDevicesMap,
  ScanOptions,
  SuccessCallback,
} from "./types";

function extensionHint(url: string): string {
  if (url.length < 3) {
    return "";
  }
  return url.substring(url.length - 3);
}

function mimeToFormat(type: string): string {
  return type
    .toLowerCase()
    .replace("image/", "")
    .replace("application/", "")
    .replace("x-ms-", "");
}

export type CreateBrowserPrintOptions = {
  baseUrl?: string;
  debug?: boolean;
  /** Opt-in origin allowlist for URL fetches / sendUrl. Null/empty = unrestricted. */
  allowedUrlOrigins?: string[] | null;
};

export function createBrowserPrint(
  baseUrlOrOptions: string | CreateBrowserPrintOptions = {}
): BrowserPrintAPI {
  const options: CreateBrowserPrintOptions =
    typeof baseUrlOrOptions === "string"
      ? { baseUrl: baseUrlOrOptions }
      : baseUrlOrOptions;
  const baseUrl = options.baseUrl ?? resolveBaseUrl();
  const api = {} as BrowserPrintAPI;
  api.debug = options.debug ?? false;
  api.allowedUrlOrigins = options.allowedUrlOrigins ?? null;
  const readIntervals = new Map<
    string,
    { stopped: boolean; timer?: ReturnType<typeof setTimeout> }
  >();

  api.defaultSuccessCallback = () => {};
  api.defaultErrorCallback = () => {};

  api.ApplicationConfiguration = class ApplicationConfiguration
    implements ApplicationConfigurationData
  {
    application = {
      version: "1.2.0.3",
      build_number: 3,
      api_level: 2,
      platform: "",
      supportedConversions: {},
    };
  };

  api.Device = createDeviceClass(api, baseUrl);

  api.getLocalDevices = (
    finished: SuccessCallback<LocalDevicesMap | DeviceLike[]>,
    error?: ErrorCallback,
    deviceType?: string
  ): void => {
    void requestText(`${baseUrl}available`, withDebug(api.debug, { method: "GET" }))
      .then((text) => {
        const response = parseJson<Record<string, unknown>>(text);
        for (const key of Object.keys(response)) {
          const value = response[key];
          if (Array.isArray(value)) {
            response[key] = value.map((item) => new api.Device(asDeviceInfo(item)));
          }
        }
        if (deviceType === undefined) {
          invokeSuccess(finished, response as LocalDevicesMap);
        } else {
          const list = Array.isArray(response[deviceType])
            ? (response[deviceType] as DeviceLike[])
            : [];
          invokeSuccess(finished, list);
        }
      })
      .catch((err) =>
        invokeError(error, api.defaultErrorCallback, err)
      );
  };

  api.getDefaultDevice = (
    deviceType: string | null | undefined,
    finished: SuccessCallback<DeviceLike | null>,
    error?: ErrorCallback
  ): void => {
    let path = "default";
    if (deviceType !== undefined && deviceType !== null) {
      path = `default?type=${encodeURIComponent(deviceType)}`;
    }
    void requestText(`${baseUrl}${path}`, withDebug(api.debug, { method: "GET" }))
      .then((text) => {
        if (text === "") {
          invokeSuccess(finished, null);
          return;
        }
        const info = asDeviceInfo(parseJson<unknown>(text));
        invokeSuccess(finished, new api.Device(info));
      })
      .catch((err) =>
        invokeError(error, api.defaultErrorCallback, err)
      );
  };

  api.getApplicationConfiguration = (
    finished: SuccessCallback<ApplicationConfigurationData | null>,
    error?: ErrorCallback
  ): void => {
    void requestText(`${baseUrl}config`, withDebug(api.debug, { method: "GET" }))
      .then((text) => {
        if (text === "") {
          invokeSuccess(finished, null);
          return;
        }
        invokeSuccess(finished, parseJson<ApplicationConfigurationData>(text));
      })
      .catch((err) =>
        invokeError(error, api.defaultErrorCallback, err)
      );
  };

  api.readOnInterval = (
    device: DeviceLike,
    finished: SuccessCallback,
    intervalMs?: number
  ): void => {
    let delay = intervalMs;
    if (delay === undefined || delay === 0) {
      delay = 1;
    }
    const key = deviceKey(device);
    api.stopReadOnInterval(device);
    const interval = { stopped: false } as {
      stopped: boolean;
      timer?: ReturnType<typeof setTimeout>;
    };

    const tick = (): void => {
      if (interval.stopped) return;
      device.read(
        (data) => {
          if (interval.stopped || readIntervals.get(key) !== interval) return;
          invokeSuccess(finished, data);
          interval.timer = setTimeout(tick, delay);
        },
        () => {
          if (interval.stopped || readIntervals.get(key) !== interval) return;
          interval.timer = setTimeout(tick, delay);
        }
      );
    };

    readIntervals.set(key, interval);
    interval.timer = setTimeout(tick, delay);
  };

  api.stopReadOnInterval = (device: DeviceLike): void => {
    const key = deviceKey(device);
    const interval = readIntervals.get(key);
    if (interval !== undefined) {
      interval.stopped = true;
      if (interval.timer !== undefined) {
        clearTimeout(interval.timer);
      }
      readIntervals.delete(key);
    }
  };

  api.bindFieldToReadData = (
    device: DeviceLike,
    field: BindableField,
    intervalMs?: number,
    onUpdate?: () => void
  ): void => {
    api.readOnInterval(
      device,
      (data) => {
        if (data !== "") {
          field.value = data;
          if (onUpdate != null) {
            onUpdate();
          }
        }
      },
      intervalMs
    );
  };

  api.loadFileFromUrl = (
    url: string,
    finished?: SuccessCallback<Blob>,
    error?: ErrorCallback
  ): void => {
    try {
      assertUrlAllowed(url, api.allowedUrlOrigins);
    } catch (err) {
      invokeError(error, api.defaultErrorCallback, err);
      return;
    }
    void requestBlob(url, { debug: api.debug })
      .then((blob) => {
        if (finished) {
          invokeSuccess(finished, blob);
        }
      })
      .catch((err) =>
        invokeError(error, api.defaultErrorCallback, err)
      );
  };

  api.convert = (
    resource: string | Blob | null | undefined,
    device: DeviceLike | null | undefined,
    options: ConvertOptions | null | undefined,
    finished?: SuccessCallback<unknown>,
    error?: ErrorCallback
  ): void => {
    if (!resource) {
      invokeError(
        error,
        api.defaultErrorCallback,
        "Resource not specified"
      );
      return;
    }

    if (typeof resource === "string") {
      const opts: ConvertOptions = { ...(options ?? {}) };
      api.loadFileFromUrl(
        resource,
        (blob) => {
          if (!opts.fromFormat) {
            opts.fromFormat = extensionHint(resource);
          }
          api.convert(blob, device, opts, finished, error);
        },
        error
      );
      return;
    }

    const opts: ConvertOptions = { ...(options ?? {}) };
    if (
      resource.type &&
      (resource.type.startsWith("image/") ||
        resource.type.startsWith("application/"))
    ) {
      opts.fromFormat = mimeToFormat(resource.type);
    }

    const meta: Record<string, unknown> = {};
    if (opts != null) {
      meta.options = opts;
    }
    if (device) {
      meta.device = devicePayload(device);
    }

    const form = new FormData();
    form.append("json", JSON.stringify(meta));
    form.append("blob", resource);

    void requestText(
      `${baseUrl}convert`,
      withDebug(api.debug, { method: "POST", body: form })
    )
      .then((text) => {
        if (finished) {
          invokeSuccess(finished, parseJson<unknown>(text));
        }
      })
      .catch((err) =>
        invokeError(error, api.defaultErrorCallback, err)
      );
  };

  api.scanImage = (
    resource: string | Blob | null | undefined,
    options: ScanOptions | null | undefined,
    finished?: SuccessCallback<unknown>,
    error?: ErrorCallback
  ): void => {
    if (!resource) {
      invokeError(
        error,
        api.defaultErrorCallback,
        "Resource not specified"
      );
      return;
    }

    if (typeof resource === "string") {
      const opts: ScanOptions = { ...(options ?? {}) };
      api.loadFileFromUrl(
        resource,
        (blob) => {
          if (!opts.format) {
            opts.format = extensionHint(resource);
          }
          api.scanImage(blob, opts, finished, error);
        },
        error
      );
      return;
    }

    const opts: ScanOptions = { ...(options ?? {}) };
    if (
      resource.type &&
      (resource.type.startsWith("image/") ||
        resource.type.startsWith("application/"))
    ) {
      opts.format = mimeToFormat(resource.type);
    }

    const form = new FormData();
    form.append("json", JSON.stringify({ options: opts }));
    form.append("blob", resource);

    void requestText(
      `${baseUrl}convert/scan`,
      withDebug(api.debug, { method: "POST", body: form })
    )
      .then((text) => {
        if (finished) {
          invokeSuccess(finished, parseJson<unknown>(text));
        }
      })
      .catch((err) =>
        invokeError(error, api.defaultErrorCallback, err)
      );
  };

  return api;
}
