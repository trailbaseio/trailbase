import { describe, expect, it, vi } from "vitest";

const adminFetch = vi.fn().mockResolvedValue({});
vi.mock("@/lib/fetch", () => ({ adminFetch }));

import { deleteBackups, restoreBackup } from "@/lib/api/backups";

describe("backup API payloads", () => {
  it("serializes delete timestamps as seconds", async () => {
    await deleteBackups([1_700_000_000n, 1_700_000_001n]);
    expect(adminFetch).toHaveBeenCalledWith("/backups/delete", {
      method: "DELETE",
      body: JSON.stringify({ timestamps: [1700000000, 1700000001] }),
    });
  });

  it("serializes restore timestamps as seconds", async () => {
    await restoreBackup(1_700_000_000n);
    expect(adminFetch).toHaveBeenCalledWith("/backups/restore", {
      method: "PATCH",
      body: JSON.stringify({ timestamp: 1700000000 }),
    });
  });

  it("rejects unsafe timestamps before making a request", async () => {
    adminFetch.mockClear();
    await expect(
      restoreBackup(BigInt(Number.MAX_SAFE_INTEGER) + 1n),
    ).rejects.toThrow(/safe integer/i);
    expect(adminFetch).not.toHaveBeenCalled();
  });

  it("rejects unsafe timestamps in delete arrays", async () => {
    adminFetch.mockClear();
    await expect(
      deleteBackups([1_700_000_000n, 9_007_199_254_740_992n]),
    ).rejects.toThrow();
    expect(adminFetch).not.toHaveBeenCalled();
  });
});
