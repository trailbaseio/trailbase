/// Decode a base64 string to bytes.
export function base64Decode(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

/// Decode a "url-safe" base64 string to bytes.
export function urlSafeBase64Decode(base64: string): Uint8Array {
  return base64Decode(base64.replace(/_/g, "/").replace(/-/g, "+"));
}

/// Encode an arbitrary string input as base64 string.
export function base64Encode(b: Uint8Array): string {
  return btoa(String.fromCharCode(...b));
}

/// Encode an arbitrary string input as a "url-safe" base64 string.
export function urlSafeBase64Encode(b: Uint8Array): string {
  return base64Encode(b).replace(/\//g, "_").replace(/\+/g, "-");
}
