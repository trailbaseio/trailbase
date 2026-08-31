import { describe, expect, it } from "vitest";

import {
  filterExplorerResources,
  groupExplorerResources,
  resourceSchemaName,
} from "@/components/tables/TablesPage";
import {
  normalizeWorkspaceTab,
  tableApiSummary,
  tableStructureCounts,
  workspaceTabSearchParams,
} from "@/components/tables/TablePane";

import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { Table } from "@bindings/Table";
import { Config } from "@proto/config";

function table(name: string, databaseSchema?: string | null): Table {
  return {
    name: { name, database_schema: databaseSchema ?? null },
    strict: false,
    columns: [],
    foreign_keys: [],
    unique: [],
    checks: [],
    virtual_table: false,
    temporary: false,
  };
}

describe("Tables workspace", () => {
  it("defaults invalid workspace tabs to data", () => {
    expect(normalizeWorkspaceTab(undefined)).toBe("data");
    expect(normalizeWorkspaceTab("data")).toBe("data");
    expect(normalizeWorkspaceTab("structure")).toBe("structure");
    expect(normalizeWorkspaceTab("api")).toBe("api");
    expect(normalizeWorkspaceTab("unknown")).toBe("data");
  });

  it("updates the workspace tab without dropping query state", () => {
    expect(
      workspaceTabSearchParams(
        { filter: "id > 2", pageSize: "50", tab: "structure" },
        "api",
      ),
    ).toEqual({ filter: "id > 2", pageSize: "50", tab: "api" });

    expect(workspaceTabSearchParams({ filter: "x" }, "data")).toEqual({
      filter: "x",
      tab: undefined,
    });
  });

  it("counts structure objects by qualified table name", () => {
    const authUsers = table("users", "auth");
    authUsers.columns = [
      {
        name: "id",
        type_name: "INTEGER",
        data_type: "Integer",
        affinity_type: "Integer",
        options: [],
      },
    ];
    const schemas: ListSchemasResponse = {
      tables: [[authUsers, ""]],
      views: [],
      indexes: [
        [
          {
            name: { name: "users_idx", database_schema: "auth" },
            table_name: "users",
            columns: [],
            unique: false,
            predicate: null,
          },
          "",
        ],
        [
          {
            name: { name: "users_idx", database_schema: "main" },
            table_name: "users",
            columns: [],
            unique: false,
            predicate: null,
          },
          "",
        ],
      ],
      triggers: [
        [
          {
            name: { name: "users_trigger", database_schema: "auth" },
            table_name: "users",
          },
          "CREATE TRIGGER users_trigger",
        ],
        [
          {
            name: { name: "users_trigger", database_schema: "main" },
            table_name: "users",
          },
          "CREATE TRIGGER users_trigger",
        ],
      ],
    };

    expect(tableStructureCounts(authUsers, schemas)).toEqual({
      columns: 1,
      indexes: 1,
      triggers: 1,
    });
  });

  it("summarizes Record API support and configured names", () => {
    const users = table("users", "auth");
    users.strict = true;
    users.columns = [
      {
        name: "id",
        type_name: "INTEGER",
        data_type: "Integer",
        affinity_type: "Integer",
        options: [
          {
            Unique: { is_primary: true, conflict_clause: null },
          },
        ],
      },
    ];

    expect(tableApiSummary(users, [users], undefined)).toMatchObject({
      supported: true,
      enabled: false,
      names: [],
      errors: [],
    });

    const config = Config.fromPartial({
      recordApis: [{ name: "users", tableName: "auth.users" }],
    });
    expect(tableApiSummary(users, [users], config)).toMatchObject({
      supported: true,
      enabled: true,
      names: ["users"],
    });

    const virtual = table("search_index");
    virtual.virtual_table = true;
    expect(tableApiSummary(virtual, [virtual], undefined)).toMatchObject({
      supported: false,
      enabled: false,
      errors: ["Virtual tables are not supported"],
    });
  });

  it("filters resources case-insensitively by qualified name", () => {
    const resources = [table("post", "main"), table("UserProfile", "auth")];

    expect(filterExplorerResources(resources, "profile")).toEqual([
      resources[1],
    ]);
    expect(filterExplorerResources(resources, "AUTH.")).toEqual([resources[1]]);
  });

  it("keeps empty-search resource ordering", () => {
    const resources = [table("zebra"), table("alpha", "auth")];

    expect(filterExplorerResources(resources, "   ")).toBe(resources);
  });

  it("uses stable schema labels and groups in first-seen order", () => {
    const resources = [
      table("users", "auth"),
      table("posts"),
      table("sessions", "auth"),
      table("comments", "main"),
    ];

    expect(resourceSchemaName(resources[1])).toBe("main");
    expect(groupExplorerResources(resources)).toEqual([
      ["auth", [resources[0], resources[2]]],
      ["main", [resources[1], resources[3]]],
    ]);
  });
});
