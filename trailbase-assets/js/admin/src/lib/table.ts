import { adminFetch } from "@/lib/fetch";
import { useQuery } from "@tanstack/solid-query";

import type { AlterIndexRequest } from "@bindings/AlterIndexRequest";
import type { AlterTableRequest } from "@bindings/AlterTableRequest";
import type { CreateIndexRequest } from "@bindings/CreateIndexRequest";
import type { CreateIndexResponse } from "@bindings/CreateIndexResponse";
import type { CreateTableRequest } from "@bindings/CreateTableRequest";
import type { CreateTableResponse } from "@bindings/CreateTableResponse";
import type { DropIndexRequest } from "@bindings/DropIndexRequest";
import type { DropTableRequest } from "@bindings/DropTableRequest";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";

export function createTableSchemaQuery() {
  async function getAllTableSchemas(): Promise<ListSchemasResponse> {
    const response = await adminFetch("/tables");
    return (await response.json()) as ListSchemasResponse;
  }

  return useQuery(() => ({
    queryKey: ["table_schema"],
    queryFn: getAllTableSchemas,
    // refetchInterval: 120 * 1000,
    refetchOnMount: true,
  }));
}

export async function createIndex(
  request: CreateIndexRequest,
): Promise<CreateIndexResponse> {
  const response = await adminFetch("/index", {
    method: "POST",
    body: JSON.stringify(request),
  });
  return await response.json();
}

export async function createTable(
  request: CreateTableRequest,
): Promise<CreateTableResponse> {
  const response = await adminFetch("/table", {
    method: "POST",
    body: JSON.stringify(request),
  });
  return await response.json();
}

export async function alterIndex(request: AlterIndexRequest) {
  const response = await adminFetch("/index", {
    method: "PATCH",
    body: JSON.stringify(request),
  });
  return await response.text();
}

export async function alterTable(request: AlterTableRequest) {
  const response = await adminFetch("/table", {
    method: "PATCH",
    body: JSON.stringify(request),
  });
  return await response.text();
}

export async function dropIndex(request: DropIndexRequest) {
  const response = await adminFetch("/index", {
    method: "DELETE",
    body: JSON.stringify(request),
  });
  return await response.text();
}

export async function dropTable(request: DropTableRequest) {
  const response = await adminFetch("/table", {
    method: "DELETE",
    body: JSON.stringify(request),
  });
  return await response.text();
}
