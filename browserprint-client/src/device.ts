import {
  invokeError,
  invokeSuccess,
  requestText,
} from "./http";
import type {
  BrowserPrintAPI,
  ConvertOptions,
  DeviceInfo,
  ErrorCallback,
  SuccessCallback,
} from "./types";

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
      void requestText(`${baseUrl}write`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          device: devicePayload(this),
          data,
        }),
      })
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
      const body: Record<string, unknown> = {
        device: devicePayload(this),
        url,
      };
      if (options != null) {
        body.options = options;
      }
      void requestText(`${baseUrl}write`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      })
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

      void requestText(`${baseUrl}write`, {
        method: "POST",
        body: form,
      })
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
      void requestText(`${baseUrl}read`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ device: devicePayload(this) }),
      })
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
