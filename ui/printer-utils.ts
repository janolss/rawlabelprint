/** Shared printer display helpers for Settings UI. */

import type { PrinterInfo } from "./types.ts";

export function displayName(p: PrinterInfo): string {
  if (p.name && p.name.trim()) return p.name;
  if (p.model) return `${p.model} (${p.address})`;
  return p.address;
}
