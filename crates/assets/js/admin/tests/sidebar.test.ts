import { describe, expect, it } from "vitest";
import {
  SIDEBAR_WIDTH,
  SIDEBAR_WIDTH_ICON,
  readSidebarCookie,
} from "@/components/ui/sidebar";

describe("sidebar shell", () => {
  it("uses the approved expanded and collapsed widths", () => {
    expect(SIDEBAR_WIDTH).toBe("15rem");
    expect(SIDEBAR_WIDTH_ICON).toBe("4rem");
  });

  it("reads the persisted cookie state", () => {
    expect(readSidebarCookie("sidebar:state=false")).toBe(false);
    expect(readSidebarCookie("sidebar:state=true; other=value")).toBe(true);
    expect(readSidebarCookie("other=value")).toBeUndefined();
  });
});
