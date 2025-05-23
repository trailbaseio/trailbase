import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";
import {
  findPrimaryKeyColumnIndex,
  prettyFormatQualifiedName,
} from "@/lib/schema";
import { preProcessRow, type FormRow } from "@/lib/convert";

import type { Table } from "@bindings/Table";
import type { InsertRowRequest } from "@bindings/InsertRowRequest";
import type { UpdateRowRequest } from "@bindings/UpdateRowRequest";
import type { DeleteRowsRequest } from "@bindings/DeleteRowsRequest";
import type { ListRowsResponse } from "@bindings/ListRowsResponse";
import type { QualifiedName } from "@bindings/QualifiedName";

export async function insertRow(table: Table, row: FormRow) {
  const processedRow = preProcessRow(table, row, false);

  const request: InsertRowRequest = {
    row: processedRow,
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
  const processedRow = preProcessRow(table, row, true);
  delete processedRow[pkColName];

  const request: UpdateRowRequest = {
    primary_key_column: pkColName,
    // eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
    primary_key_value: pkValue as Object,
    row: processedRow,
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
  tableName: QualifiedName;
  filter: string | null;
  pageSize: number;
  pageIndex: number;
  cursors: string[];
};

export async function fetchRows(
  source: FetchArgs,
  { value }: { value: ListRowsResponse | undefined },
): Promise<ListRowsResponse> {
  const params = buildListSearchParams({
    filter: source.filter,
    pageSize: source.pageSize,
    pageIndex: source.pageIndex,
    cursor: value?.cursor,
    prevCursors: source.cursors,
  });

  try {
    const path = `/table/${prettyFormatQualifiedName(source.tableName)}/rows?${params}`;

    const response = await adminFetch(path);
    return (await response.json()) as ListRowsResponse;
  } catch (err) {
    if (value) {
      return value;
    }
    throw err;
  }
}
