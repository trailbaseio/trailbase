import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";

import type { ListLogsResponse } from "@bindings/ListLogsResponse";

export type GetLogsProps = {
  // Filter where clause to pass to the fetch.
  filter?: string;
  pageSize: number;
  pageIndex: number;
  // Keep track of the timestamp cursor to have consistency for forwards and backwards pagination.
  cursors: string[];
};

export async function getLogs(
  source: GetLogsProps,
  cursor: string | null,
): Promise<ListLogsResponse> {
  const params = buildListSearchParams({
    filter: source.filter,
    pageSize: source.pageSize,
    pageIndex: source.pageIndex,
    cursor,
    prevCursors: source.cursors,
  });

  const response = await adminFetch(`/logs?${params}`);
  return await response.json();
}
