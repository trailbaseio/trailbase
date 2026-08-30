import { cleanup, render, screen, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WasmComponent } from "@bindings/WasmComponent";

vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; children: import("solid-js").JSX.Element }) => (
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

  it("renders counts, semantic columns, and responsive source details", () => {
    const refetch = vi.fn();
    const rows = [
      component({
        display_name: "Auth UI",
        description: "Authentication dashboard",
        guest_runtime: "wasi",
        version: "1.2.3",
        repo_id: "github.com/trailbase/auth_ui",
        admin_ui_path: "/dashboard",
      }),
      component({
        name: "pending",
        loaded: false,
        installed: true,
        display_name: "Pending UI",
      }),
    ];
    render(() => (
      <WasmComponentsList
        components={rows}
        isLoading={false}
        isError={false}
        refetch={refetch}
      />
    ));
    expect(screen.getByText("2 total · 1 running")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /refresh wasm components/i }),
    ).toBeInTheDocument();
    screen.getByRole("button", { name: /refresh wasm components/i }).click();
    expect(refetch).toHaveBeenCalledOnce();
    expect(
      screen.getByRole("columnheader", { name: "Component" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: "State" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: "Runtime / Version" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: "Source" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: "Actions" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Auth UI")).toBeInTheDocument();
    expect(
      screen.getByText("Internal name: trailbase/auth_ui"),
    ).toBeInTheDocument();
    expect(screen.getByText("Authentication dashboard")).toBeInTheDocument();
    expect(screen.getByText("wasi / 1.2.3")).toBeInTheDocument();
    expect(screen.getByText("github.com/trailbase/auth_ui")).toHaveClass(
      "select-text",
    );
    expect(
      screen.getByRole("link", { name: /open dashboard/i }),
    ).toHaveAttribute("href", "/wasm/trailbase/auth_ui");
    const tableWrapper = screen.getByRole("table").parentElement?.parentElement;
    expect(tableWrapper).toHaveClass("min-w-0", "overflow-x-auto");
    const firstRow = screen.getAllByRole("row")[1];
    expect(within(firstRow).getByText("Pending UI")).toBeInTheDocument();
  });

  it("keeps CLI guidance stable for pending and empty states", () => {
    const { unmount } = renderList([
      component({ loaded: false, installed: true }),
    ]);
    expect(
      screen.getByText(
        /trail \[--depot=\.\.\] components add trailbase\/auth_ui/,
      ),
    ).toBeInTheDocument();
    unmount();
    renderList([]);
    expect(
      screen.getByText("No WASM components installed."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("trail components add trailbase/auth_ui"),
    ).toBeInTheDocument();
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

  it("does not show prominent restart guidance when registries are synchronized", () => {
    renderList([
      component(),
      component({ name: "available", loaded: false, installed: false }),
    ]);
    expect(screen.queryByText("Restart required")).not.toBeInTheDocument();
  });

  it("always renders the internal component name field", () => {
    renderList([component({ display_name: undefined })]);
    expect(screen.getByText(/Internal name:/)).toBeInTheDocument();
    expect(screen.getByText("trailbase/auth_ui")).toBeInTheDocument();
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
