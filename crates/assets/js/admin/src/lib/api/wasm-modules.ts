import { adminFetch } from "@/lib/fetch";

import type { ListWasmModulesResponse } from "@bindings/ListWasmModulesResponse";

export async function fetchWasmModules(): Promise<ListWasmModulesResponse> {
  const response = await adminFetch("/wasm-modules");
  return await response.json();
}
