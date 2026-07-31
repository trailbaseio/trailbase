import { adminFetch } from "@/lib/fetch";

import type { ListWasmComponentsResponse } from "@bindings/ListWasmComponentsResponse";
import type { WasmComponentRequest } from "@bindings/WasmComponentRequest";

export async function listWasmComponents(): Promise<ListWasmComponentsResponse> {
  const response = await adminFetch("/wasm");
  return await response.json();
}

export async function installWasmComponent(
  r: WasmComponentRequest,
): Promise<void> {
  await adminFetch("/wasm/install", {
    method: "POST",
    body: JSON.stringify(r),
  });
}

export async function uninstallWasmComponent(
  r: WasmComponentRequest,
): Promise<void> {
  await adminFetch("/wasm/uninstall", {
    method: "POST",
    body: JSON.stringify(r),
  });
}
