import type { BrowserPrintAPI } from "../types";
import { createPrinterClass } from "./printer";
import type { ZebraAPI, ZebraConfig } from "./types";
import { createWatchApi } from "./watch";

export function createZebra(api: BrowserPrintAPI, config: ZebraConfig = {}): ZebraAPI {
  const pollIntervalMs = config.pollIntervalMs ?? 2000;
  const Printer = createPrinterClass(api);
  const { watch, stopWatching } = createWatchApi(Printer, pollIntervalMs);
  return { Printer, watch, stopWatching };
}
