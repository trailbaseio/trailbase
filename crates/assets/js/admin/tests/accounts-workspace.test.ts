import { describe, expect, it } from "vitest";

import {
  accountIdentity,
  accountProviderLabel,
  accountStatuses,
  buildColumns,
  formatAccountTime,
  shortAccountId,
} from "@/components/accounts/AccountsPage";

import type { UserJson } from "@bindings/UserJson";

const user: UserJson = {
  id: "8c29281a-be84-44d7-9bb6-00fe8032dfa1",
  email: "ada@example.com",
  unverified_email: null,
  username: "ada",
  admin: false,
  provider_id: 0n,
  provider_user_id: null,
  created: 1_700_000_000n,
  updated: 1_700_000_000n,
};

describe("Accounts workspace presentation", () => {
  it("selects a primary and distinct secondary identity", () => {
    expect(accountIdentity(user)).toEqual({
      primary: "ada@example.com",
      secondary: "ada",
    });
    expect(accountIdentity({ ...user, email: null, username: null })).toEqual({
      primary: "Unnamed account",
      secondary: undefined,
    });
  });

  it("shortens account IDs without losing their ends", () => {
    expect(shortAccountId("8c29281a-be84-44d7-9bb6-00fe8032dfa1")).toBe(
      "8c29281a…dfa1",
    );
  });

  it("describes account status with text-bearing badge variants", () => {
    expect(accountStatuses({ ...user, admin: true })).toEqual([
      { label: "Admin", variant: "default" },
      { label: "Verified", variant: "success" },
    ]);
    expect(
      accountStatuses({ ...user, unverified_email: "next@example.com" }),
    ).toContainEqual({ label: "Pending verification", variant: "warning" });
  });

  it("labels password and OAuth providers", () => {
    expect(accountProviderLabel(0n)).toBe("Password");
    expect(accountProviderLabel(0)).toBe("Password");
    expect(accountProviderLabel(12n)).toBe("Google");
    expect(accountProviderLabel(99n)).toBe("OAuth 99");
  });

  it("formats relative account times from a deterministic clock", () => {
    expect(formatAccountTime(1_700_000_000n, 1_700_003_600_000, "en")).toBe(
      "1 hour ago",
    );
  });

  it("uses a stable label for sub-minute relative times", () => {
    expect(formatAccountTime(1_700_000_030n, 1_700_000_000_000, "fr")).toBe(
      "just now",
    );
  });

  it("sorts the derived account column by the real user ID field", () => {
    expect(buildColumns()[0]).toMatchObject({
      header: "Account",
      accessorKey: "id",
    });
  });
});
