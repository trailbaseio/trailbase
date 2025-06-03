import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams2 } from "@/lib/list";

import type { ListLogsResponse } from "@bindings/ListLogsResponse";

export async function getLogs(
  pageSize: number,
  filter?: string,
  cursor?: string | null,
): Promise<ListLogsResponse> {
  const params = buildListSearchParams2({
    filter,
    pageSize,
    cursor,
  });

  const response = await adminFetch(`/logs?${params}`);
  return await response.json();
}
