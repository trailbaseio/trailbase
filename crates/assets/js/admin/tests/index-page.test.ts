import { describe, expect, it } from "vitest";
import { formatBytes } from "@/components/IndexPage";

describe("formatBytes", () => {
  it("formats zero and non-positive values safely", () => {
    expect(formatBytes(0)).toBe("0 Bytes");
    expect(formatBytes(-1)).toBe("0 Bytes");
  });
});
