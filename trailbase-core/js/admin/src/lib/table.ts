import { adminFetch } from "@/lib/fetch";
import type {
  AlterIndexRequest,
  AlterTableRequest,
  CreateIndexRequest,
  CreateIndexResponse,
  CreateTableRequest,
  CreateTableResponse,
  DropIndexRequest,
  DropTableRequest,
  ListSchemasResponse,
} from "@/lib/bindings";

export async function getAllTableSchemas(): Promise<ListSchemasResponse> {
  const response = await adminFetch("/tables");
  return (await response.json()) as ListSchemasResponse;
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
