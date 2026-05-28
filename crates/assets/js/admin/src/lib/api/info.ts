import { useQuery } from "@tanstack/solid-query";

import { adminFetch } from "@/lib/fetch";
import type { InfoResponse } from "@bindings/InfoResponse";

export function createSystemInfoQuery() {
  return useQuery(() => ({
    queryKey: ["version_info"],
    staleTime: 60 * 1000, // 1min
    queryFn: async ({ queryKey: _ }) => {
      const response = await adminFetch("/info");
      return (await response.json()) as InfoResponse;
    },
  }));
}
