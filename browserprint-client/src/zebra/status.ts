/**
 * Parsers for Zebra host responses: `~HS` (status), `~HI` (info), `^HH` (configuration).
 * Field-based `~HS` parsing so `\n` vs `\r\n` line endings do not matter.
 */

export const STX = "\x02";
export const ETX = "\x03";

function hasFrameMarkers(text: string): boolean {
  return text.length > 1 && text.charAt(0) === STX && text.charAt(text.length - 1) === ETX;
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
