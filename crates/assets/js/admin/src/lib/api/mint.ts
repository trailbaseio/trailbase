import { adminFetch } from "@/lib/fetch";

import type { MintRequest, LoginResponse } from "trailbase-bindings";

export async function mintTokens(request: MintRequest): Promise<LoginResponse> {
  return (
    await adminFetch("/mint", {
      method: "POST",
      body: JSON.stringify(request),
    })
  ).json();
}
