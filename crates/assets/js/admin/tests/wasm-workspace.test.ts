import { describe, expect, it } from "vitest";

import type { WasmComponent } from "@bindings/WasmComponent";
import {
  sortWasmComponents,
  wasmComponentSource,
  wasmComponentStatus,
} from "@/components/wasm/WasmComponentsList";

function component(
  overrides: Partial<WasmComponent> = {},
): WasmComponent {
  return {
    name: "component",
    path: "wasm/component.wasm",
    loaded: false,
    installed: false,
    ...overrides,
  };
}

describe("WASM component state model", () => {
  it("maps every loaded and installed combination", () => {
    expect(wasmComponentStatus(component({ loaded: true, installed: true }))).toEqual({
      key: "running",
      label: "Running",
      priority: 1,
      variant: "success",
    });
    expect(wasmComponentStatus(component({ loaded: false, installed: false }))).toEqual({
      key: "available",
      label: "Available",
      priority: 2,
      variant: "secondary",
    });
    expect(wasmComponentStatus(component({ loaded: false, installed: true }))).toEqual({
      key: "install-pending",
      label: "Install pending restart",
      priority: 0,
      variant: "warning",
    });
    expect(wasmComponentStatus(component({ loaded: true, installed: false }))).toEqual({
      key: "removal-pending",
      label: "Removal pending restart",
      priority: 0,
      variant: "warning",
    });
  });

  it("sorts pending, running, and available components by display name", () => {
    const components = [
      component({ name: "z-running", loaded: true, installed: true }),
      component({ name: "a-available" }),
      component({
        name: "z-install",
        display_name: "a install",
        installed: true,
      }),
      component({
        name: "a-removal",
        display_name: "z removal",
        loaded: true,
      }),
      component({ name: "a-running", loaded: true, installed: true }),
    ];

    expect(sortWasmComponents(components).map((c) => c.name)).toEqual([
      "z-install",
      "a-removal",
      "a-running",
      "z-running",
      "a-available",
    ]);
  });

  it("uses the repository ID or local path as the source", () => {
    expect(wasmComponentSource(component({ repo_id: "trailbase/auth_ui" }))).toBe(
      "trailbase/auth_ui",
    );
    expect(wasmComponentSource(component())).toBe("wasm/component.wasm");
  });
});
