import { render, screen, fireEvent, cleanup } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@antv/x6", () => ({ Graph: class {} }));
vi.mock("@/components/erd/ErdGraph", () => ({
  ErdGraph: () => null,
  nodeName: () => "rect",
  NODE_WIDTH: 320,
  LINE_HEIGHT: 24,
}));

import { ErdToolbar } from "@/components/erd/ErdPage";

afterEach(cleanup);

const entities = [
  { id: "main.posts", name: "main.posts", type: "table" as const },
  { id: "main.post_summary", name: "main.post_summary", type: "view" as const },
  { id: "main.users", name: "main.users", type: "table" as const },
];

describe("ERD toolbar", () => {
  it("filters and selects entities with keyboard navigation", async () => {
    const onSelect = vi.fn();
    render(() => (
      <ErdToolbar
        entities={entities}
        tables={true}
        views={true}
        selectedId={undefined}
        onChange={vi.fn()}
        onSelect={onSelect}
      />
    ));
    const search = screen.getByRole("combobox", { name: /search entities/i });
    await fireEvent.input(search, { target: { value: "post" } });
    expect(
      screen.getByRole("option", { name: /main\.posts.*table/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: /main\.users.*table/i }),
    ).not.toBeInTheDocument();
    await fireEvent.keyDown(search, { key: "ArrowDown" });
    await fireEvent.keyDown(search, { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith("main.posts");
  });

  it("closes search and clears selection on Escape", async () => {
    const onSelect = vi.fn();
    render(() => (
      <ErdToolbar
        entities={entities}
        tables={true}
        views={true}
        selectedId="main.posts"
        onChange={vi.fn()}
        onSelect={onSelect}
      />
    ));
    const search = screen.getByRole("combobox", { name: /search entities/i });
    await fireEvent.focus(search);
    await fireEvent.keyDown(search, { key: "Escape" });
    expect(search).toHaveAttribute("aria-expanded", "false");
    expect(onSelect).toHaveBeenCalledWith(undefined);
  });

  it("supports visibility toggles and graph actions", async () => {
    const callbacks = {
      onChange: vi.fn(),
      onSelect: vi.fn(),
      onZoomIn: vi.fn(),
      onZoomOut: vi.fn(),
      onFit: vi.fn(),
      onReset: vi.fn(),
    };
    render(() => (
      <ErdToolbar
        entities={entities}
        tables={true}
        views={false}
        selectedId={undefined}
        {...callbacks}
      />
    ));
    expect(screen.getByRole("button", { name: "Tables" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Views" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    await fireEvent.click(screen.getByRole("button", { name: "Tables" }));
    await fireEvent.click(screen.getByRole("button", { name: "Views" }));
    expect(callbacks.onChange).toHaveBeenNthCalledWith(1, {
      tables: false,
      views: false,
    });
    expect(callbacks.onChange).toHaveBeenNthCalledWith(2, {
      tables: true,
      views: true,
    });
    await fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    await fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    await fireEvent.click(screen.getByRole("button", { name: "Fit view" }));
    await fireEvent.click(screen.getByRole("button", { name: "Reset layout" }));
    expect(callbacks.onZoomIn).toHaveBeenCalled();
    expect(callbacks.onZoomOut).toHaveBeenCalled();
    expect(callbacks.onFit).toHaveBeenCalled();
    expect(callbacks.onReset).toHaveBeenCalled();
  });
});
