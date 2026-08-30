import {
  cleanup,
  render,
  screen,
  within,
  waitFor,
} from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WasmComponent } from "@bindings/WasmComponent";

vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; children: import("solid-js").JSX.Element }) => (
    <a href={props.href}>{props.children}</a>
  ),
}));
vi.mock("@/lib/api/wasm-components", () => ({
  installWasmComponent: vi.fn().mockResolvedValue(undefined),
  uninstallWasmComponent: vi.fn().mockResolvedValue(undefined),
}));

import { WasmComponentsList } from "@/components/wasm/WasmComponentsList";
import {
  installWasmComponent,
  uninstallWasmComponent,
} from "@/lib/api/wasm-components";

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

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

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
    const table = screen.getByRole("table");
    const tableContainer = table.parentElement?.parentElement;
    expect(tableContainer).toHaveClass(
      "border",
      "rounded-md",
      "overflow-x-auto",
    );
    const tableWrapper = tableContainer?.parentElement;
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

  it("renders manifest SVG icons as inert images", () => {
    const { container } = renderList([
      component({
        icon: '<svg aria-label="component icon" onload="alert(1)"></svg>',
      }),
    ]);
    expect(screen.queryByText("true")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("component icon")).not.toBeInTheDocument();
    const icon = container.querySelector("img");
    expect(icon).toHaveAttribute(
      "src",
      expect.stringContaining("data:image/svg+xml"),
    );
    expect(icon).not.toHaveAttribute("onload");
  });

  it("disables refresh while loading", () => {
    const refetch = vi.fn();
    renderList([], { isLoading: true, refetch });
    const refresh = screen.getByRole("button", {
      name: /refresh wasm components/i,
    });
    expect(refresh).toBeDisabled();
    refresh.click();
    expect(refetch).not.toHaveBeenCalled();
  });

  it("offers eligible actions with exact names and awaits refresh", async () => {
    const refetch = vi.fn().mockResolvedValue(undefined);
    renderList(
      [
        component({
          name: "available",
          loaded: false,
          installed: false,
          repo_id: "repo/id",
        }),
        component({
          name: "no-repo",
          loaded: false,
          installed: false,
          repo_id: undefined,
        }),
        component({ name: "installed", installed: true }),
      ],
      { refetch },
    );
    expect(
      screen.getByRole("button", { name: "Install available" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Install no-repo" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Remove installed" }),
    ).toBeInTheDocument();

    screen.getByRole("button", { name: "Install available" }).click();
    expect(
      screen.getByRole("heading", { name: "Install available" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/requires a server restart/i)).toBeInTheDocument();
    screen.getByRole("button", { name: /^Install$/ }).click();
    await waitFor(() =>
      expect(installWasmComponent).toHaveBeenCalledWith({ RepoId: "repo/id" }),
    );
    await waitFor(() => expect(refetch).toHaveBeenCalledOnce());
    expect(
      screen.queryByRole("heading", { name: "Install available" }),
    ).not.toBeInTheDocument();
  });

  it("awaits uninstall and refetch before closing the removal dialog", async () => {
    let resolveUninstall!: () => void;
    const uninstall = new Promise<void>((resolve) => {
      resolveUninstall = resolve;
    });
    let resolveRefetch!: () => void;
    const refetch = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRefetch = resolve;
        }),
    );
    vi.mocked(uninstallWasmComponent).mockImplementationOnce(() => uninstall);

    renderList([component({ name: "loaded-component" })], { refetch });
    screen.getByRole("button", { name: "Remove loaded-component" }).click();
    screen.getByRole("button", { name: /^Remove$/ }).click();

    expect(
      screen.getByRole("button", { name: "Working…" }),
    ).toBeInTheDocument();
    expect(uninstallWasmComponent).toHaveBeenCalledWith({
      Path: "components/auth_ui.wasm",
    });
    expect(refetch).not.toHaveBeenCalled();
    expect(
      screen.getByRole("heading", { name: "Remove loaded-component" }),
    ).toBeInTheDocument();

    resolveUninstall();
    await waitFor(() => expect(refetch).toHaveBeenCalledOnce());
    expect(
      screen.getByRole("heading", { name: "Remove loaded-component" }),
    ).toBeInTheDocument();

    resolveRefetch();
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { name: "Remove loaded-component" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("retries only the refresh when a completed action cannot refresh the list", async () => {
    const refetch = vi
      .fn()
      .mockRejectedValueOnce(new Error("refresh secret"))
      .mockResolvedValueOnce(undefined);
    vi.mocked(installWasmComponent).mockResolvedValueOnce(undefined);

    renderList(
      [
        component({
          name: "available",
          loaded: false,
          installed: false,
          repo_id: "repo/id",
        }),
      ],
      { refetch },
    );
    screen.getByRole("button", { name: "Install available" }).click();
    screen.getByRole("button", { name: /^Install$/ }).click();

    await waitFor(() =>
      expect(
        screen.getByText(/changed, but the list could not be refreshed/i),
      ).toBeInTheDocument(),
    );
    expect(screen.queryByText("refresh secret")).not.toBeInTheDocument();
    expect(installWasmComponent).toHaveBeenCalledOnce();
    expect(refetch).toHaveBeenCalledOnce();
    expect(
      screen.getByRole("button", { name: "Retry refresh" }),
    ).toBeInTheDocument();

    screen.getByRole("button", { name: "Retry refresh" }).click();
    await waitFor(() => expect(refetch).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { name: "Install available" }),
      ).not.toBeInTheDocument(),
    );
    expect(installWasmComponent).toHaveBeenCalledOnce();
  });

  it("reports mutation failures without exposing backend details or retrying the action", async () => {
    vi.mocked(uninstallWasmComponent).mockRejectedValueOnce(
      new Error("mutation secret"),
    );
    renderList([component({ name: "installed" })]);
    screen.getByRole("button", { name: "Remove installed" }).click();
    screen.getByRole("button", { name: /^Remove$/ }).click();

    await waitFor(() =>
      expect(screen.getByText(/could not be removed/i)).toBeInTheDocument(),
    );
    expect(screen.queryByText("mutation secret")).not.toBeInTheDocument();
    expect(uninstallWasmComponent).toHaveBeenCalledOnce();
    expect(
      screen.queryByRole("button", { name: "Retry refresh" }),
    ).not.toBeInTheDocument();
  });

  it("offers removal copy for loaded instances", () => {
    renderList([component({ name: "loaded-component", installed: true })]);
    screen.getByRole("button", { name: "Remove loaded-component" }).click();
    expect(
      screen.getByRole("heading", { name: "Remove loaded-component" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/loaded instance continues until restart/i),
    ).toBeInTheDocument();
    expect(uninstallWasmComponent).not.toHaveBeenCalled();
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
