import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";

const UUID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export function appendAccountSearchParams(
  params: URLSearchParams,
  search: string,
) {
  const value = search.trim();
  if (!value) return params;
  params.append("filter[$or][0][email][$like]", `%${value}%`);
  params.append("filter[$or][1][username][$like]", `%${value}%`);
  if (UUID.test(value)) {
    params.append("filter[$or][2][id][$eq]", value);
  }
  return params;
}

import type { UpdateUserRequest } from "@bindings/UpdateUserRequest";
import type { CreateUserRequest } from "@bindings/CreateUserRequest";
import type { ListUsersResponse } from "@bindings/ListUsersResponse";
import type { DeleteUserRequest } from "@bindings/DeleteUserRequest";

export async function createUser(request: CreateUserRequest) {
  await adminFetch("/user", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function deleteUser(request: DeleteUserRequest): Promise<void> {
  await adminFetch("/user", {
    method: "DELETE",
    body: JSON.stringify(request),
  });
}

export async function updateUser(request: UpdateUserRequest) {
  await adminFetch("/user", {
    method: "PATCH",
    body: JSON.stringify(request),
  });
}

export async function fetchUsers(
  filter: string | undefined,
  pageSize: number,
  pageIndex: number,
  order?: string,
  search?: string,
): Promise<ListUsersResponse> {
  const params = buildListSearchParams({
    filter,
    pageSize,
    pageIndex,
    // Users use UUIDv4 and cannot be cursored on `id`.
    cursor: undefined,
    order,
  });
  if (search !== undefined) appendAccountSearchParams(params, search);

  const response = await adminFetch(`/user?${params}`);
  return await response.json();
}
