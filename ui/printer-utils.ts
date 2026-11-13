/** Shared printer display helpers for Settings UI. */

import type { PrinterInfo } from "./types.ts";

export function isUsbPrinter(p: PrinterInfo): boolean {
  return (p.connection || "network").toLowerCase() === "usb";
}

export function isVirtualPrinter(p: PrinterInfo): boolean {
  return (p.connection || "").toLowerCase() === "virtual";
}

/** Shorten discovery model strings for default display names.
 * e.g. "ZTC ZD421-203dpi ZPL112923" → "Zebra ZD421". */
export function friendlyModelLabel(model: string): string {
  return model
    .split(/\s+/)
    .filter(Boolean)
    .flatMap((token) => {
      if (token.length >= 3 && token.slice(0, 3).toLowerCase() === "zpl") {
        return [];
      }
      if (token.toLowerCase() === "ztc") return ["Zebra"];
      const dash = token.indexOf("-");
      return [dash >= 0 ? token.slice(0, dash) : token];
    })
    .join(" ");
}

export function displayName(p: PrinterInfo): string {
  if (p.name && p.name.trim()) return p.name;
  const label = friendlyModelLabel(p.model || "");
  if (label) return label;
  return p.address;
}
