import { adminFetch } from "@/lib/fetch";
import { useQuery } from "@tanstack/solid-query";

import type {
  AlterIndexRequest,
  AlterIndexResponse,
  AlterTableRequest,
  AlterTableResponse,
  CreateIndexRequest,
  CreateIndexResponse,
  CreateTableRequest,
  CreateTableResponse,
  DropIndexRequest,
  DropIndexResponse,
  DropTableRequest,
  DropTableResponse,
  ListSchemasResponse,
} from "trailbase-bindings";

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

export async function alterIndex(
  request: AlterIndexRequest,
): Promise<AlterIndexResponse> {
  const response = await adminFetch("/index", {
    method: "PATCH",
    body: JSON.stringify(request),
  });
  return await response.json();
}

export async function alterTable(
  request: AlterTableRequest,
): Promise<AlterTableResponse> {
  const response = await adminFetch("/table", {
    method: "PATCH",
    body: JSON.stringify(request),
  });
  return await response.json();
}

export async function dropIndex(
  request: DropIndexRequest,
): Promise<DropIndexResponse> {
  const response = await adminFetch("/index", {
    method: "DELETE",
    body: JSON.stringify(request),
  });
  return await response.json();
}

export async function dropTable(
  request: DropTableRequest,
): Promise<DropTableResponse> {
  const response = await adminFetch("/table", {
    method: "DELETE",
    body: JSON.stringify(request),
  });
  return await response.json();
}
