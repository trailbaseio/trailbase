import { adminFetch } from "@/lib/fetch";

import type { ListBackupsResponse } from "@bindings/ListBackupsResponse";

export async function listBackups(): Promise<ListBackupsResponse> {
  const response = await adminFetch("/backups", {
    method: "GET",
  });
  return await response.json();
}
