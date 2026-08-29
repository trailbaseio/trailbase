import { describe, expect, it } from "vitest";
import type { QueryResponse } from "@bindings/QueryResponse";
import type { ExecutionResult } from "@/lib/api/execute";
import {
  buildCsv,
  DARK_SQL_COLORS,
  filterSavedQueries,
  nextEditorTabAfterExecution,
  paginateResultRows,
  resultPresentation,
  type Script,
} from "@/components/editor/EditorPage";

const scripts: Script[] = [
  { name: "Active users", contents: "SELECT 1" },
  { name: "Recent posts", contents: "SELECT 2" },
];

const response = (rows: QueryResponse["rows"]): QueryResponse =>
  ({
    columns: [
      {
        name: "full,name",
        type_name: "TEXT",
        data_type: "Text",
        affinity_type: "Text",
        options: [],
      },
    ],
    rows,
  }) as QueryResponse;

const result = (data?: QueryResponse): ExecutionResult => ({
  query: "SELECT 1",
  timestamp: 1,
  data,
});

function relativeLuminance(hex: string): number {
  const [r, g, b] = hex
    .match(/[a-f\d]{2}/gi)!
    .map((value) => Number.parseInt(value, 16) / 255)
    .map((value) =>
      value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
    );
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRatio(foreground: string, background: string): number {
  const lighter = Math.max(
    relativeLuminance(foreground),
    relativeLuminance(background),
  );
  const darker = Math.min(
    relativeLuminance(foreground),
    relativeLuminance(background),
  );
  return (lighter + 0.05) / (darker + 0.05);
}

describe("SQL editor workspace", () => {
  it("uses accessible dark syntax colors", () => {
    for (const color of Object.values(DARK_SQL_COLORS)) {
      expect(contrastRatio(color, "#000000")).toBeGreaterThanOrEqual(4.5);
    }
  });
  it("filters saved queries case-insensitively", () => {
    expect(filterSavedQueries(scripts, "USER")).toEqual([scripts[0]]);
  });

  it("treats blank search as no filter", () => {
    expect(filterSavedQueries(scripts, "  ")).toEqual(scripts);
  });

  it("escapes CSV headers and values", () => {
    expect(buildCsv(response([[{ Text: 'Ada "Lovelace"' }]]))).toBe(
      '"full,name"\n"Ada ""Lovelace"""',
    );
  });

  it("describes query result states", () => {
    const success = result(response([[{ Text: "Ada" }]]));
    const empty = result(response([]));
    const noData = result({ columns: null, rows: [] });
    const failure: ExecutionResult = {
      query: "broken",
      timestamp: 1,
      error: { code: 400, message: "syntax error" },
    };

    expect(resultPresentation(undefined, false).label).toBe("No result");
    expect(resultPresentation(success, false, true).label).toBe("Running…");
    expect(resultPresentation(success, true).label).toBe("Cached result");
    expect(resultPresentation(success, false).label).toBe("Success");
    expect(resultPresentation(empty, false).label).toBe("No rows");
    expect(resultPresentation(noData, false).label).toBe("No data");
    expect(resultPresentation(failure, false).label).toBe("Error");
  });

  it("opens results after mobile execution only", () => {
    expect(nextEditorTabAfterExecution(true, "editor")).toBe("results");
    expect(nextEditorTabAfterExecution(false, "editor")).toBe("editor");
  });

  it("slices result rows for client-side pagination", () => {
    expect(paginateResultRows([0, 1, 2, 3], 1, 2)).toEqual([2, 3]);
    expect(paginateResultRows([0, 1, 2], 99, 2)).toEqual([2]);
    expect(paginateResultRows([0, 1, 2], -1, 2)).toEqual([0, 1]);
    expect(paginateResultRows([0, 1, 2], 0, 3)).toEqual([0, 1, 2]);
  });
});
