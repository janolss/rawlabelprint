import {
  assertUrlAllowed,
  invokeError,
  invokeSuccess,
  requestText,
  withDebug,
} from "./http";
import type {
  BrowserPrintAPI,
  ConvertOptions,
  DeviceInfo,
  DeviceLike,
  ErrorCallback,
  SuccessCallback,
} from "./types";

/** Cap for accumulated read data in readUntilStringReceived / readAllAvailable. */
export const MAX_READ_ACCUMULATED_BYTES = 2 * 1024 * 1024;

const textEncoder = new TextEncoder();

/** Stable key for read intervals and watch maps. */
export function deviceKey(device: DeviceInfo | DeviceLike): string {
  if (device.uid && device.uid.length > 0) {
    return device.uid;
  }
  return `${device.name ?? ""}|${device.connection ?? ""}|${device.deviceType ?? ""}`;
}

/** Validate agent JSON before hydrating a Device instance. */
export function asDeviceInfo(item: unknown): DeviceInfo {
  if (item == null || typeof item !== "object" || Array.isArray(item)) {
    throw new Error("Invalid device info from Browser Print agent");
  }
  const raw = item as Record<string, unknown>;
  const stringFields = [
    "name",
    "deviceType",
    "connection",
    "uid",
    "provider",
    "manufacturer",
  ] as const;
  for (const key of stringFields) {
    const value = raw[key];
    if (value !== undefined && typeof value !== "string") {
      throw new Error(`Invalid device info field: ${key}`);
    }
  }
  if (raw.version !== undefined && typeof raw.version !== "number") {
    throw new Error("Invalid device info field: version");
  }
  return {
    name: raw.name as string | undefined,
    deviceType: raw.deviceType as string | undefined,
    connection: raw.connection as string | undefined,
    uid: raw.uid as string | undefined,
    version: raw.version as number | undefined,
    provider: raw.provider as string | undefined,
    manufacturer: raw.manufacturer as string | undefined,
  };
}

/** Serializable device payload for /write and /read. */
export function devicePayload(device: {
  name?: string;
  uid?: string;
  connection?: string;
  deviceType?: string;
  version?: number;
  provider?: string;
  manufacturer?: string;
}): Record<string, unknown> {
  return {
    name: device.name,
    uid: device.uid,
    connection: device.connection,
    deviceType: device.deviceType,
    version: device.version,
    provider: device.provider,
    manufacturer: device.manufacturer,
  };
}

export function createDeviceClass(
  api: BrowserPrintAPI,
  baseUrl: string
): new (info: DeviceInfo) => InstanceType<BrowserPrintAPI["Device"]> {
  return class Device {
    name?: string;
    deviceType?: string;
    connection?: string;
    uid?: string;
    version: number;
    provider?: string;
    manufacturer?: string;
    readRetries: number;

    sendErrorCallback: ErrorCallback = () => {};
    sendFinishedCallback: SuccessCallback = () => {};
    readErrorCallback: ErrorCallback = () => {};
    readFinishedCallback: SuccessCallback = () => {};

    constructor(info: DeviceInfo) {
      this.name = info.name;
      this.deviceType = info.deviceType;
      this.connection = info.connection;
      this.uid = info.uid;
      this.version = info.version ?? 2;
      this.provider = info.provider;
      this.manufacturer = info.manufacturer;
      this.readRetries =
        this.connection === "bluetooth" ? 1 : 0;
    }

    send(
      data: string,
      finished?: SuccessCallback,
      error?: ErrorCallback
    ): void {
      const success = finished ?? this.sendFinishedCallback;
      const failure = error ?? this.sendErrorCallback;
      void requestText(
        `${baseUrl}write`,
        withDebug(api.debug, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            device: devicePayload(this),
            data,
          }),
        })
      )
        .then((text) => invokeSuccess(success, text))
        .catch((err) => invokeError(failure, api.defaultErrorCallback, err));
    }

    sendUrl(
      url: string,
      finished?: SuccessCallback,
      error?: ErrorCallback,
      options?: Record<string, unknown>
    ): void {
      const success = finished ?? this.sendFinishedCallback;
      const failure = error ?? this.sendErrorCallback;
      try {
        assertUrlAllowed(url, api.allowedUrlOrigins);
      } catch (err) {
        invokeError(failure, api.defaultErrorCallback, err);
        return;
      }
      const body: Record<string, unknown> = {
        device: devicePayload(this),
        url,
      };
      if (options != null) {
        body.options = options;
      }
      void requestText(
        `${baseUrl}write`,
        withDebug(api.debug, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        })
      )
        .then((text) => invokeSuccess(success, text))
        .catch((err) => invokeError(failure, api.defaultErrorCallback, err));
    }

    sendFile(
      resource: string | Blob,
      finished?: SuccessCallback,
      error?: ErrorCallback
    ): void {
      if (typeof resource === "string") {
        api.loadFileFromUrl(
          resource,
          (blob) => this.sendFile(blob, finished, error),
          error
        );
        return;
      }

      const success = finished ?? api.defaultSuccessCallback;
      const failure = error ?? api.defaultErrorCallback;
      const form = new FormData();
      form.append("json", JSON.stringify({ device: devicePayload(this) }));
      form.append("blob", resource);

      void requestText(
        `${baseUrl}write`,
        withDebug(api.debug, {
          method: "POST",
          body: form,
        })
      )
        .then((text) => invokeSuccess(success, text))
        .catch((err) => invokeError(failure, api.defaultErrorCallback, err));
    }

    convertAndSendFile(
      resource: string | Blob,
      finished?: SuccessCallback,
      error?: ErrorCallback,
      options?: ConvertOptions
    ): void {
      const opts: ConvertOptions = { ...(options ?? {}) };
      if (!opts.action) {
        opts.action = "print";
      }
      // Zebra's convertAndSendFile forwards the convert success payload as-is (often a JSON object).
      api.convert(
        resource,
        this,
        opts,
        finished as SuccessCallback<unknown> | undefined,
        error
      );
    }

    read(finished?: SuccessCallback, error?: ErrorCallback): void {
      const success = finished ?? this.readFinishedCallback;
      const failure = error ?? this.readErrorCallback;
      void requestText(
        `${baseUrl}read`,
        withDebug(api.debug, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ device: devicePayload(this) }),
        })
      )
        .then((text) => invokeSuccess(success, text))
        .catch((err) => invokeError(failure, api.defaultErrorCallback, err));
    }

    readUntilStringReceived(
      needle: string,
      finished?: SuccessCallback,
      error?: ErrorCallback,
      retries?: number,
      accumulated = ""
    ): void {
      const remaining = retries ?? this.readRetries;
      const success = finished ?? this.readFinishedCallback;
      const failure = error ?? this.readErrorCallback;

      this.read(
        (chunk) => {
          let retriesLeft = remaining;
          if (chunk && chunk.length !== 0) {
            retriesLeft = 0;
          } else if (retriesLeft <= 0) {
            invokeSuccess(success, accumulated);
            return;
          }

          const next = accumulated + chunk;
          if (textEncoder.encode(next).byteLength > MAX_READ_ACCUMULATED_BYTES) {
            invokeError(
              failure,
              api.defaultErrorCallback,
              "Read buffer exceeded maximum size"
            );
            return;
          }
          if (needle !== "" && next.includes(needle)) {
            invokeSuccess(success, next);
          } else {
            this.readUntilStringReceived(
              needle,
              success,
              failure,
              retriesLeft - 1,
              next
            );
          }
        },
        failure
      );
    }

    readAllAvailable(
      finished?: SuccessCallback,
      error?: ErrorCallback,
      retries?: number
    ): void {
      this.readUntilStringReceived("", finished, error, retries);
    }

    sendThenRead(
      data: string,
      finished?: SuccessCallback,
      error?: ErrorCallback
    ): void {
      this.send(
        data,
        () => {
          this.read(finished, error);
        },
        error
      );
    }

    sendThenReadUntilStringReceived(
      data: string,
      needle: string,
      finished?: SuccessCallback,
      error?: ErrorCallback,
      retries?: number
    ): void {
      this.send(
        data,
        () => {
          this.readUntilStringReceived(needle, finished, error, retries);
        },
        error
      );
    }

    sendThenReadAllAvailable(
      data: string,
      finished?: SuccessCallback,
      error?: ErrorCallback,
      retries?: number
    ): void {
      this.send(
        data,
        () => {
          this.readUntilStringReceived("", finished, error, retries);
        },
        error
      );
    }
  };
}
