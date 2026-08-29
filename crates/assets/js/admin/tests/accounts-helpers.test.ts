import { describe, expect, test } from "vitest";

import { formatAccountTime } from "@/components/accounts/AccountsPage";

describe("formatAccountTime", () => {
  const now = 1_000_000;

  test("truncates sub-minute values without rounding to 0 or 60", () => {
    expect(formatAccountTime(1000n, now - 59_600)).toContain("59 seconds");
    expect(formatAccountTime(1000n, now + 59_600)).toContain("59 seconds");
    expect(formatAccountTime(1000n, now - 400)).toContain("1 second");
  });
});
