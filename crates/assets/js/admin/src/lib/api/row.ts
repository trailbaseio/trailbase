import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";
import {
  findPrimaryKeyColumnIndex,
  prettyFormatQualifiedName,
} from "@/lib/schema";
import type { Record } from "@/lib/record";

import type { Table } from "@bindings/Table";
import type { InsertRowRequest } from "@bindings/InsertRowRequest";
import type { UpdateRowRequest } from "@bindings/UpdateRowRequest";
import type { DeleteRowsRequest } from "@bindings/DeleteRowsRequest";
import type { ListRowsResponse } from "@bindings/ListRowsResponse";
import type { QualifiedName } from "@bindings/QualifiedName";
import type { SqlValue } from "@bindings/SqlValue";

function removeUndefined(row: Record): { [key: string]: SqlValue } {
  return Object.fromEntries(
    Object.entries(row).filter(([_, value]) => value !== undefined),
  ) as { [key: string]: SqlValue };
}

export async function insertRow(table: Table, row: Record) {
  const request: InsertRowRequest = {
    row: removeUndefined(row),
  };

  const tableName: string = prettyFormatQualifiedName(table.name);
  const response = await adminFetch(`/table/${tableName}`, {
    method: "POST",
    body: JSON.stringify(request),
  });

  return await response.text();
}

export async function updateRow(table: Table, row: Record) {
  const tableName: string = prettyFormatQualifiedName(table.name);
  const primaryKeyColumIndex = findPrimaryKeyColumnIndex(table.columns);
  if (primaryKeyColumIndex === undefined) {
    throw Error("No primary key column found.");
  }
  const pkColName = table.columns[primaryKeyColumIndex].name;

  const pkValue = row[pkColName];
  if (pkValue === undefined) {
    throw Error("Row is missing primary key.");
  }

  const filteredRow = removeUndefined(row);
  // Update cannot change the PK value.
  delete filteredRow[pkColName];

  const request: UpdateRowRequest = {
    primary_key_column: pkColName,
    primary_key_value: pkValue,
    row: filteredRow,
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

/// Flavor that parses `i64` correctly.
function parseJSON(text: string) {
  function reviver(_key: string, value: unknown, context: { source: string }) {
    if (
      typeof value === "number" &&
      Number.isInteger(value) &&
      !Number.isSafeInteger(value)
    ) {
      // Ignore the value because it has already lost precision
      return BigInt(context.source);
    }
    return value;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return JSON.parse(text, reviver as any);
}

export async function fetchRows(
  tableName: QualifiedName,
  filter: string | null,
  pageSize: number,
  pageIndex: number,
  cursor: string | null,
  order?: string,
): Promise<ListRowsResponse> {
  const params = buildListSearchParams({
    filter,
    pageSize,
    pageIndex,
    cursor,
    order,
  });

  const response = await adminFetch(
    `/table/${prettyFormatQualifiedName(tableName)}/rows?${params}`,
  );
  // IMPORTANT: Use JSON parser that handles i64 correctly.
  return parseJSON(await response.text()) as ListRowsResponse;
}
