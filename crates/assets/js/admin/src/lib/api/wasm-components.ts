import { adminFetch } from "@/lib/fetch";

import type { ListWasmComponentsResponse } from "@bindings/ListWasmComponentsResponse";

export async function listWasmComponents(): Promise<ListWasmComponentsResponse> {
  const response = await adminFetch("/wasm");
  return await response.json();
}
