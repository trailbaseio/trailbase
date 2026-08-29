import { describe, expect, test } from "vitest";

import { formatAccountTime } from "@/components/accounts/AccountsPage";
import { appendAccountSearchParams } from "@/lib/api/user";

describe("appendAccountSearchParams", () => {
  test("trims and appends email and username like filters", () => {
    const params = new URLSearchParams();
    appendAccountSearchParams(params, "  ada  ");
    expect([...params]).toEqual([
      ["filter[$or][0][email][$like]", "%ada%"],
      ["filter[$or][1][username][$like]", "%ada%"],
    ]);
  });

  test("uses an exact id filter for canonical UUIDs", () => {
    const params = new URLSearchParams();
    appendAccountSearchParams(params, "550e8400-e29b-41d4-a716-446655440000");
    expect([...params]).toEqual([
      ["filter[id][$eq]", "550e8400-e29b-41d4-a716-446655440000"],
    ]);
  });

  test("does nothing for whitespace", () => {
    const params = new URLSearchParams("existing=value");
    appendAccountSearchParams(params, "  ");
    expect(params.toString()).toBe("existing=value");
  });
});

describe("formatAccountTime", () => {
  const now = 1_000_000;

  test("truncates sub-minute values without rounding to 0 or 60", () => {
    expect(formatAccountTime(1000n, now - 59_600)).toContain("59 seconds");
    expect(formatAccountTime(1000n, now + 59_600)).toContain("59 seconds");
    expect(formatAccountTime(1000n, now - 400)).toContain("1 second");
  });
});
