import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams2 } from "@/lib/list";
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

  const tableName: string = prettyFormatQualifiedName(table.name);
  const response = await adminFetch(`/table/${tableName}`, {
    method: "POST",
    body: JSON.stringify(request),
  });

  return await response.text();
}

export async function updateRow(table: Table, row: FormRow) {
  const tableName: string = prettyFormatQualifiedName(table.name);
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

export async function fetchRows(
  tableName: QualifiedName,
  filter: string | null,
  pageSize: number,
  pageIndex: number,
  cursor: string | null,
): Promise<ListRowsResponse> {
  const params = buildListSearchParams2({
    filter,
    pageSize,
    pageIndex,
    cursor,
  });

  const path = `/table/${prettyFormatQualifiedName(tableName)}/rows?${params}`;

  const response = await adminFetch(path);
  return (await response.json()) as ListRowsResponse;
}
