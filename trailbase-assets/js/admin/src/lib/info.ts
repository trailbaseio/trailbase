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

export function version(info: InfoResponse | undefined): string {
  // Version tags have the shape <tag>[-<n>-<hash>], where the latter part is
  // missing if it's an exact match. Otherwise, it will contain a reference to
  // the actual commit and how many commits `n` are in between.
  const tag = info?.version_tag;
  if (!tag) {
    return "";
  }

  const fragments = tag.split("-");
  if (fragments.length == 1) {
    return fragments[0];
  }
  return `${fragments[0]} (${fragments[1]})`;
}
