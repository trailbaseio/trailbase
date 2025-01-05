import type { UpdateUserRequest, CreateUserRequest } from "@/lib/bindings";
import { adminFetch } from "@/lib/fetch";

export async function createUser(request: CreateUserRequest) {
  await adminFetch("/user", {
    method: "Post",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(request),
  });
}

export async function deleteUser(id: string): Promise<void> {
  // TODO: We should probably have a dedicated delete/disable user endpoint?
  await adminFetch("/table/_user", {
    method: "DELETE",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      id: id,
    }),
  });
}

export async function updateUser(request: UpdateUserRequest) {
  await adminFetch("/user", {
    method: "PATCH",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(request),
  });
}
