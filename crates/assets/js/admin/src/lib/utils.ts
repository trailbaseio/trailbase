import type { ClassValue } from "clsx";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import { stringify as uuidStringify } from "uuid";
import { urlSafeBase64Decode } from "trailbase";

import { showToast } from "@/components/ui/toast";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function pathJoin(parts: string[], sep?: string): string {
  const separator = sep ?? "/";
  const replace = new RegExp(`${separator}{1,}`, "g");
  return parts.join(separator).replace(replace, separator);
}

export function copyToClipboard(contents: string) {
  navigator.clipboard.writeText(contents);
  showToast({
    title: "Copied to clipboard",
  });
}

export function tryParseInt(value: string): number | undefined {
  const n = parseInt(value.trim());
  return isNaN(n) ? undefined : n;
}

export function tryParseBigInt(value: string): bigint | undefined {
  if (value === "") {
    return undefined;
  }

  try {
    return BigInt(value.trim());
  } catch {
    return undefined;
  }
}

export function tryParseFloat(value: string): number | undefined {
  const n = parseFloat(value.trim());
  return isNaN(n) ? undefined : n;
}

export function urlSafeBase64ToUuid(id: string): string {
  return uuidStringify(
    Uint8Array.from(urlSafeBase64Decode(id), (c) => c.charCodeAt(0)),
  );
}

export async function showSaveFileDialog(opts: {
  contents: string;
  filename: string;
}) {
  // Not supported by firefox: https://developer.mozilla.org/en-US/docs/Web/API/Window/showSaveFilePicker#browser_compatibility
  // possible fallback: https://stackoverflow.com/a/67806663
  if (window.showSaveFilePicker) {
    const handle = await window.showSaveFilePicker({
      suggestedName: opts.filename,
    });
    const writable = await handle.createWritable();
    await writable.write(opts.contents);
    writable.close();
  } else {
    const saveFile = document.createElement("a");
    saveFile.href = URL.createObjectURL(new Blob([opts.contents]));
    saveFile.download = opts.filename;
    saveFile.click();
    setTimeout(() => URL.revokeObjectURL(saveFile.href), 60000);
  }
}
