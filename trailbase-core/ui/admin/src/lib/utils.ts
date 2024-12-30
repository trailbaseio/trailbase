import type { ClassValue } from "clsx";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import { stringify as uuidStringify } from "uuid";
import { urlSafeBase64Decode } from "trailbase";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
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
