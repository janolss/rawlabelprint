/**
 * Zebra.watch / stopWatching: polls only while something is watched,
 * never overlaps polls for one printer, and accepts a plain Device.
 */
import { deviceKey } from "../device";
import type { DeviceInfo, DeviceLike } from "../types";
import { PrinterStatus } from "./status";
import type { ZebraAPI, ZebraPrinter, ZebraPrinterConstructor } from "./types";

type Watcher = {
  printer: ZebraPrinter;
  previous: PrinterStatus | "";
  onchange: (previous: PrinterStatus | "", current: PrinterStatus) => void;
  errors: number;
  errorsForOffline: number;
  inFlight: boolean;
};

export function createWatchApi(
  Printer: ZebraPrinterConstructor,
  pollIntervalMs: number
): Pick<ZebraAPI, "watch" | "stopWatching"> {
  const watchers = new Map<string, Watcher>();
  let timer: ReturnType<typeof setInterval> | undefined;

  function keyOf(device: DeviceInfo | DeviceLike): string {
    return deviceKey(device);
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

  return {
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
}
