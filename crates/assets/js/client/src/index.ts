import { isDev } from "./constants";

export * from "./client";
export * from "./record_api";

/// Decode a base64 string to bytes.
function base64Decode(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

/// Decode a "url-safe" base64 string to bytes.
export function urlSafeBase64Decode(base64: string): Uint8Array {
  return base64Decode(base64.replace(/_/g, "/").replace(/-/g, "+"));
}

/// Encode an arbitrary string input as base64 string.
function base64Encode(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes));
}

/// Encode an arbitrary string input as a "url-safe" base64 string.
export function urlSafeBase64Encode(bytes: Uint8Array): string {
  return base64Encode(bytes).replace(/\//g, "_").replace(/\+/g, "-");
}

export const exportedForTesting = isDev
  ? {
      base64Decode,
      base64Encode,
    }
  : undefined;
