import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createComponent } from "solid-js";
import { describe, expect, it, vi } from "vitest";

vi.mock("@antv/x6", () => ({
  Graph: class {
    static registerPortLayout() {}
    static registerNode() {}
  },
}));
vi.mock("@/components/erd/ErdGraph", async () => {
  const actual = await vi.importActual<
    typeof import("@/components/erd/ErdGraph")
  >("@/components/erd/ErdGraph");
  return { ...actual, ErdGraph: () => null };
});

import {
  buildErdModel,
  ErdToolbar,
  relatedEntityIds,
  searchErdEntities,
  selectionStatus,
} from "@/components/erd/ErdPage";
import {
  focusedErdIds,
  layoutErdNodes,
  setErdNodeOpacity,
} from "@/components/erd/ErdGraph";
import type { Column } from "@bindings/Column";
import type { ColumnOption } from "@bindings/ColumnOption";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { Table } from "@bindings/Table";
import type { View } from "@bindings/View";

function column(name: string, options: ColumnOption[] = []): Column {
  return {
    name,
    type_name: "TEXT",
    data_type: "Text",
    affinity_type: "Text",
    options,
  };
}

function table(
  name: string,
  columns: Column[],
  databaseSchema = "main",
): Table {
  return {
    name: { name, database_schema: databaseSchema },
    strict: true,
    columns,
    foreign_keys: [],
    unique: [],
    checks: [],
    virtual_table: false,
    temporary: false,
  };
}

function view(name: string, columns: Column[], databaseSchema = "main"): View {
  return {
    name: { name, database_schema: databaseSchema },
    column_mapping: {
      columns: columns.map((column) => ({
        column,
        parent_name: null,
        aggregation: null,
      })),
      group_by: null,
      joins: [],
    },
    query: `SELECT * FROM ${name}`,
    temporary: false,
  };
}

function schema(tables: Table[], views: View[] = []): ListSchemasResponse {
  return {
    tables: tables.map((table) => [table, ""]),
    views: views.map((view) => [view, ""]),
    indexes: [],
    triggers: [],
  };
}

describe("ERD workspace model", () => {
  it("applies opacity to node markup and every rendered port row", () => {
    const attr = vi.fn();
    const setPortProp = vi.fn();
    const node = {
      attr,
      getPorts: () => [{ id: "id" }, { id: "title" }],
      setPortProp,
    };

    setErdNodeOpacity(node, 0.28);

    expect(attr).toHaveBeenCalledWith({
      body: { opacity: 0.28 },
      label: { opacity: 0.28 },
      typeLabel: { opacity: 0.28 },
    });
    for (const portId of ["id", "title"]) {
      expect(setPortProp).toHaveBeenCalledWith(portId, {
        attrs: {
          portBody: { opacity: 0.28 },
          portNameLabel: { opacity: 0.28 },
          portTypeLabel: { opacity: 0.28 },
        },
      });
    }
  });

  it("lays out nodes immutably using the available aspect ratio", () => {
    const nodes = [
      { id: "a", width: 10 },
      { id: "b", width: 10 },
    ];

    expect(layoutErdNodes(nodes, 10).map((node) => node.position)).toEqual([
      { x: 0, y: 0 },
      { x: 270, y: 0 },
    ]);
    expect(layoutErdNodes(nodes, 0.1).map((node) => node.position)).toEqual([
      { x: 0, y: 0 },
      { x: 0, y: 34 },
    ]);
    expect(nodes).toEqual([
      { id: "a", width: 10 },
      { id: "b", width: 10 },
    ]);
    expect(layoutErdNodes([], 1)).toEqual([]);
  });

  it("preserves explicit positions while laying out new nodes", () => {
    expect(
      layoutErdNodes(
        [{ id: "a", position: { x: 12, y: 34 } }, { id: "b" }],
        10,
      ).map((node) => node.position),
    ).toEqual([
      { x: 12, y: 34 },
      { x: 270, y: 0 },
    ]);
  });

  it("focuses only the selected entity and its direct neighbors", () => {
    expect(
      focusedErdIds(
        [
          { sourceId: "posts", targetId: "users" },
          { sourceId: "comments", targetId: "posts" },
          { sourceId: "users", targetId: "roles" },
        ],
        "posts",
      ),
    ).toEqual(new Set(["posts", "users", "comments"]));
    expect(focusedErdIds([], undefined)).toEqual(new Set());
  });

  it("returns table and view entities with visible counts", () => {
    const model = buildErdModel(
      schema(
        [table("posts", [column("id")])],
        [view("post_summary", [column("id")])],
      ),
      { tables: true, views: true },
    );

    expect(model.entities).toEqual([
      { id: "posts", name: "posts", type: "table" },
      { id: "post_summary", name: "post_summary", type: "view" },
    ]);
    expect(model.tableCount).toBe(1);
    expect(model.viewCount).toBe(1);
    expect(model.nodes).toHaveLength(2);
  });

  it("gives duplicate view columns stable unique port IDs", () => {
    const model = buildErdModel(
      schema(
        [],
        [view("duplicate_columns", [column("value"), column("value")])],
      ),
      { tables: true, views: true },
    );
    const ports = model.nodes[0]?.ports;

    expect(ports).toBeInstanceOf(Array);
    expect((ports as { id?: string }[]).map(({ id }) => id)).toEqual([
      "duplicate_columns-value-0",
      "duplicate_columns-value-1",
    ]);
  });

  it("excludes hidden internals but keeps main._user", () => {
    const model = buildErdModel(
      schema([
        table("_internal", []),
        table("sqlite_stat1", []),
        table("_user", []),
      ]),
      { tables: true, views: true },
    );

    expect(model.entities.map(({ id }) => id)).toEqual(["_user"]);
  });

  it("removes hidden views and dangling relations", () => {
    const source = table("posts", [
      column("summary_id", [
        {
          ForeignKey: {
            foreign_table: "post_summary",
            referred_columns: ["id"],
            on_delete: null,
            on_update: null,
          },
        },
      ]),
    ]);
    const summary = view("post_summary", [column("id")]);

    const model = buildErdModel(schema([source], [summary]), {
      tables: true,
      views: false,
    });

    expect(model.entities.map(({ id }) => id)).toEqual(["posts"]);
    expect(model.nodes).toHaveLength(1);
    expect(model.edges).toEqual([]);
    expect(model.relations).toEqual([]);
  });

  it("creates stable relation IDs for foreign keys", () => {
    const users = table("users", [column("id")]);
    const posts = table("posts", [
      column("author_id", [
        {
          ForeignKey: {
            foreign_table: "users",
            referred_columns: ["id"],
            on_delete: null,
            on_update: null,
          },
        },
      ]),
    ]);

    const model = buildErdModel(schema([posts, users]), {
      tables: true,
      views: true,
    });

    expect(model.relations).toEqual([{ sourceId: "posts", targetId: "users" }]);
    expect(model.edges).toHaveLength(1);
  });

  it("returns the selected entity and its direct neighbors", () => {
    expect(
      relatedEntityIds(
        [
          { sourceId: "posts", targetId: "users" },
          { sourceId: "comments", targetId: "posts" },
          { sourceId: "users", targetId: "roles" },
        ],
        "posts",
      ),
    ).toEqual(new Set(["posts", "users", "comments"]));
  });

  it("formats the polite focus status", () => {
    const entities = [{ id: "posts", name: "posts", type: "table" as const }];
    const relations = [{ sourceId: "posts", targetId: "users" }];

    expect(selectionStatus(entities, relations)).toBe("No entity focused");
    expect(selectionStatus(entities, relations, "posts")).toBe(
      "posts focused, 1 direct relationships",
    );
  });

  it("wires search selection and toolbar actions", async () => {
    const onSelect = vi.fn();
    const onReset = vi.fn();
    render(() =>
      createComponent(ErdToolbar, {
        entities: [{ id: "posts", name: "posts", type: "table" }],
        showTables: true,
        showViews: false,
        onShowTablesChange: vi.fn(),
        onShowViewsChange: vi.fn(),
        onSelect,
        onZoomIn: vi.fn(),
        onZoomOut: vi.fn(),
        onFit: vi.fn(),
        onReset,
      }),
    );
    await fireEvent.input(screen.getByRole("combobox"), {
      target: { value: "post" },
    });
    await fireEvent.click(screen.getByRole("option", { name: /posts/ }));
    expect(onSelect).toHaveBeenCalledWith("posts");
    await fireEvent.click(screen.getByRole("button", { name: "Reset layout" }));
    expect(onReset).toHaveBeenCalledOnce();
  });

  it("searches qualified names case-insensitively and returns all for an empty query", () => {
    const entities = [
      { id: "main.posts", name: "main.posts", type: "table" as const },
      {
        id: "analytics.PostSummary",
        name: "analytics.PostSummary",
        type: "view" as const,
      },
    ];

    expect(searchErdEntities(entities, "POSTSUMMARY")).toEqual([entities[1]]);
    expect(searchErdEntities(entities, "   ")).toBe(entities);
  });
});
