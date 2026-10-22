/**
 * Zebra.Printer: request queue, SGD, convert helpers.
 * setSGD is queued so it cannot interleave with a pending query.
 */
import type {
  BrowserPrintAPI,
  ConvertOptions,
  DeviceInfo,
  DeviceLike,
  ErrorCallback,
  SuccessCallback,
} from "../types";
import { assertSgdToken, CONVERT_FAILED, dual, errorText } from "./helpers";
import {
  ETX,
  PrinterConfiguration,
  PrinterInfo,
  PrinterStatus,
} from "./status";
import type { PrinterOptions, ZebraPrinterConstructor } from "./types";

type Waiter = { resolve: (value: unknown) => void; reject: (reason: string) => void };

type QueueItem = {
  kind: "status" | "info" | "config" | "sgd" | "query" | "set";
  command: string;
  parse?: (raw: string) => unknown;
  waiters: Waiter[];
  started: boolean;
};

const CONFIG_LOAD_MAX_ATTEMPTS = 5;
const CONFIG_LOAD_BASE_DELAY_MS = 1000;

export function createPrinterClass(api: BrowserPrintAPI): ZebraPrinterConstructor {
  const BaseDevice = api.Device as new (info: DeviceInfo) => DeviceLike;

  class Printer extends BaseDevice {
    static Status = PrinterStatus;
    static Info = PrinterInfo;
    static Configuration = PrinterConfiguration;

    configuration?: PrinterConfiguration;
    private queue: QueueItem[] = [];

    constructor(info: DeviceInfo, options: PrinterOptions = {}) {
      super(info);
      if (options.autoLoadConfiguration !== false) {
        this.loadConfigurationInBackground(1);
      }
    }

    private loadConfigurationInBackground(attempt: number): void {
      if (this.configuration) return;
      this.getConfiguration()!.catch(() => {
        if (attempt >= CONFIG_LOAD_MAX_ATTEMPTS) return;
        const timer = setTimeout(
          () => this.loadConfigurationInBackground(attempt + 1),
          CONFIG_LOAD_BASE_DELAY_MS * 2 ** (attempt - 1)
        );
        (timer as { unref?: () => void }).unref?.();
      });
    }

    /** Drops queued requests; a request already on the wire still completes. */
    clearRequestQueue(): void {
      const running = this.queue[0]?.started ? this.queue[0] : undefined;
      for (const item of this.queue) {
        if (item !== running) {
          item.waiters.forEach((w) => w.reject("Request cancelled"));
        }
      }
      this.queue = running ? [running] : [];
    }

    private enqueue(
      kind: QueueItem["kind"],
      command: string,
      parse?: (raw: string) => unknown
    ): Promise<unknown> {
      return new Promise((resolve, reject) => {
        const waiter: Waiter = { resolve, reject };
        // Concurrent status requests share one `~HS` round trip.
        const pending =
          kind === "status" ? this.queue.find((i) => i.kind === "status" && !i.started) : undefined;
        if (pending) {
          pending.waiters.push(waiter);
          return;
        }
        this.queue.push({ kind, command, parse, waiters: [waiter], started: false });
        this.pump();
      });
    }

    private pump(): void {
      const item = this.queue[0];
      if (!item || item.started) return;
      item.started = true;

      let settled = false;
      const finish = (outcome: { value: unknown } | { error: string }): void => {
        if (settled) return;
        settled = true;
        // Remove by identity: the queue may have been cleared meanwhile.
        const idx = this.queue.indexOf(item);
        if (idx >= 0) this.queue.splice(idx, 1);
        for (const w of item.waiters) {
          if ("error" in outcome) w.reject(outcome.error);
          else w.resolve(outcome.value);
        }
        this.pump();
      };
      const onReply = (raw: string): void => {
        try {
          finish({ value: item.parse ? item.parse(raw) : raw });
        } catch (err) {
          finish({ error: errorText(err) });
        }
      };
      const onFail = (reason: string): void => finish({ error: reason });

      if (item.kind === "set") {
        this.send(item.command, onReply, onFail);
      } else if (item.kind === "status" || item.kind === "info" || item.kind === "config") {
        this.sendThenReadUntilStringReceived(item.command, ETX, onReply, onFail);
      } else {
        this.sendThenReadAllAvailable(item.command, onReply, onFail);
      }
    }

    getStatus(ok?: SuccessCallback<PrinterStatus>, fail?: ErrorCallback): Promise<PrinterStatus> | undefined {
      return dual<PrinterStatus>(
        (resolve, reject) => {
          // A missing or garbled reply means "offline", not a failed request.
          this.enqueue("status", "~hs\r\n", (raw) => new PrinterStatus(raw)).then(
            (v) => resolve(v as PrinterStatus),
            reject
          );
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    isPrinterReady(ok?: SuccessCallback<string>, fail?: ErrorCallback): Promise<string> | undefined {
      return dual<string>(
        (resolve, reject) => {
          this.getStatus()!.then(
            (status) =>
              status.isPrinterReady() ? resolve(status.getMessage()) : reject(status.getMessage()),
            reject
          );
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    getInfo(ok?: SuccessCallback<PrinterInfo>, fail?: ErrorCallback): Promise<PrinterInfo> | undefined {
      return dual<PrinterInfo>(
        (resolve, reject) => {
          this.enqueue("info", "~hi\r\n", (raw) => new PrinterInfo(raw)).then(
            (v) => resolve(v as PrinterInfo),
            reject
          );
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    getConfiguration(
      ok?: SuccessCallback<PrinterConfiguration>,
      fail?: ErrorCallback
    ): Promise<PrinterConfiguration> | undefined {
      return dual<PrinterConfiguration>(
        (resolve, reject) => {
          this.enqueue("config", "^XA^HH^XZ", (raw) => {
            const parsed = new PrinterConfiguration(raw);
            this.configuration = parsed;
            return parsed;
          }).then((v) => resolve(v as PrinterConfiguration), reject);
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    getSGD(name: string, ok?: SuccessCallback<string>, fail?: ErrorCallback): Promise<string> | undefined {
      return dual<string>(
        (resolve, reject) => {
          try {
            assertSgdToken(name, "name");
          } catch (err) {
            reject(errorText(err));
            return;
          }
          this.enqueue("sgd", `! U1 getvar "${name}"\r\n`).then((v) => resolve(v as string), reject);
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    setSGD(
      name: string,
      value: string,
      ok?: SuccessCallback<string>,
      fail?: ErrorCallback
    ): Promise<string> | undefined {
      return dual<string>(
        (resolve, reject) => {
          try {
            assertSgdToken(name, "name");
            assertSgdToken(value, "value");
          } catch (err) {
            reject(errorText(err));
            return;
          }
          this.enqueue("set", `! U1 setvar "${name}" "${value}"\r\n`).then(
            (v) => resolve(v as string),
            reject
          );
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    setThenGetSGD(
      name: string,
      value: string,
      ok?: SuccessCallback<string>,
      fail?: ErrorCallback
    ): Promise<string> | undefined {
      return dual<string>(
        (resolve, reject) => {
          this.setSGD(name, value)!.then(() => this.getSGD(name)!.then(resolve, reject), reject);
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    query(command: string, ok?: SuccessCallback<string>, fail?: ErrorCallback): Promise<string> | undefined {
      return dual<string>(
        (resolve, reject) => {
          this.enqueue("query", command).then((v) => resolve(v as string), reject);
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    private async ensureConfiguration(): Promise<PrinterConfiguration> {
      return this.configuration ?? (await this.getConfiguration()!);
    }

    private convertWith(
      action: "print" | "return" | "store",
      resource: string | Blob,
      options: ConvertOptions | undefined,
      ok?: SuccessCallback<unknown>,
      fail?: ErrorCallback
    ): Promise<unknown> | undefined {
      return dual<unknown>(
        (resolve, reject) => {
          this.ensureConfiguration().then((cfg) => {
            const opts: ConvertOptions = { ...(options ?? {}), action };
            if (action === "print") {
              if (
                !Number.isFinite(cfg.printWidth) ||
                cfg.printWidth <= 0 ||
                !Number.isFinite(cfg.labelLength) ||
                cfg.labelLength <= 0
              ) {
                reject("Printer configuration incomplete: printWidth/labelLength");
                return;
              }
              (opts as Record<string, unknown>).fitTo = {
                width: cfg.printWidth,
                height: cfg.labelLength,
              };
            }
            api.convert(resource, this, opts, resolve, (e) => reject(e || CONVERT_FAILED));
          }, reject);
        },
        ok,
        fail,
        api.defaultErrorCallback
      );
    }

    printImageAsLabel(
      resource: string | Blob,
      options?: ConvertOptions,
      ok?: SuccessCallback<unknown>,
      fail?: ErrorCallback
    ): Promise<unknown> | undefined {
      return this.convertWith("print", resource, options, ok, fail);
    }

    getConvertedResource(
      resource: string | Blob,
      options?: ConvertOptions,
      ok?: SuccessCallback<unknown>,
      fail?: ErrorCallback
    ): Promise<unknown> | undefined {
      return this.convertWith("return", resource, options, ok, fail);
    }

    storeConvertedResource(
      resource: string | Blob,
      options?: ConvertOptions,
      ok?: SuccessCallback<unknown>,
      fail?: ErrorCallback
    ): Promise<unknown> | undefined {
      return this.convertWith("store", resource, options, ok, fail);
    }
  }

  return Printer as unknown as ZebraPrinterConstructor;
}
