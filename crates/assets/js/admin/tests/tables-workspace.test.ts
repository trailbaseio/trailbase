import { describe, expect, it } from "vitest";

import {
  filterExplorerResources,
  groupExplorerResources,
  resourceSchemaName,
} from "@/components/tables/TablesPage";
import {
  normalizeWorkspaceTab,
  workspaceTabSearchParams,
} from "@/components/tables/TablePane";

import type { Table } from "@bindings/Table";

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
