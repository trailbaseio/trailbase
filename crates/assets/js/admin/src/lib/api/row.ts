import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";
import { findPrimaryKeyColumnIndex, escapeQualifiedName } from "@/lib/schema";
import type { Record } from "@/lib/record";

import type {
  Table,
  Column,
  InsertRowRequest,
  UpdateRowRequest,
  DeleteRowsRequest,
  ListRowsResponse,
  QualifiedName,
  SqlValue,
  Tenant,
} from "trailbase-bindings";

function removeUndefined(row: Record): { [key: string]: SqlValue } {
  return Object.fromEntries(
    Object.entries(row).filter(([_, value]) => value !== undefined),
  ) as { [key: string]: SqlValue };
}

export async function insertRow(
  table: Table,
  tenant: Tenant | null,
  row: Record,
) {
  const request: InsertRowRequest = {
    tenant,
    row: removeUndefined(row),
  };

  const tableName: string = escapeQualifiedName(table.name);
  const response = await adminFetch(`/table/${tableName}`, {
    method: "POST",
    body: JSON.stringify(request),
  });

  return await response.text();
}

export async function updateRow(
  table: Table,
  tenant: Tenant | null,
  row: Record,
) {
  return await updateRowInternal(table.name, tenant, table.columns, row);
}

export async function updateRowInternal(
  qualifiedTableName: QualifiedName,
  tenant: Tenant | null,
  columns: Column[],
  row: Record,
) {
  const tableName: string = escapeQualifiedName(qualifiedTableName);
  const primaryKeyColumIndex = findPrimaryKeyColumnIndex(columns);
  if (primaryKeyColumIndex === undefined) {
    throw Error("No primary key column found.");
  }
  const pkColName = columns[primaryKeyColumIndex].name;

  const pkValue = row[pkColName];
  if (pkValue === undefined) {
    throw Error("Row is missing primary key.");
  }

  const filteredRow = removeUndefined(row);
  // Update cannot change the PK value.
  delete filteredRow[pkColName];

  const request: UpdateRowRequest = {
    tenant,
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
  tableName: QualifiedName,
  request: DeleteRowsRequest,
) {
  const response = await adminFetch(
    `/table/${escapeQualifiedName(tableName)}`,
    {
      method: "DELETE",
      body: JSON.stringify(request),
    },
  );
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

// TODO: Move more options into opts.
export async function listRows(
  tableName: QualifiedName,
  pageSize: number,
  pageIndex: number,
  opts?: {
    filter?: string | null;
    cursor: string | null;
    order?: string;
    tenant?: Tenant | null;
  },
): Promise<ListRowsResponse> {
  const params = buildListSearchParams({
    pageSize,
    pageIndex,
    ...opts,
  });

  const response = await adminFetch(
    `/table/${escapeQualifiedName(tableName)}/rows?${params}`,
  );
  // IMPORTANT: Use JSON parser that handles i64 correctly.
  return parseJSON(await response.text()) as ListRowsResponse;
}
