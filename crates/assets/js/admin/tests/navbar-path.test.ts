import { describe, expect, it } from "vitest";
import { isPathActive } from "@/components/Navbar";

describe("isPathActive", () => {
  it("matches routes with or without trailing slashes", () => {
    expect(isPathActive("/table/", "/table")).toBe(true);
    expect(isPathActive("/wasm", "/wasm/")).toBe(true);
    expect(isPathActive("/settings/", "/settings/")).toBe(true);
  });

  it("does not let root match every route", () => {
    expect(isPathActive("/table", "/")).toBe(false);
    expect(isPathActive("/", "/")).toBe(true);
  });
});
