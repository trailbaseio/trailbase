import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";

import type { ListLogsResponse } from "@bindings/ListLogsResponse";

export async function getLogs(
  pageSize: number,
  filter?: string,
  cursor?: string | null,
): Promise<ListLogsResponse> {
  const params = buildListSearchParams({
    filter,
    pageSize,
    cursor,
  });

  const response = await adminFetch(`/logs?${params}`);
  return await response.json();
}
