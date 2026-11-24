import type {
  ListTenantsResponse,
  CreateTenantRequest,
  DeleteTenantRequest,
} from "trailbase-bindings";

import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";

export async function listTenants(
  filter: string | undefined,
  pageSize: number,
  pageIndex: number,
  order?: string,
): Promise<ListTenantsResponse> {
  const params = buildListSearchParams({
    filter,
    pageSize,
    pageIndex,
    // Users use UUIDv4 and cannot be cursored on `id`.
    cursor: undefined,
    order,
  });

  const response = await adminFetch(`/tenant?${params}`);
  return await response.json();
}

export async function createTenant(request: CreateTenantRequest) {
  await adminFetch("/tenant", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function deleteTenant(request: DeleteTenantRequest) {
  await adminFetch("/tenant", {
    method: "DELETE",
    body: JSON.stringify(request),
  });
}
