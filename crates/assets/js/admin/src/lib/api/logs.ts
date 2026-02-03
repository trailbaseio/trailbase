import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";

import type { ListLogsResponse } from "@bindings/ListLogsResponse";

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

  const response = await adminFetch(`/logs?${params}`);
  return await response.json();
}
