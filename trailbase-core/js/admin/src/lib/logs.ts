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
  { value }: { value: ListLogsResponse | undefined },
): Promise<ListLogsResponse> {
  const params = buildListSearchParams({
    filter: source.filter,
    pageSize: source.pageSize,
    pageIndex: source.pageIndex,
    cursor: value?.cursor,
    prevCursors: source.cursors,
  });

  try {
    const response = await adminFetch(`/logs?${params}`);
    return await response.json();
  } catch (err) {
    if (value) {
      return value;
    }
    throw err;
  }
}
