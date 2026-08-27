import { adminFetch } from "@/lib/fetch";

import type { MintRequest } from "@bindings/MintRequest";
import type { LoginResponse } from "@bindings/LoginResponse";

export async function mintTokens(request: MintRequest): Promise<LoginResponse> {
  return (
    await adminFetch("/mint", {
      method: "POST",
      body: JSON.stringify(request),
    })
  ).json();
}
