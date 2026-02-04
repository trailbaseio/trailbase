import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";

import type { ListLogsResponse } from "@bindings/ListLogsResponse";
import type { StatsResponse } from "@bindings/StatsResponse";

export async function fetchLogs(
  pageSize: number,
  pageIndex: number,
  filter?: string,
  cursor?: string | null,
  order?: string,
): Promise<ListLogsResponse> {
  const params = buildListSearchParams({
    filter,
    pageSize,
    pageIndex,
    cursor,
    order,
  });

  const response = await adminFetch(`/logs/list?${params}`);
  return await response.json();
}

export async function fetchStats(filter?: string): Promise<StatsResponse> {
  const params = buildListSearchParams({
    filter,
  });

  const response = await adminFetch(`/logs/stats?${params}`);
  return await response.json();
}
