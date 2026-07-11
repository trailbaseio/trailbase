import { adminFetch } from "@/lib/fetch";

import type { ListBackupsResponse } from "@bindings/ListBackupsResponse";
import type { DeleteBackupsRequest } from "@bindings/DeleteBackupsRequest";
import type { RestoreBackupRequest } from "@bindings/RestoreBackupRequest";

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
    } as DeleteBackupsRequest),
  });
}

export async function triggerBackup(): Promise<void> {
  await adminFetch("/backups/trigger", {
    method: "GET",
  });
}

export async function restoreBackup(timestamp: bigint): Promise<void> {
  await adminFetch("/backups/restore", {
    method: "PATCH",
    body: JSON.stringify({
      timestamp,
    } as RestoreBackupRequest),
  });
}
