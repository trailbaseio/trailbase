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

const MAX_SAFE_BIGINT = BigInt(Number.MAX_SAFE_INTEGER);
type DeleteBackupsJson = Omit<DeleteBackupsRequest, "timestamps"> & {
  timestamps: number[];
};
type RestoreBackupJson = Omit<RestoreBackupRequest, "timestamp"> & {
  timestamp: number;
};

function safeJsonNumber(value: bigint): number {
  if (value > MAX_SAFE_BIGINT || value < -MAX_SAFE_BIGINT) {
    throw new TypeError("Backup timestamp is outside the safe integer range");
  }
  return Number(value);
}

export async function deleteBackups(timestamps: bigint[]): Promise<void> {
  const payload: DeleteBackupsJson = {
    timestamps: timestamps.map(safeJsonNumber),
  };
  await adminFetch("/backups/delete", {
    method: "DELETE",
    body: JSON.stringify(payload),
  });
}

export async function triggerBackup(): Promise<void> {
  await adminFetch("/backups/trigger", {
    method: "POST",
  });
}

export async function restoreBackup(timestamp: bigint): Promise<void> {
  const payload: RestoreBackupJson = {
    timestamp: safeJsonNumber(timestamp),
  };
  await adminFetch("/backups/restore", {
    method: "PATCH",
    body: JSON.stringify(payload),
  });
}
