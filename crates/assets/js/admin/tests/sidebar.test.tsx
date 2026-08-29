import { describe, expect, it, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import {
  SIDEBAR_WIDTH,
  SIDEBAR_WIDTH_ICON,
  readSidebarCookie,
} from "@/components/ui/sidebar";

describe("sidebar shell", () => {
  beforeEach(() => {
    document.cookie = "sidebar:state=; Max-Age=0";
    document.cookie = "admin-sidebar:state=; Max-Age=0";
    if (!window.matchMedia) {
      window.matchMedia = () =>
        ({
          matches: false,
          addEventListener: () => undefined,
          removeEventListener: () => undefined,
        }) as unknown as MediaQueryList;
    }
  });

  it("persists the state when the trigger is clicked", async () => {
    const result = render(() => (
      <SidebarProvider defaultOpen cookieName="admin-sidebar:state">
        <SidebarTrigger aria-label="Toggle sidebar" />
      </SidebarProvider>
    ));

    await fireEvent.click(
      result.getByRole("button", { name: "Toggle sidebar" }),
    );
    expect(document.cookie).toContain("admin-sidebar:state=false");
    result.unmount();
  });

  it("uses the approved expanded and collapsed widths", () => {
    expect(SIDEBAR_WIDTH).toBe("15rem");
    expect(SIDEBAR_WIDTH_ICON).toBe("4rem");
  });

  it("reads only valid persisted cookie state", () => {
    expect(readSidebarCookie("sidebar:state=false")).toBe(false);
    expect(readSidebarCookie("sidebar:state=true; other=value")).toBe(true);
    expect(readSidebarCookie("not-sidebar:state=true")).toBeUndefined();
    expect(readSidebarCookie("other=value")).toBeUndefined();
    expect(
      readSidebarCookie(
        "sidebar:state=false; admin-sidebar:state=true",
        "admin-sidebar:state",
      ),
    ).toBe(true);
  });

  it("ignores malformed persisted values", () => {
    expect(() => readSidebarCookie("sidebar:state=%")).not.toThrow();
    expect(readSidebarCookie("sidebar:state=%")).toBeUndefined();
  });
});
