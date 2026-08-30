import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WasmComponent } from "@bindings/WasmComponent";

vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; children: unknown }) => (
    <a href={props.href}>{props.children}</a>
  ),
}));

import { WasmComponentsList } from "@/components/wasm/WasmComponentsList";

const component = (overrides: Partial<WasmComponent> = {}): WasmComponent => ({
  name: "trailbase/auth_ui",
  path: "components/auth_ui.wasm",
  loaded: true,
  installed: true,
  ...overrides,
});

const renderList = (components: WasmComponent[], options = {}) =>
  render(() => (
    <WasmComponentsList
      components={components}
      isLoading={false}
      isError={false}
      refetch={vi.fn()}
      {...options}
    />
  ));

afterEach(cleanup);

describe("WASM workspace UI", () => {
  it("keeps the workspace header and refresh action visible while loading", () => {
    renderList([], { isLoading: true });
    expect(
      screen.getByRole("heading", { name: "WASM Components" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /refresh wasm components/i }),
    ).toBeInTheDocument();
    expect(screen.getByTitle("Loading")).toBeInTheDocument();
  });

  it("does not render boolean text before SVG icons", () => {
    renderList([
      component({ icon: '<svg aria-label="component icon"></svg>' }),
    ]);
    expect(screen.queryByText("true")).not.toBeInTheDocument();
    expect(screen.getByLabelText("component icon")).toBeInTheDocument();
  });

  it("renders states as accessible badges", () => {
    renderList([
      component(),
      component({ name: "available", loaded: false, installed: false }),
      component({ name: "pending", loaded: false, installed: true }),
      component({ name: "removing", loaded: true, installed: false }),
    ]);
    expect(screen.getByText("Running")).toHaveClass("bg-success");
    expect(screen.getByText("Available")).toHaveClass("bg-secondary");
    expect(screen.getByText("Install pending restart")).toHaveClass(
      "bg-warning",
    );
    expect(screen.getByText("Removal pending restart")).toHaveClass(
      "bg-warning",
    );
  });

  it("shows visible guidance headings and warning styling for restart-required changes", () => {
    renderList([component({ loaded: false, installed: true })]);
    expect(
      screen.getByRole("heading", { name: "Restart required" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Restart required" }).parentElement,
    ).toHaveClass("bg-warning");
  });

  it("renders accessible safe error copy and retry", () => {
    render(() => (
      <WasmComponentsList
        components={[]}
        isLoading={false}
        isError={true}
        refetch={vi.fn()}
      />
    ));
    expect(
      screen.getByRole("heading", { name: "Unable to load WASM components" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/WASM components could not be loaded/i),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });
});
