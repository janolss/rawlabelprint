/**
 * Zebra helper layer (`Zebra.Printer`, `Zebra.watch`, `Zebra.stopWatching`).
 *
 * Provenance: this is an independent implementation written for RawLabelPrint. It reproduces the
 * *public behaviour* of Zebra's `BrowserPrint-Zebra-1.1.250` (method names, argument order,
 * callback/Promise duality, result shapes) but contains no Zebra source code. Wire formats come
 * from Zebra's published printer command documentation (`~HS`, `~HI`, `^HH`, SGD `getvar`/`setvar`).
 *
 * Deliberate differences (all backwards compatible):
 * - `Status`/`Info`/`Configuration` are static members from the start (Zebra only defines them
 *   after the first `Printer` is constructed).
 * - `~HS` is parsed by comma-separated field, not absolute character offset, so `\n` vs `\r\n`
 *   line endings do not matter. Extra fields are exposed as additional properties.
 * - `Configuration` tolerates missing keys (NaN / empty string) instead of failing the request.
 * - A failed request never causes the next queued request to be skipped.
 * - `setSGD` goes through the request queue so it cannot interleave with a pending query.
 * - `isPrinterReady` forwards transport errors to the error callback.
 * - The background configuration load stops after a bounded number of attempts and can be
 *   disabled with `new Zebra.Printer(device, { autoLoadConfiguration: false })`.
 * - `watch` only polls while something is watched, never overlaps polls for one printer, and
 *   accepts a plain `Device`.
 * - Conversion helpers copy the options object instead of mutating the caller's.
 */
import type {
  BrowserPrintAPI,
  ConvertOptions,
  DeviceInfo,
  DeviceLike,
  ErrorCallback,
  SuccessCallback,
} from "./types";

const STX = "\x02";
const ETX = "\x03";

const CONVERT_FAILED = "Conversion is not supported by this Browser Print agent";

function hasFrameMarkers(text: string): boolean {
  return text.length > 1 && text.charAt(0) === STX && text.charAt(text.length - 1) === ETX;
}

function errorText(err: unknown): string {
  if (typeof err === "string") {
    return err;
  }
  if (err instanceof Error) {
    return err.message;
  }
  return String(err);
}

/** Splits `~HS` output into frames and then into trimmed comma-separated fields. */
function hsFrames(raw: string): string[][] {
  return raw
    .split(ETX)
    .map((frame) => frame.replace(/^[\s\x02]+/, "").trim())
    .filter((frame) => frame.length > 0)
    .map((frame) => frame.split(",").map((field) => field.trim()));
}

function flag(fields: string[] | undefined, index: number): boolean {
  return fields?.[index] === "1";
}

function intField(fields: string[] | undefined, index: number): number | undefined {
  const n = parseInt(fields?.[index] ?? "", 10);
  return Number.isNaN(n) ? undefined : n;
}

/** Result of `~HS`. Property names match Zebra's `Zebra.Printer.Status`. */
export class PrinterStatus {
  raw: string;
  offline = false;
  paperOut = false;
  paused = false;
  headOpen = false;
  ribbonOut = false;
  /** Additional `~HS` fields (not present in Zebra's class). */
  labelLengthDots?: number;
  formatsInBuffer?: number;
  bufferFull = false;
  partialFormatInProgress = false;
  corruptRam = false;
  underTemperature = false;
  overTemperature = false;
  labelsRemaining?: number;

  constructor(raw?: string) {
    this.raw = raw ?? "";
    const text = this.raw.trim();
    if (!hasFrameMarkers(text)) {
      this.offline = true;
      return;
    }
    const [line1, line2] = hsFrames(text);
    this.paperOut = flag(line1, 1);
    this.paused = flag(line1, 2);
    this.labelLengthDots = intField(line1, 3);
    this.formatsInBuffer = intField(line1, 4);
    this.bufferFull = flag(line1, 5);
    this.partialFormatInProgress = flag(line1, 7);
    this.corruptRam = flag(line1, 9);
    this.underTemperature = flag(line1, 10);
    this.overTemperature = flag(line1, 11);
    this.headOpen = flag(line2, 2);
    this.ribbonOut = flag(line2, 3);
    this.labelsRemaining = intField(line2, 8);
  }

  /** Zebra-compatible: true when the raw string has "1" at the given character offset. */
  isFlagSet(index: number): boolean {
    return this.raw.charAt(index) === "1";
  }

  isPrinterReady(): boolean {
    return !(this.paperOut || this.paused || this.headOpen || this.ribbonOut || this.offline);
  }

  getMessage(): string {
    if (this.isPrinterReady()) return "Ready";
    if (this.offline) return "Offline";
    if (this.paperOut) return "Paper Out";
    if (this.headOpen) return "Head Open";
    if (this.ribbonOut) return "Ribbon Out";
    if (this.paused) return "Paused";
    return "Ready";
  }
}

/** Result of `~HI`: `STX model,firmware,dpi-code,memory ETX`. */
export class PrinterInfo {
  raw: string;
  model: string;
  firmware: string;
  /** Remaining comma-separated fields (not present in Zebra's class). */
  extra: string[];

  constructor(raw: string) {
    if (!raw) {
      throw new Error("Invalid Response");
    }
    this.raw = raw;
    const text = raw.trim();
    if (!hasFrameMarkers(text)) {
      throw new Error("Invalid Response");
    }
    const parts = text.slice(1, -1).split(",");
    this.model = (parts[0] ?? "").trim();
    this.firmware = (parts[1] ?? "").trim();
    this.extra = parts.slice(2).map((p) => p.trim());
  }
}

/** Result of `^HH` (configuration label report). */
export class PrinterConfiguration {
  raw: string;
  settings: Record<string, string> = {};
  darkness: number;
  printSpeed: number;
  printWidth: number;
  labelLength: number;
  firmwareVersion: string;
  linkOSVersion: string;

  constructor(raw: string) {
    if (!raw) {
      throw new Error("Invalid Response");
    }
    const text = raw.trim();
    this.raw = text;
    if (!hasFrameMarkers(text)) {
      throw new Error("Invalid Response");
    }
    for (const rawLine of text.replace(STX, "").replace(ETX, "").split("\n")) {
      const line = rawLine.trim();
      if (line === "") continue;
      // Report lines are "<value padded to 20 columns><name>".
      let value = line.substring(0, 20).trim();
      let key = line.substring(20).trim();
      if (key === "") {
        const m = /^(.*?)\s{2,}(\S.*)$/.exec(line);
        if (!m) continue;
        value = m[1].trim();
        key = m[2].trim();
      }
      this.settings[key] = value;
    }
    const s = this.settings;
    this.darkness = parseFloat(s["DARKNESS"]);
    this.printSpeed = parseInt((s["PRINT SPEED"] ?? "").replace("IPS", "").trim(), 10);
    this.printWidth = parseInt(s["PRINT WIDTH"], 10);
    this.labelLength = parseInt(s["LABEL LENGTH"], 10);
    this.firmwareVersion = (s["FIRMWARE"] ?? "").replace("<-", "").trim();
    this.linkOSVersion = Object.prototype.hasOwnProperty.call(s, "LINK-OS VERSION")
      ? s["LINK-OS VERSION"]
      : "0";
  }
}

export interface PrinterOptions {
  /**
   * Load `^HH` in the background after construction so `printer.configuration` fills in
   * (Zebra does this unconditionally). Default true.
   */
  autoLoadConfiguration?: boolean;
}

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

/**
 * Calls `run` with callbacks when any callback is given, else returns a Promise.
 * Mirrors Zebra's "callbacks or Promise" convention.
 */
function dual<T>(
  run: (ok: (value: T) => void, fail: (reason: string) => void) => void,
  success?: SuccessCallback<T>,
  error?: ErrorCallback
): Promise<T> | undefined {
  if (!success && !error) {
    return new Promise<T>((resolve, reject) => run(resolve, reject));
  }
  run(
    (value) => success?.(value),
    (reason) => error?.(reason)
  );
  return undefined;
}

export interface ZebraAPI {
  Printer: ZebraPrinterConstructor;
  watch(
    device: DeviceInfo | DeviceLike,
    onchange: (previous: PrinterStatus | "", current: PrinterStatus) => void,
    errorsForOffline?: number
  ): void;
  stopWatching(device: DeviceInfo | DeviceLike): void;
}

export interface ZebraPrinterConstructor {
  new (info: DeviceInfo, options?: PrinterOptions): ZebraPrinter;
  Status: typeof PrinterStatus;
  Info: typeof PrinterInfo;
  Configuration: typeof PrinterConfiguration;
}

export interface ZebraPrinter extends DeviceLike {
  configuration?: PrinterConfiguration;
  clearRequestQueue(): void;
  getStatus(): Promise<PrinterStatus>;
  getStatus(ok: SuccessCallback<PrinterStatus>, fail?: ErrorCallback): void;
  isPrinterReady(): Promise<string>;
  isPrinterReady(ok: SuccessCallback<string>, fail?: ErrorCallback): void;
  getInfo(): Promise<PrinterInfo>;
  getInfo(ok: SuccessCallback<PrinterInfo>, fail?: ErrorCallback): void;
  getConfiguration(): Promise<PrinterConfiguration>;
  getConfiguration(ok: SuccessCallback<PrinterConfiguration>, fail?: ErrorCallback): void;
  getSGD(name: string): Promise<string>;
  getSGD(name: string, ok: SuccessCallback<string>, fail?: ErrorCallback): void;
  setSGD(name: string, value: string): Promise<string>;
  setSGD(name: string, value: string, ok: SuccessCallback<string>, fail?: ErrorCallback): void;
  setThenGetSGD(name: string, value: string): Promise<string>;
  setThenGetSGD(
    name: string,
    value: string,
    ok: SuccessCallback<string>,
    fail?: ErrorCallback
  ): void;
  query(command: string): Promise<string>;
  query(command: string, ok: SuccessCallback<string>, fail?: ErrorCallback): void;
  printImageAsLabel(resource: string | Blob, options?: ConvertOptions): Promise<unknown>;
  printImageAsLabel(
    resource: string | Blob,
    options: ConvertOptions | undefined,
    ok: SuccessCallback<unknown>,
    fail?: ErrorCallback
  ): void;
  getConvertedResource(resource: string | Blob, options?: ConvertOptions): Promise<unknown>;
  getConvertedResource(
    resource: string | Blob,
    options: ConvertOptions | undefined,
    ok: SuccessCallback<unknown>,
    fail?: ErrorCallback
  ): void;
  storeConvertedResource(resource: string | Blob, options?: ConvertOptions): Promise<unknown>;
  storeConvertedResource(
    resource: string | Blob,
    options: ConvertOptions | undefined,
    ok: SuccessCallback<unknown>,
    fail?: ErrorCallback
  ): void;
}

export interface ZebraConfig {
  /** Status polling interval used by `Zebra.watch` (default 2000 ms). */
  pollIntervalMs?: number;
}

export function createZebra(api: BrowserPrintAPI, config: ZebraConfig = {}): ZebraAPI {
  const pollIntervalMs = config.pollIntervalMs ?? 2000;
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
        fail
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
        fail
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
        fail
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
        fail
      );
    }

    getSGD(name: string, ok?: SuccessCallback<string>, fail?: ErrorCallback): Promise<string> | undefined {
      return dual<string>(
        (resolve, reject) => {
          this.enqueue("sgd", `! U1 getvar "${name}"\r\n`).then((v) => resolve(v as string), reject);
        },
        ok,
        fail
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
          this.enqueue("set", `! U1 setvar "${name}" "${value}"\r\n`).then(
            (v) => resolve(v as string),
            reject
          );
        },
        ok,
        fail
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
        fail
      );
    }

    query(command: string, ok?: SuccessCallback<string>, fail?: ErrorCallback): Promise<string> | undefined {
      return dual<string>(
        (resolve, reject) => {
          this.enqueue("query", command).then((v) => resolve(v as string), reject);
        },
        ok,
        fail
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
              (opts as Record<string, unknown>).fitTo = {
                width: cfg.printWidth,
                height: cfg.labelLength,
              };
            }
            api.convert(resource, this, opts, resolve, (e) => reject(e || CONVERT_FAILED));
          }, reject);
        },
        ok,
        fail
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

  type Watcher = {
    printer: ZebraPrinter;
    previous: PrinterStatus | "";
    onchange: (previous: PrinterStatus | "", current: PrinterStatus) => void;
    errors: number;
    errorsForOffline: number;
    inFlight: boolean;
  };
  const watchers = new Map<string, Watcher>();
  let timer: ReturnType<typeof setInterval> | undefined;

  function keyOf(device: DeviceInfo | DeviceLike): string {
    return device.uid ?? `${device.name ?? ""}|${device.connection ?? ""}`;
  }

  function deliver(w: Watcher, key: string, status: PrinterStatus): void {
    if (watchers.get(key) !== w) return;
    if (status.offline) {
      w.errors += 1;
      // Tolerate short outages before reporting "offline".
      if (w.errors < w.errorsForOffline) return;
    } else {
      w.errors = 0;
    }
    const prev = w.previous;
    w.previous = status;
    if (prev === "" || prev.raw !== status.raw || prev.offline !== status.offline) {
      try {
        w.onchange(prev, status);
      } catch {
        // A throwing consumer must not stop polling.
      }
    }
  }

  function poll(): void {
    for (const [key, w] of watchers) {
      if (w.inFlight) continue;
      w.inFlight = true;
      w.printer.getStatus()!.then(
        (status) => {
          w.inFlight = false;
          deliver(w, key, status);
        },
        () => {
          w.inFlight = false;
          deliver(w, key, new PrinterStatus(""));
        }
      );
    }
  }

  const zebra: ZebraAPI = {
    Printer: Printer as unknown as ZebraPrinterConstructor,
    watch(device, onchange, errorsForOffline = 2) {
      const printer =
        device instanceof Printer
          ? (device as unknown as ZebraPrinter)
          : (new Printer(device, { autoLoadConfiguration: false }) as unknown as ZebraPrinter);
      watchers.set(keyOf(device), {
        printer,
        previous: "",
        onchange,
        errors: 0,
        errorsForOffline,
        inFlight: false,
      });
      if (timer === undefined) {
        timer = setInterval(poll, pollIntervalMs);
        (timer as { unref?: () => void }).unref?.();
      }
    },
    stopWatching(device) {
      watchers.delete(keyOf(device));
      if (watchers.size === 0 && timer !== undefined) {
        clearInterval(timer);
        timer = undefined;
      }
    },
  };
  return zebra;
}
