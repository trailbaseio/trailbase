import { adminFetch } from "@/lib/fetch";
import { buildListSearchParams } from "@/lib/list";

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

export type FetchUsersArgs = {
  filter: string | undefined;
  pageSize: number;
  pageIndex: number;
  cursors: string[];
};

export async function fetchUsers(
  source: FetchUsersArgs,
  { value }: { value: ListUsersResponse | undefined },
): Promise<ListUsersResponse> {
  const params = buildListSearchParams({
    filter: source.filter,
    pageSize: source.pageSize,
    pageIndex: source.pageIndex,
    cursor: value?.cursor,
    prevCursors: source.cursors,
  });

  try {
    const response = await adminFetch(`/user?${params}`);
    return await response.json();
  } catch (err) {
    if (value) {
      return value;
    }
    throw err;
  }
}
