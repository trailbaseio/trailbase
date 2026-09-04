import { For } from "solid-js";
import { TbOutlineCopy, TbOutlineChevronDown } from "solid-icons/tb";

import { buttonVariants } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { showToast } from "@/components/ui/toast";

import { unpackSqlValue, sqlValueToString } from "@/lib/value";
import {
  copyToClipboard,
  stringToReadableStream,
  showSaveFileDialog,
} from "@/lib/utils";

import type { QueryResponse } from "@bindings/QueryResponse";

function buildDelimited(response: QueryResponse, delimiter: string): string {
  const escape = (value: string) => `"${value.replaceAll('"', '""')}"`;
  const lines: string[] = [];

  if (response.columns !== null) {
    lines.push(
      response.columns.map((column) => escape(column.name)).join(delimiter),
    );
  }
  for (const row of response.rows) {
    lines.push(
      row
        .map((value, _index) => escape(sqlValueToString(value)))
        .join(delimiter),
    );
  }
  return lines.join("\n");
}

function resultObjects(response: QueryResponse): Record<string, unknown>[] {
  const used = new Set<string>();
  const names = (response.columns ?? []).map((column) => {
    let name = column.name;
    for (let suffix = 2; used.has(name); suffix++)
      name = `${column.name}_${suffix}`;
    used.add(name);
    return name;
  });

  return response.rows.map((row) =>
    Object.fromEntries(
      names.map((name, index) => [name, unpackSqlValue(row[index])]),
    ),
  );
}

function buildJson(response: QueryResponse): string {
  return JSON.stringify(resultObjects(response), null, 2);
}

function buildJsonl(response: QueryResponse): string {
  return resultObjects(response)
    .map((row) => JSON.stringify(row))
    .join("\n");
}

type ResultExportFormat = "csv" | "tsv" | "json" | "jsonl";

function buildResultExport(
  response: QueryResponse,
  format: ResultExportFormat,
): string {
  switch (format) {
    case "csv":
      return buildDelimited(response, ", ");
    case "tsv":
      return buildDelimited(response, "\t");
    case "json":
      return buildJson(response);
    case "jsonl":
      return buildJsonl(response);
  }
}

function resultExportFilename(
  scriptName: string,
  format: ResultExportFormat,
): string {
  const name = scriptName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  return `${name || "query-results"}.${format}`;
}

export function ExportMenu(props: {
  data: QueryResponse | undefined;
  scriptName: string;
}) {
  const copy = (format: ResultExportFormat) => {
    if (props.data?.columns !== null && props.data !== undefined) {
      copyToClipboard(
        buildResultExport(props.data, format),
        false,
        `Copied ${formatName(format)}`,
      );
    }
  };

  const save = async (format: ResultExportFormat) => {
    if (props.data?.columns === null || props.data === undefined) return;

    try {
      await showSaveFileDialog({
        contents: async () =>
          stringToReadableStream(buildResultExport(props.data!, format)),
        filename: resultExportFilename(props.scriptName, format),
        mimeType: RESULT_EXPORT_MIME_TYPES[format],
      });
    } catch (error) {
      showToast({
        title: "Could not save results",
        description: error instanceof Error ? error.message : `${error}`,
        variant: "error",
      });
    }
  };

  return (
    <DropdownMenu placement="bottom-start">
      <DropdownMenuTrigger
        class={buttonVariants({ variant: "ghost", size: "sm" })}
        aria-label="Export results"
        disabled={props.data?.columns == null}
      >
        <div class="flex gap-1">
          <TbOutlineCopy />
          <span>Export</span>
          <TbOutlineChevronDown />
        </div>
      </DropdownMenuTrigger>

      <DropdownMenuContent>
        <DropdownMenuGroup>
          <DropdownMenuLabel>Copy to clipboard</DropdownMenuLabel>

          <For each={RESULT_EXPORT_FORMATS}>
            {(format) => {
              return (
                <DropdownMenuItem onSelect={() => copy(format)}>
                  {formatName(format)}
                </DropdownMenuItem>
              );
            }}
          </For>
        </DropdownMenuGroup>

        <DropdownMenuSeparator />

        <DropdownMenuGroup>
          <DropdownMenuLabel>Save to file</DropdownMenuLabel>

          <For each={RESULT_EXPORT_FORMATS}>
            {(format) => (
              <DropdownMenuItem onSelect={() => void save(format)}>
                {formatName(format)}
              </DropdownMenuItem>
            )}
          </For>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function formatName(format: ResultExportFormat): string {
  switch (format) {
    case "csv":
      return "CSV";
    case "tsv":
      return "TSV";
    case "json":
      return "JSON";
    case "jsonl":
      return "JSON Lines";
  }
}

const RESULT_EXPORT_FORMATS = ["csv", "tsv", "json", "jsonl"] as const;
const RESULT_EXPORT_MIME_TYPES: Record<ResultExportFormat, string> = {
  csv: "text/csv;charset=utf-8",
  tsv: "text/tab-separated-values;charset=utf-8",
  json: "application/json;charset=utf-8",
  jsonl: "application/x-ndjson;charset=utf-8",
};
