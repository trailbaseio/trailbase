import type {
  UpdateUserRequest,
  CreateUserRequest,
  ListUsersResponse,
} from "@/lib/bindings";
import { adminFetch } from "@/lib/fetch";

export async function createUser(request: CreateUserRequest) {
  await adminFetch("/user", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function deleteUser(id: string): Promise<void> {
  // TODO: We should probably have a dedicated delete/disable user endpoint?
  await adminFetch("/table/_user", {
    method: "DELETE",
    body: JSON.stringify({
      id: id,
    }),
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
  const pageIndex = source.pageIndex;
  const limit = source.pageSize;
  const cursors = source.cursors;

  const filter = source.filter ?? "";
  const filterQuery = filter
    .split("AND")
    .map((frag) => frag.trim().replaceAll(" ", ""))
    .join("&");

  const params = new URLSearchParams(filterQuery);
  params.set("limit", limit.toString());

  // Build the next UUIDv7 "cursor" from previous response and update local
  // cursor stack. If we're paging forward we add new cursors, otherwise we're
  // re-using previously seen cursors for consistency. We reset if we go back
  // to the start.
  if (pageIndex === 0) {
    cursors.length = 0;
  } else {
    const index = pageIndex - 1;
    if (index < cursors.length) {
      // Already known page
      params.set("cursor", cursors[index]);
    } else {
      // New page case: use cursor from previous response or fall back to more
      // expensive and inconsistent offset-based pagination.
      const cursor = value?.cursor;
      if (cursor) {
        cursors.push(cursor);
        params.set("cursor", cursor);
      } else {
        params.set("offset", `${pageIndex * source.pageSize}`);
      }
    }
  }

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
