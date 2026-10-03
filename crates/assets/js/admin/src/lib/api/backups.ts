import { adminFetch } from "@/lib/fetch";

import type {
  ListBackupsResponse,
  DeleteBackupsRequest,
  RestoreBackupRequest,
} from "trailbase-bindings";

export async function listBackups(): Promise<ListBackupsResponse> {
  const response = await adminFetch("/backups", {
    method: "GET",
  });
  return await response.json();
}

export async function deleteBackups(timestamps: bigint[]): Promise<void> {
  await adminFetch("/backups/delete", {
    method: "DELETE",
    body: JSON.stringify({
      timestamps,
    } satisfies DeleteBackupsRequest),
  });
}

export async function triggerBackup(): Promise<void> {
  await adminFetch("/backups/trigger", {
    method: "POST",
  });
}

export async function restoreBackup(timestamp: bigint): Promise<void> {
  await adminFetch("/backups/restore", {
    method: "PATCH",
    body: JSON.stringify({
      timestamp,
    } satisfies RestoreBackupRequest),
  });
}
