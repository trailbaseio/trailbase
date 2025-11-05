import { useQuery } from "@tanstack/solid-query";

import { adminFetch } from "@/lib/fetch";
import type { InfoResponse } from "@bindings/InfoResponse";

export function createVersionInfoQuery() {
  async function getVersionInfo(): Promise<InfoResponse> {
    const response = await adminFetch("/info");
    return (await response.json()) as InfoResponse;
  }

  return useQuery(() => ({
    queryKey: ["version_info"],
    queryFn: getVersionInfo,
  }));
}
