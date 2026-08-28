import { beforeEach, describe, expect, it } from "vitest";
import {
  $themePreference,
  applyResolvedTheme,
  initializeTheme,
} from "@/lib/theme";

describe("theme", () => {
  beforeEach(() => {
    document.documentElement.className = "";
    document.documentElement.removeAttribute("data-kb-theme");
    $themePreference.set(undefined);
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: () => ({ matches: true }),
    });
  });

  it("applies the resolved theme to the root", () => {
    applyResolvedTheme("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.dataset.kbTheme).toBe("dark");
  });

  it("uses the saved preference before the system preference", () => {
    $themePreference.set("light");
    initializeTheme();
    expect(document.documentElement.dataset.kbTheme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });
});
