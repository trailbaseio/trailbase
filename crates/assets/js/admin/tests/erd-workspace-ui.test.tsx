import { render, screen, fireEvent, cleanup } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";

const queryState = vi.hoisted(() => ({
  isPending: false,
  isError: false,
  error: undefined as unknown,
  data: undefined as ListSchemasResponse | undefined,
  refetch: vi.fn(),
}));

vi.mock("@/lib/api/table", () => ({
  createTableSchemaQuery: () => queryState,
}));
vi.mock("@antv/x6", () => ({ Graph: class {} }));
vi.mock("@/components/erd/ErdGraph", () => ({
  ErdGraph: () => null,
  nodeName: () => "rect",
  NODE_WIDTH: 320,
  LINE_HEIGHT: 24,
}));

import { ErdPage, ErdToolbar } from "@/components/erd/ErdPage";

beforeEach(() => {
  queryState.isPending = false;
  queryState.isError = false;
  queryState.error = undefined;
  queryState.data = undefined;
  queryState.refetch.mockReset();
});

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
        showTables={true}
        showViews={true}
        selectedId={undefined}
        onShowTablesChange={vi.fn()}
        onShowViewsChange={vi.fn()}
        onSelect={onSelect}
        onZoomIn={vi.fn()}
        onZoomOut={vi.fn()}
        onFit={vi.fn()}
        onReset={vi.fn()}
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
    const activeOption = screen.getByRole("option", {
      name: /main\.posts.*table/i,
    });
    expect(activeOption).toHaveAttribute("aria-selected", "true");
    expect(search).toHaveAttribute(
      "aria-activedescendant",
      "erd-search-option-main.posts",
    );
    await fireEvent.keyDown(search, { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith("main.posts");
  });

  it("clears active descendant when there are no matches", async () => {
    render(() => (
      <ErdToolbar
        entities={entities}
        showTables={true}
        showViews={true}
        onShowTablesChange={vi.fn()}
        onShowViewsChange={vi.fn()}
        onSelect={vi.fn()}
        onZoomIn={vi.fn()}
        onZoomOut={vi.fn()}
        onFit={vi.fn()}
        onReset={vi.fn()}
      />
    ));
    const search = screen.getByRole("combobox", { name: /search entities/i });
    await fireEvent.input(search, { target: { value: "missing" } });
    expect(search).toHaveAttribute("aria-expanded", "false");
    expect(search).not.toHaveAttribute("aria-controls");
    expect(search).not.toHaveAttribute("aria-activedescendant");
    expect(screen.queryByRole("option")).not.toBeInTheDocument();
  });

  it("clamps the active option when reactive results shrink or change", async () => {
    const [currentEntities, setEntities] = createSignal(entities);
    render(() => (
      <ErdToolbar
        entities={currentEntities()}
        showTables={true}
        showViews={true}
        onShowTablesChange={vi.fn()}
        onShowViewsChange={vi.fn()}
        onSelect={vi.fn()}
        onZoomIn={vi.fn()}
        onZoomOut={vi.fn()}
        onFit={vi.fn()}
        onReset={vi.fn()}
      />
    ));
    const search = screen.getByRole("combobox", { name: /search entities/i });
    await fireEvent.input(search, { target: { value: "main" } });
    await fireEvent.keyDown(search, { key: "ArrowDown" });
    await fireEvent.keyDown(search, { key: "ArrowDown" });
    await fireEvent.keyDown(search, { key: "ArrowDown" });
    expect(search).toHaveAttribute(
      "aria-activedescendant",
      "erd-search-option-main.users",
    );

    setEntities([entities[0]]);
    expect(search).toHaveAttribute(
      "aria-activedescendant",
      "erd-search-option-main.posts",
    );
    expect(screen.getByRole("option")).toHaveAttribute("aria-selected", "true");

    setEntities([]);
    expect(search).not.toHaveAttribute("aria-activedescendant");
  });

  it("closes search before clearing selection on Escape", async () => {
    const onSelect = vi.fn();
    render(() => (
      <ErdToolbar
        entities={entities}
        showTables={true}
        showViews={true}
        selectedId="main.posts"
        onShowTablesChange={vi.fn()}
        onShowViewsChange={vi.fn()}
        onSelect={onSelect}
        onZoomIn={vi.fn()}
        onZoomOut={vi.fn()}
        onFit={vi.fn()}
        onReset={vi.fn()}
      />
    ));
    const search = screen.getByRole("combobox", { name: /search entities/i });
    await fireEvent.focus(search);
    await fireEvent.keyDown(search, { key: "Escape" });
    expect(search).toHaveAttribute("aria-expanded", "false");
    expect(onSelect).not.toHaveBeenCalled();

    await fireEvent.keyDown(search, { key: "Escape" });
    expect(onSelect).toHaveBeenCalledWith(undefined);
  });

  it("supports visibility toggles and graph actions", async () => {
    const callbacks = {
      onShowTablesChange: vi.fn(),
      onShowViewsChange: vi.fn(),
      onSelect: vi.fn(),
      onZoomIn: vi.fn(),
      onZoomOut: vi.fn(),
      onFit: vi.fn(),
      onReset: vi.fn(),
    };
    render(() => (
      <ErdToolbar
        entities={entities}
        showTables={true}
        showViews={false}
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
    expect(callbacks.onShowTablesChange).toHaveBeenCalledWith(false);
    expect(callbacks.onShowViewsChange).toHaveBeenCalledWith(true);
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

const emptySchema: ListSchemasResponse = {
  tables: [],
  views: [],
  indexes: [],
  triggers: [],
};

const populatedSchema: ListSchemasResponse = {
  tables: [
    [
      {
        name: { name: "users", database_schema: "main" },
        strict: true,
        columns: [],
        foreign_keys: [],
        unique: [],
        checks: [],
        virtual_table: false,
        temporary: false,
      },
      "",
    ],
    [
      {
        name: { name: "posts", database_schema: "main" },
        strict: true,
        columns: [
          {
            name: "author_id",
            type_name: "BLOB",
            data_type: "Blob",
            affinity_type: "Blob",
            options: [
              {
                ForeignKey: {
                  foreign_table: "users",
                  referred_columns: ["id"],
                  on_delete: null,
                  on_update: null,
                },
              },
            ],
          },
        ],
        foreign_keys: [],
        unique: [],
        checks: [],
        virtual_table: false,
        temporary: false,
      },
      "",
    ],
  ],
  views: [
    [
      {
        name: { name: "post_summary", database_schema: "main" },
        column_mapping: null,
        query: "SELECT * FROM posts",
        temporary: false,
      },
      "",
    ],
  ],
  indexes: [],
  triggers: [],
};

describe("ERD page states", () => {
  it("keeps the workspace visible while loading", () => {
    queryState.isPending = true;
    render(() => <ErdPage />);

    expect(screen.getByRole("heading", { name: "ERD" })).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "Search entities" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Loading schema")).toBeInTheDocument();
  });

  it("shows a retryable schema error", async () => {
    queryState.isError = true;
    queryState.error = new Error("offline");
    render(() => <ErdPage />);

    expect(screen.getByText("Unable to load schema")).toBeInTheDocument();
    expect(
      screen.getByText("TrailBase couldn't load the database schema."),
    ).toBeInTheDocument();
    expect(screen.queryByText("Error: offline")).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(queryState.refetch).toHaveBeenCalledOnce();
  });

  it("shows an empty schema state", () => {
    queryState.data = emptySchema;
    render(() => <ErdPage />);

    expect(screen.getByText("No schema entities")).toBeInTheDocument();
  });

  it("shows counts and recovers when filters hide every entity", async () => {
    queryState.data = populatedSchema;
    render(() => <ErdPage />);

    expect(
      screen.getByText("2 tables · 1 view · 1 relationship"),
    ).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Tables" }));
    await fireEvent.click(screen.getByRole("button", { name: "Views" }));
    expect(
      screen.getByText("No entities match these filters"),
    ).toBeInTheDocument();

    await fireEvent.click(
      screen.getByRole("button", { name: "Show all entities" }),
    );
    expect(
      screen.queryByText("No entities match these filters"),
    ).not.toBeInTheDocument();
  });
});
