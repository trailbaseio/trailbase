import { adminFetch } from "@/lib/fetch";
import { findPrimaryKeyColumnIndex } from "@/lib/schema";
import { copyRow, type FormRow } from "@/lib/convert";

import type { Table } from "@bindings/Table";
import type { InsertRowRequest } from "@bindings/InsertRowRequest";
import type { UpdateRowRequest } from "@bindings/UpdateRowRequest";
import type { DeleteRowsRequest } from "@bindings/DeleteRowsRequest";
import type { ListRowsResponse } from "@bindings/ListRowsResponse";

export async function insertRow(table: Table, row: FormRow) {
  const request: InsertRowRequest = {
    row: copyRow(row),
  };

  const response = await adminFetch(`/table/${table.name}`, {
    method: "POST",
    body: JSON.stringify(request),
  });

  return await response.text();
}

export async function updateRow(table: Table, row: FormRow) {
  const tableName = table.name;
  const primaryKeyColumIndex = findPrimaryKeyColumnIndex(table.columns);
  if (primaryKeyColumIndex < 0) {
    throw Error("No primary key column found.");
  }
  const pkColName = table.columns[primaryKeyColumIndex].name;

  const pkValue = row[pkColName];
  if (pkValue === undefined) {
    throw Error("Row is missing primary key.");
  }

  // Update cannot change the PK value.
  const copy = {
    ...row,
  };
  delete copy[pkColName];

  const request: UpdateRowRequest = {
    primary_key_column: pkColName,
    // eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
    primary_key_value: pkValue as Object,
    row: copyRow(copy),
  };

  const response = await adminFetch(`/table/${tableName}`, {
    method: "PATCH",
    body: JSON.stringify(request),
  });

  return await response.text();
}

export async function deleteRows(
  tableName: string,
  request: DeleteRowsRequest,
) {
  const response = await adminFetch(`/table/${tableName}/rows`, {
    method: "DELETE",
    body: JSON.stringify(request),
  });
  return await response.text();
}

export type FetchArgs = {
  tableName: string;
  filter: string | null;
  pageSize: number;
  pageIndex: number;
  cursors: string[];
};

export async function fetchRows(
  source: FetchArgs,
  { value }: { value: ListRowsResponse | undefined },
): Promise<ListRowsResponse> {
  const pageIndex = source.pageIndex;
  const limit = source.pageSize;
  const cursors = source.cursors;

  const filter = source.filter ?? "";
  const filterQuery = filter
    .split("AND")
    .map((frag) => frag.trim().replaceAll(" ", ""))
    .join("&");

  const params = new URLSearchParams(filterQuery);
  params.set("limit", limit.toString());

  // Build the next UUIDv7 "cursor" from previous response and update local
  // cursor stack. If we're paging forward we add new cursors, otherwise we're
  // re-using previously seen cursors for consistency. We reset if we go back
  // to the start.
  if (pageIndex === 0) {
    cursors.length = 0;
  } else {
    const index = pageIndex - 1;
    if (index < cursors.length) {
      // Already known page
      params.set("cursor", cursors[index]);
    } else {
      // New page case: use cursor from previous response or fall back to more
      // expensive and inconsistent offset-based pagination.
      const cursor = value?.cursor;
      if (cursor) {
        cursors.push(cursor);
        params.set("cursor", cursor);
      } else {
        params.set("offset", `${pageIndex * source.pageSize}`);
      }
    }
  }

  try {
    const response = await adminFetch(
      `/table/${source.tableName}/rows?${params}`,
    );
    return (await response.json()) as ListRowsResponse;
  } catch (err) {
    if (value) {
      return value;
    }
    throw err;
  }
}
