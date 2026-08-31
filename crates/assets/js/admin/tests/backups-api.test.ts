import { beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({ adminFetch: vi.fn() }));
vi.mock("@/lib/fetch", () => ({ adminFetch: state.adminFetch }));

import { deleteBackups, restoreBackup } from "@/lib/api/backups";

describe("backup API payloads", () => {
  beforeEach(() => {
    state.adminFetch.mockReset().mockResolvedValue({});
  });
  it("serializes delete timestamps as seconds", async () => {
    await deleteBackups([1_700_000_000n, 1_700_000_001n]);
    expect(state.adminFetch).toHaveBeenCalledWith("/backups/delete", {
      method: "DELETE",
      body: JSON.stringify({ timestamps: [1700000000, 1700000001] }),
    });
  });

  it("serializes restore timestamps as seconds", async () => {
    await restoreBackup(1_700_000_000n);
    expect(state.adminFetch).toHaveBeenCalledWith("/backups/restore", {
      method: "PATCH",
      body: JSON.stringify({ timestamp: 1700000000 }),
    });
  });

  it("rejects unsafe timestamps before making a request", async () => {
    state.adminFetch.mockClear();
    await expect(
      restoreBackup(BigInt(Number.MAX_SAFE_INTEGER) + 1n),
    ).rejects.toThrow(/safe integer/i);
    expect(state.adminFetch).not.toHaveBeenCalled();
  });

  it("rejects unsafe timestamps in delete arrays", async () => {
    state.adminFetch.mockClear();
    await expect(
      deleteBackups([1_700_000_000n, 9_007_199_254_740_992n]),
    ).rejects.toThrow();
    expect(state.adminFetch).not.toHaveBeenCalled();
  });
});
