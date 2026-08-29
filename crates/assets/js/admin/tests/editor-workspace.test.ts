import { describe, expect, it } from "vitest";
import type { QueryResponse } from "@bindings/QueryResponse";
import type { ExecutionResult } from "@/lib/api/execute";
import { tryFormatUuidBlob } from "@/lib/value";
import {
  buildCsv,
  DARK_SQL_COLORS,
  detectUuidColumnVersion,
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
  it("infers UUID result columns only when every blob is a UUID", () => {
    const uuidColumn = {
      columns: [
        {
          name: "id",
          type_name: "",
          data_type: "Blob",
          affinity_type: "Blob",
          options: [],
        },
      ],
      rows: [
        [
          {
            Blob: {
              Hex: "2964a4cecc134813963dc64d0d5db8c9",
            },
          },
        ],
        ["Null"],
      ],
    } as QueryResponse;
    expect(detectUuidColumnVersion(uuidColumn, 0)).toBe(4);

    uuidColumn.rows.push([{ Blob: { Array: Array(16).fill(0) } }]);
    expect(detectUuidColumnVersion(uuidColumn, 0)).toBeUndefined();
  });

  it("recognizes only supported UUID blobs", () => {
    expect(
      tryFormatUuidBlob({
        Array: [
          0x29, 0x64, 0xa4, 0xce, 0xcc, 0x13, 0x48, 0x13, 0x96, 0x3d, 0xc6,
          0x4d, 0x0d, 0x5d, 0xb8, 0xc9,
        ],
      }),
    ).toEqual({ value: "2964a4ce-cc13-4813-963d-c64d0d5db8c9", version: 4 });
    expect(
      tryFormatUuidBlob({ Base64UrlSafe: "KWSkzswTSBOWPcZNDV24yQ==" }),
    ).toEqual({ value: "2964a4ce-cc13-4813-963d-c64d0d5db8c9", version: 4 });
    expect(
      tryFormatUuidBlob({ Hex: "01941f297c0073e4a310744d2167fc5b" }),
    ).toEqual({ value: "01941f29-7c00-73e4-a310-744d2167fc5b", version: 7 });
    expect(tryFormatUuidBlob({ Array: Array(16).fill(0) })).toBeUndefined();
    expect(tryFormatUuidBlob({ Array: [1, 2, 3] })).toBeUndefined();
  });

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
