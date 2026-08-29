import {
  For,
  Match,
  Show,
  Switch,
  createMemo,
  createSignal,
  JSX,
} from "solid-js";
import type { Accessor, Signal } from "solid-js";
import {
  TbOutlineRefresh,
  TbOutlineTable,
  TbOutlineTrash,
  TbOutlineColumns,
  TbOutlineChevronRight,
  TbOutlineCopy,
  TbOutlineDotsVertical,
} from "solid-icons/tb";
import { A, useSearchParams } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import type { QueryObserverResult } from "@tanstack/solid-query";
import type {
  CellContext,
  ColumnDef,
  ColumnPinningState,
  PaginationState,
  Row,
  SortingState,
} from "@tanstack/solid-table";
import { createColumnHelper } from "@tanstack/solid-table";
import type { DialogTriggerProps } from "@kobalte/core/dialog";
import { urlSafeBase64Decode } from "trailbase";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Callout, CalloutContent, CalloutTitle } from "@/components/ui/callout";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { SheetContent, SheetTrigger } from "@/components/ui/sheet";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { SidebarTrigger, useSidebar } from "@/components/ui/sidebar";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { showToast } from "@/components/ui/toast";

import { DebugDialogButton } from "@/components/tables/SchemaDownload";
import { CreateAlterTableForm } from "@/components/tables/CreateAlterTable";
import { CreateAlterIndexForm } from "@/components/tables/CreateAlterIndex";
import { Table as TableComponent, buildTable } from "@/components/Table";
import type { Updater } from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";
import { InsertUpdateRowForm } from "@/components/tables/InsertUpdateRow";
import {
  RecordApiSettingsForm,
  getRecordApis,
  hasRecordApis,
} from "@/components/tables/RecordApiSettings";
import { SafeSheet } from "@/components/SafeSheet";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  type FileUpload,
  type FileUploads,
  UploadedFile,
  UploadedFiles,
} from "@/components/tables/Files";

import { createConfigQuery } from "@/lib/api/config";
import { wkbToWkt } from "@/lib/geometry";
import type { Record, ArrayRecord } from "@/lib/record";
import { hashSqlValue } from "@/lib/value";
import { urlSafeBase64ToUuid, toHex, safeParseInt } from "@/lib/utils";
import { equalQualifiedNames, TableType } from "@/lib/schema";
import { dropTable, dropIndex } from "@/lib/api/table";
import { deleteRows, fetchRows } from "@/lib/api/row";
import { formatSortingAsOrder } from "@/lib/list";
import {
  findPrimaryKeyColumnIndex,
  getDefaultValue,
  getForeignKey,
  getUnique,
  isPrimaryKeyColumn,
  isFileUploadColumn,
  isGeometryColumn,
  isFileUploadsColumn,
  isJSONColumn,
  isNotNull,
  isUUIDColumn,
  hiddenTable,
  tableType,
  validateViewRecordApiRequirements,
  validateTableRecordApiRequirements,
  prettyFormatQualifiedName,
} from "@/lib/schema";

import type { Column } from "@bindings/Column";
import type { ColumnDataType } from "@bindings/ColumnDataType";
import type { ListRowsResponse } from "@bindings/ListRowsResponse";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { QualifiedName } from "@bindings/QualifiedName";
import type { SqlValue } from "@bindings/SqlValue";
import type { Table } from "@bindings/Table";
import type { TableIndex } from "@bindings/TableIndex";
import type { TableTrigger } from "@bindings/TableTrigger";
import type { View } from "@bindings/View";
import type { Config } from "@proto/config";
import { createWritableMemo } from "@solid-primitives/memo";

type SimpleSignal<T> = [Accessor<T>, set: (state: T) => void];

const blobEncodings = ["base64", "hex", "mixed"] as const;
type BlobEncoding = (typeof blobEncodings)[number];

function rowDataToRow(columns: Column[], row: ArrayRecord): Record {
  const result: Record = {};
  for (let i = 0; i < row.length; ++i) {
    result[columns[i].name] = row[i];
  }
  return result;
}

function renderCell(
  context: CellContext<ArrayRecord, SqlValue>,
  tableName: QualifiedName,
  columns: Column[],
  pkIndex: number,
  cell: {
    column: Column;
    type: CellType;
  },
  blobEncoding: BlobEncoding,
  rowsRefetch: () => void,
): JSX.Element {
  const value: SqlValue = context.getValue();

  // Special handling for file columns.
  if (cell.type === "File") {
    let file: FileUpload | null;
    if (value === "Null") {
      file = null;
    } else if ("Text" in value) {
      file = JSON.parse(value.Text) as FileUpload;
    } else {
      throw new Error("expected JSON text");
    }

    const pkCol = columns[pkIndex].name;
    const pkVal = context.row.original[pkIndex];

    return (
      <UploadedFile
        file={file}
        tableName={tableName}
        columns={columns}
        columnName={cell.column.name}
        pk={{ columnName: pkCol, value: pkVal }}
        rowsRefetch={rowsRefetch}
      />
    );
  } else if (cell.type === "File[]") {
    let files: FileUploads;
    if (value === "Null") {
      files = [];
    } else if ("Text" in value) {
      files = JSON.parse(value.Text) as FileUploads;
    } else {
      throw new Error("expected JSON text");
    }

    const pkCol = columns[pkIndex].name;
    const pkVal = context.row.original[pkIndex];

    return (
      <UploadedFiles
        files={files}
        tableName={tableName}
        columns={columns}
        columnName={cell.column.name}
        pk={{ columnName: pkCol, value: pkVal }}
        rowsRefetch={rowsRefetch}
      />
    );
  }

  if (value === "Null") {
    return "NULL";
  }

  if ("Integer" in value) {
    return value.Integer.toString();
  }

  if ("Real" in value) {
    return value.Real.toString();
  }

  if ("Blob" in value) {
    const blob = value.Blob;
    if ("Base64UrlSafe" in blob) {
      switch (cell.type) {
        case "UUID": {
          return (
            <Uuid
              base64UrlSafeBlob={blob.Base64UrlSafe}
              blobEncoding={blobEncoding}
            />
          );
        }
        case "Geometry": {
          return wkbToWkt(urlSafeBase64Decode(blob.Base64UrlSafe));
        }
      }

      if (blobEncoding === "hex") {
        return toHex(urlSafeBase64Decode(blob.Base64UrlSafe));
      }
      return blob.Base64UrlSafe;
    }
    throw Error("Expected Base64UrlSafe");
  }

  if ("Text" in value) {
    return value.Text;
  }

  throw Error("Unhandled value type");
}

function Uuid(props: {
  base64UrlSafeBlob: string;
  blobEncoding: BlobEncoding;
}) {
  const render = () => {
    if (props.blobEncoding === "hex") {
      return toHex(urlSafeBase64Decode(props.base64UrlSafeBlob));
    }
    return props.base64UrlSafeBlob;
  };

  return (
    <Tooltip>
      <TooltipTrigger as="div">
        {props.blobEncoding === "mixed"
          ? urlSafeBase64ToUuid(props.base64UrlSafeBlob)
          : render()}
      </TooltipTrigger>

      <TooltipContent>
        <div>
          <ul>
            <li>
              UUID:{" "}
              <span class="font-bold">
                {urlSafeBase64ToUuid(props.base64UrlSafeBlob)}
              </span>
            </li>
            <li>
              Url-safe base64:{" "}
              <span class="font-bold">{props.base64UrlSafeBlob}</span>
            </li>
          </ul>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

function validateTableOrViewRecordApiRequirements(
  table: Table | View,
  allTables: Table[],
): string[] {
  switch (tableType(table)) {
    case "table":
      return validateTableRecordApiRequirements(table as Table, allTables);
    case "virtualTable":
      return ["Virtual tables are not supported"];
    case "view":
      return validateViewRecordApiRequirements(table as View, allTables);
  }
}

export function tableApiSummary(
  table: Table | View,
  allTables: Table[],
  config: Config | undefined,
): {
  supported: boolean;
  enabled: boolean;
  names: string[];
  errors: string[];
} {
  const errors = validateTableOrViewRecordApiRequirements(table, allTables);
  const names = getRecordApis(config, table.name)
    .map((api) => api.name)
    .filter((name): name is string => Boolean(name));

  return {
    supported: errors.length === 0,
    enabled: names.length > 0,
    names,
    errors,
  };
}

function TableHeaderRightHandButtons(props: {
  table: Table | View;
  allTables: Table[];
  schemaRefetch: () => Promise<void>;
  postgres: boolean;
  activeTab: WorkspaceTab;
}) {
  const selectedSchema = () => props.table;
  const hidden = () => hiddenTable(selectedSchema());
  const type = () => tableType(selectedSchema());
  const validateRecordApi = createMemo(() =>
    validateTableOrViewRecordApiRequirements(props.table, props.allTables),
  );
  const satisfiesRecordApi = () => validateRecordApi().length === 0;
  const hasRecordApi = () =>
    hasRecordApis(config?.data?.config, selectedSchema().name);

  const config = createConfigQuery();

  return (
    <div class="flex items-center justify-end gap-2">
      {/* Record API settings*/}
      {props.activeTab === "api" &&
        (type() === "table" || type() === "view") &&
        !hidden() && (
          <SafeSheet
            children={(sheet) => {
              return (
                <>
                  <SheetContent class={sheetMaxWidth}>
                    <RecordApiSettingsForm schema={props.table} {...sheet} />
                  </SheetContent>

                  <SheetTrigger
                    as={(props: DialogTriggerProps) => (
                      <Tooltip>
                        <TooltipTrigger as="div">
                          <Button
                            variant="outline"
                            size="sm"
                            class="flex items-center"
                            disabled={!satisfiesRecordApi()}
                            {...props}
                          >
                            <span class="hidden sm:inline">Configure </span>API
                            <Checkbox
                              disabled={!satisfiesRecordApi()}
                              checked={hasRecordApi()}
                            />
                          </Button>
                        </TooltipTrigger>

                        <TooltipContent>
                          <Switch>
                            <Match when={!satisfiesRecordApi()}>
                              <UnsatisfiedApiRequirementsTooltip
                                type={type()}
                                errors={validateRecordApi()}
                              />
                            </Match>

                            <Match when={true}>
                              <p>
                                Expose an API for this{" "}
                                {type().toLocaleUpperCase()}.
                              </p>
                            </Match>
                          </Switch>
                        </TooltipContent>
                      </Tooltip>
                    )}
                  />
                </>
              );
            }}
          />
        )}

      {/* Alter table schema */}
      {props.activeTab === "structure" &&
        type() === "table" &&
        !hidden() &&
        !props.postgres && (
          <SafeSheet
            children={(sheet) => {
              return (
                <>
                  <SheetContent class={sheetMaxWidth}>
                    <CreateAlterTableForm
                      schemaRefetch={props.schemaRefetch}
                      allTables={props.allTables}
                      setSelected={() => {
                        /* No selection change needed for AlterTable */
                      }}
                      schema={props.table as Table}
                      {...sheet}
                    />
                  </SheetContent>

                  <SheetTrigger
                    as={(props: DialogTriggerProps) => (
                      <Button variant="default" size="sm" {...props}>
                        <div class="flex items-center gap-2">
                          Alter <TbOutlineTable />
                        </div>
                      </Button>
                    )}
                  />
                </>
              );
            }}
          />
        )}
    </div>
  );
}

function TableHeader(props: {
  table: [Table, string] | [View, string];
  allTables: [Table, string][];
  schemaRefetch: () => Promise<void>;
  rowsRefetch: () => void;
  postgres: boolean;
  activeTab: WorkspaceTab;
}) {
  const allTables = createMemo(() => props.allTables.map(([t, _]) => t));
  const selectedSchema = () => props.table[0];
  const { state: explorerState } = useSidebar();
  const [sqlOpen, setSqlOpen] = createSignal(false);
  const [deleteOpen, setDeleteOpen] = createSignal(false);
  const [deleting, setDeleting] = createSignal(false);
  const config = createConfigQuery();

  const headerTitle = () => {
    switch (tableType(selectedSchema())) {
      case "view":
        return "View";
      case "virtualTable":
        return "Virtual Table";
      default:
        return "Table";
    }
  };
  const schemaName = () => selectedSchema().name.database_schema || "main";
  const resourceName = () => selectedSchema().name.name;
  const canDelete = () => !hiddenTable(selectedSchema()) && !props.postgres;

  const deleteTable = async () => {
    setDeleting(true);
    try {
      await dropTable({
        name: prettyFormatQualifiedName(selectedSchema().name),
        dry_run: null,
      });
      await config.refetch();
      await props.schemaRefetch();
      setDeleteOpen(false);
    } catch (err) {
      showToast({
        title: "Deletion Error",
        description: `${err}`,
        variant: "error",
      });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <header class="bg-background/95 sticky top-0 z-20 border-b backdrop-blur-sm">
      <div class="flex min-h-14 items-center justify-between gap-2 px-3 sm:px-4">
        <div class="flex min-w-0 items-center gap-2">
          <SidebarTrigger
            aria-label={
              explorerState() === "collapsed"
                ? "Show table explorer"
                : "Hide table explorer"
            }
          />
          <div class="text-muted-foreground flex min-w-0 items-center gap-1.5 text-sm">
            <span class="hidden sm:inline">Tables</span>
            <TbOutlineChevronRight class="hidden size-3.5 sm:block" />
            <span class="hidden sm:inline">{schemaName()}</span>
            <TbOutlineChevronRight class="hidden size-3.5 sm:block" />
            <span class="text-foreground truncate font-semibold">
              {resourceName()}
            </span>
          </div>
          <Badge variant="outline" class="hidden sm:inline-flex">
            {headerTitle()}
          </Badge>
        </div>

        <div class="flex shrink-0 items-center gap-2">
          <Show when={props.activeTab === "data"}>
            <Button
              variant="ghost"
              size="icon"
              aria-label="Refresh rows"
              title="Refresh rows"
              onClick={props.rowsRefetch}
            >
              <TbOutlineRefresh />
            </Button>
          </Show>

          <TableHeaderRightHandButtons
            table={selectedSchema()}
            allTables={allTables()}
            schemaRefetch={props.schemaRefetch}
            postgres={props.postgres}
            activeTab={props.activeTab}
          />

          <DropdownMenu placement="bottom-end">
            <DropdownMenuTrigger
              class={buttonVariants({ variant: "outline", size: "icon" })}
              aria-label="More table actions"
            >
              <TbOutlineDotsVertical />
            </DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuItem onSelect={() => setSqlOpen(true)}>
                <TbOutlineColumns />
                SQL schema
              </DropdownMenuItem>
              <Show when={canDelete()}>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  class="text-destructive ui-highlighted:text-destructive"
                  onSelect={() => setDeleteOpen(true)}
                >
                  <TbOutlineTrash />
                  Delete table
                </DropdownMenuItem>
              </Show>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      <Dialog open={sqlOpen()} onOpenChange={setSqlOpen}>
        <DialogContent class="max-w-[80dvw]">
          <DialogHeader>
            <DialogTitle>SQL Schema</DialogTitle>
          </DialogHeader>
          <pre class="bg-muted max-h-[70dvh] overflow-auto rounded-md p-4 font-mono text-sm whitespace-pre-wrap">
            {props.table[1]}
          </pre>
        </DialogContent>
      </Dialog>

      <Dialog open={deleteOpen()} onOpenChange={setDeleteOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete {resourceName()}?</DialogTitle>
          </DialogHeader>
          <p class="text-muted-foreground text-sm">
            Deleting this table will irreversibly delete all data it contains.
          </p>
          <DialogFooter class="gap-2">
            <Button variant="outline" onClick={() => setDeleteOpen(false)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={deleting()}
              onClick={deleteTable}
            >
              {deleting() ? "Deleting…" : "Delete table"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </header>
  );
}

type CellType =
  "UUID" | "JSON" | "File" | "File[]" | "Geometry" | ColumnDataType;

function deriveCellType(column: Column): CellType {
  if (isUUIDColumn(column)) {
    return "UUID";
  }
  if (isGeometryColumn(column)) {
    return "Geometry";
  }
  if (isFileUploadColumn(column)) {
    return "File";
  }
  if (isFileUploadsColumn(column)) {
    return "File[]";
  }

  if (isJSONColumn(column)) {
    return "JSON";
  }

  return column.data_type;
}

function buildColumnDefs(
  selectedSchema: Table | View,
  columns: Column[] | undefined,
  pkColumnIndex: number,
  blobEncoding: BlobEncoding,
  rowsRefetch: () => void,
): ColumnDef<ArrayRecord, SqlValue>[] {
  if (columns === undefined) {
    // Fallback to schema (rather than response) column defintions.
    if (tableType(selectedSchema) === "table") {
      return (selectedSchema as Table).columns.map((c) => ({
        id: c.name,
        header: c.name,
      }));
    }

    // We don't have any schema column defs. Fallback to single col. Avoid cell access.
    return [
      {
        id: "__missing__",
        cell: () => "missing",
      },
    ];
  }

  return columns.map((col, idx): ColumnDef<ArrayRecord, SqlValue> => {
    const fk = getForeignKey(col.options);
    const notNull = isNotNull(col.options);
    const type = deriveCellType(col);

    const typeName = notNull ? type : type + "?";
    const fkSuffix = fk ? ` ‣ ${fk.foreign_table}[${fk.referred_columns}]` : "";
    const header = `${col.name} [${typeName}] ${fkSuffix}`;

    return {
      id: col.name,
      header,
      enableSorting: true,
      sortingFn: "alphanumeric",
      cell: (context) =>
        renderCell(
          context,
          selectedSchema.name,
          columns,
          pkColumnIndex,
          {
            column: col,
            type,
          },
          blobEncoding,
          rowsRefetch,
        ),
      accessorFn: (row: ArrayRecord) => row[idx],
    };
  });
}

function RecordTable(props: {
  selectedSchema: Table | View;
  records: ListRowsResponse | undefined;
  pagination: SimpleSignal<PaginationState>;
  filter: SimpleSignal<string | undefined>;
  columnPinningState: Signal<ColumnPinningState>;
  sorting: Signal<SortingState>;
  rowsRefetch: () => void;
}) {
  const [blobEncoding, setBlobEncoding] = createSignal<BlobEncoding>("mixed");
  const [editRow, setEditRow] = createSignal<Record | undefined>();
  const [insertOpen, setInsertOpen] = createSignal(false);
  const [selectedRows, setSelectedRows] = createSignal(
    new Map<string, SqlValue>(),
  );
  const [deletingRows, setDeletingRows] = createSignal(false);

  const selectedSchema = () => props.selectedSchema;
  const mutable = () =>
    tableType(selectedSchema()) === "table" && !hiddenTable(selectedSchema());
  const rowsRefetch = () => props.rowsRefetch();

  const data = () => props.records?.rows;
  const columns = () => props.records?.columns;
  const totalRowCount = () => props.records?.total_row_count ?? 0;

  const pkColumnIndex = createMemo(
    () => findPrimaryKeyColumnIndex(columns() ?? []) ?? 0,
  );

  const table = createMemo(() => {
    const columnDefs = buildColumnDefs(
      selectedSchema(),
      columns(),
      pkColumnIndex(),
      blobEncoding(),
      props.rowsRefetch,
    );

    return buildTable(
      {
        // NOTE: The cell rendering is controlled via the columnsDefs.
        columns: columnDefs,
        data: data(),
        columnPinning: props.columnPinningState[0],
        onColumnPinningChange: props.columnPinningState[1],
        rowCount: Number(totalRowCount()),
        pagination: props.pagination[0](),
        onPaginationChange: (s: PaginationState) => {
          props.pagination[1](s);
        },
        onRowSelection: mutable()
          ? // eslint-disable-next-line solid/reactivity
            (rows: Row<ArrayRecord>[], value: boolean) => {
              const newSelection = new Map<string, SqlValue>(selectedRows());

              for (const row of rows) {
                const pkValue: SqlValue = row.original[pkColumnIndex()];
                const key = hashSqlValue(pkValue);

                if (value) {
                  newSelection.set(key, pkValue);
                } else {
                  newSelection.delete(key);
                }
              }
              setSelectedRows(newSelection);
            }
          : undefined,
      },
      {
        manualSorting: true,
        state: {
          sorting: props.sorting[0](),
        },
        onSortingChange: props.sorting[1],
      },
    );
  });

  const deleteSelectedRows = async () => {
    const ids = [...selectedRows().values()];
    if (ids.length === 0) return;

    setDeletingRows(true);
    try {
      await deleteRows(prettyFormatQualifiedName(selectedSchema().name), {
        primary_key_column: columns()?.[pkColumnIndex()].name ?? "??",
        values: ids,
      });
      setSelectedRows(new Map<string, SqlValue>());
    } catch (err) {
      showToast({
        title: "Deletion Error",
        description: `${err}`,
        variant: "error",
      });
    } finally {
      setDeletingRows(false);
      rowsRefetch();
    }
  };

  return (
    <div id="data">
      <SafeSheet
        open={[
          () => editRow() !== undefined,
          (isOpen: boolean | ((value: boolean) => boolean)) => {
            if (!isOpen) {
              setEditRow(undefined);
            }
          },
        ]}
        children={(sheet) => {
          return (
            <>
              <SheetContent class={sheetMaxWidth}>
                <InsertUpdateRowForm
                  schema={selectedSchema() as Table}
                  rowsRefetch={rowsRefetch}
                  row={editRow()}
                  {...sheet}
                />
              </SheetContent>

              <div class="flex flex-col gap-3">
                <div class="flex flex-col gap-2 lg:flex-row lg:items-start">
                  <FilterBar
                    initial={props.filter[0]()}
                    onSubmit={(value: string) => {
                      const next = value || undefined;
                      if (next === props.filter[0]()) {
                        rowsRefetch();
                      } else {
                        props.filter[1](next);
                      }
                    }}
                    placeholder="Filter rows with an expression…"
                    example={
                      <span>
                        Press{" "}
                        <kbd class="rounded-sm border px-1 font-mono">/</kbd> to
                        focus. Example: <code>status = "active"</code>
                      </span>
                    }
                  />

                  <div class="flex shrink-0 flex-wrap items-center gap-2">
                    <span class="text-muted-foreground text-sm whitespace-nowrap">
                      {props.records === undefined
                        ? "Loading rows…"
                        : `${totalRowCount()} rows`}
                    </span>

                    <Select
                      multiple={false}
                      options={[...blobEncodings]}
                      value={blobEncoding()}
                      itemComponent={(props) => (
                        <SelectItem item={props.item}>
                          {props.item.rawValue}
                        </SelectItem>
                      )}
                      onChange={(encoding: BlobEncoding | null) => {
                        if (encoding !== null) setBlobEncoding(encoding);
                      }}
                    >
                      <SelectTrigger class="w-32" aria-label="Blob format">
                        <SelectValue<string>>
                          {(state) => `Blobs: ${state.selectedOption()}`}
                        </SelectValue>
                      </SelectTrigger>
                      <SelectContent />
                    </Select>

                    <Show when={import.meta.env.DEV}>
                      <DebugDialogButton title="Schema" data={data() ?? []} />
                    </Show>

                    <Show when={mutable()}>
                      <SafeSheet open={[insertOpen, setInsertOpen]}>
                        {(sheet) => (
                          <>
                            <SheetContent class={sheetMaxWidth}>
                              <InsertUpdateRowForm
                                schema={selectedSchema() as Table}
                                rowsRefetch={rowsRefetch}
                                {...sheet}
                              />
                            </SheetContent>
                            <SheetTrigger
                              as={(triggerProps: DialogTriggerProps) => (
                                <Button {...triggerProps}>Insert row</Button>
                              )}
                            />
                          </>
                        )}
                      </SafeSheet>
                    </Show>
                  </div>
                </div>

                <div class="overflow-x-auto">
                  <TableComponent
                    table={table()}
                    loading={props.records === undefined}
                    dense
                    paginationPosition="bottom"
                    emptyState={
                      <div class="flex flex-col items-center gap-2 py-8 text-center">
                        <p class="font-medium">
                          {props.filter[0]()
                            ? "No rows match this filter"
                            : "No rows yet"}
                        </p>
                        <p class="text-muted-foreground text-sm">
                          {props.filter[0]()
                            ? "Try a different expression or clear the filter."
                            : "Insert the first record to get started."}
                        </p>
                        <Show
                          when={props.filter[0]()}
                          fallback={
                            <Show when={mutable()}>
                              <Button onClick={() => setInsertOpen(true)}>
                                Insert first row
                              </Button>
                            </Show>
                          }
                        >
                          <Button
                            variant="outline"
                            onClick={() => props.filter[1](undefined)}
                          >
                            Clear filter
                          </Button>
                        </Show>
                      </div>
                    }
                    onRowClick={
                      mutable()
                        ? (_idx: number, row: ArrayRecord) => {
                            setEditRow(rowDataToRow(columns() ?? [], row));
                          }
                        : undefined
                    }
                  />
                </div>
              </div>
            </>
          );
        }}
      />

      <Show when={selectedRows().size > 0}>
        <div class="border-primary/30 bg-primary/5 mt-3 flex flex-wrap items-center justify-between gap-3 rounded-md border px-3 py-2">
          <span class="text-sm font-medium">
            {selectedRows().size} {selectedRows().size === 1 ? "row" : "rows"}{" "}
            selected
          </span>
          <div class="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setSelectedRows(new Map<string, SqlValue>())}
            >
              Clear selection
            </Button>
            <Button
              variant="destructive"
              size="sm"
              disabled={deletingRows()}
              onClick={deleteSelectedRows}
            >
              {deletingRows() ? "Deleting…" : "Delete selected"}
            </Button>
          </div>
        </div>
      </Show>
    </div>
  );
}

export function tableStructureCounts(
  table: Table,
  schemas: ListSchemasResponse,
): { columns: number; indexes: number; triggers: number } {
  const matchesTable = (tableName: string, databaseSchema: string | null) =>
    equalQualifiedNames(table.name, {
      name: tableName,
      database_schema: databaseSchema,
    });

  return {
    columns: table.columns.length,
    indexes: schemas.indexes.filter(([index]) =>
      matchesTable(index.table_name, index.name.database_schema),
    ).length,
    triggers: schemas.triggers.filter(([trigger]) =>
      matchesTable(trigger.table_name, trigger.name.database_schema),
    ).length,
  };
}

function StructureTab(props: {
  table: Table;
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const counts = () => tableStructureCounts(props.table, props.schemas);
  const primaryKey = () =>
    props.table.columns.find(isPrimaryKeyColumn)?.name ?? "None";

  return (
    <div class="flex flex-col gap-6">
      <section aria-labelledby="structure-overview">
        <h2 id="structure-overview" class="text-base font-semibold">
          Overview
        </h2>
        <div class="mt-3 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <div class="bg-card rounded-md border p-3">
            <p class="text-muted-foreground text-xs font-medium tracking-wide uppercase">
              Resource
            </p>
            <p class="mt-1 truncate font-mono text-sm">
              {prettyFormatQualifiedName(props.table.name)}
            </p>
          </div>
          <div class="bg-card rounded-md border p-3">
            <p class="text-muted-foreground text-xs font-medium tracking-wide uppercase">
              Primary key
            </p>
            <p class="mt-1 truncate font-mono text-sm">{primaryKey()}</p>
          </div>
          <div class="bg-card rounded-md border p-3">
            <p class="text-muted-foreground text-xs font-medium tracking-wide uppercase">
              Columns
            </p>
            <p class="mt-1 text-lg font-semibold">{counts().columns}</p>
          </div>
          <div class="bg-card rounded-md border p-3">
            <p class="text-muted-foreground text-xs font-medium tracking-wide uppercase">
              Schema objects
            </p>
            <p class="mt-1 text-sm">
              {counts().indexes} indexes · {counts().triggers} triggers
            </p>
          </div>
        </div>
      </section>

      <section aria-labelledby="columns-heading">
        <div class="mb-3 flex items-center justify-between">
          <h2 id="columns-heading" class="text-base font-semibold">
            Columns
          </h2>
          <span class="text-muted-foreground text-sm">
            {counts().columns} total
          </span>
        </div>
        <div class="overflow-x-auto rounded-md border">
          <div class="bg-muted/40 text-muted-foreground grid min-w-[720px] grid-cols-[minmax(180px,1fr)_160px_minmax(260px,1.4fr)_minmax(180px,1fr)] border-b px-3 py-2 text-xs font-medium tracking-wide uppercase">
            <span>Name</span>
            <span>Type</span>
            <span>Constraints</span>
            <span>Default</span>
          </div>
          <For each={props.table.columns}>
            {(column) => {
              const unique = () => getUnique(column.options);
              const foreignKey = () => getForeignKey(column.options);
              const defaultValue = () => getDefaultValue(column.options);
              return (
                <div class="grid min-w-[720px] grid-cols-[minmax(180px,1fr)_160px_minmax(260px,1.4fr)_minmax(180px,1fr)] items-center border-b px-3 py-2.5 text-sm last:border-b-0">
                  <code class="text-foreground truncate font-medium">
                    {column.name}
                  </code>
                  <span class="text-muted-foreground truncate font-mono">
                    {column.type_name || column.data_type}
                  </span>
                  <div class="flex flex-wrap gap-1.5">
                    <Show when={unique()?.is_primary}>
                      <Badge variant="secondary">Primary key</Badge>
                    </Show>
                    <Show when={unique() && !unique()?.is_primary}>
                      <Badge variant="outline">Unique</Badge>
                    </Show>
                    <Show when={isNotNull(column.options)}>
                      <Badge variant="outline">Not null</Badge>
                    </Show>
                    <Show when={foreignKey()}>
                      {(foreignKey) => (
                        <Badge variant="outline">
                          <A
                            class="hover:underline"
                            href={`/table/${encodeURIComponent(foreignKey().foreign_table)}`}
                          >
                            → {foreignKey().foreign_table}
                          </A>
                        </Badge>
                      )}
                    </Show>
                    <Show
                      when={
                        !unique() && !isNotNull(column.options) && !foreignKey()
                      }
                    >
                      <span class="text-muted-foreground">—</span>
                    </Show>
                  </div>
                  <code class="text-muted-foreground truncate text-xs">
                    {defaultValue() ?? "—"}
                  </code>
                </div>
              );
            }}
          </For>
        </div>
      </section>

      <IndexTable
        table={props.table}
        schemas={props.schemas}
        schemaRefetch={props.schemaRefetch}
      />
      <TriggerTable table={props.table} schemas={props.schemas} />
    </div>
  );
}

function IndexTable(props: {
  table: Table;
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const hidden = () => hiddenTable(props.table);
  const [editIndex, setEditIndex] = createSignal<TableIndex | undefined>();
  const [selectedIndexes, setSelectedIndexes] = createSignal(new Set<string>());

  const indexes = createMemo(() => {
    return props.schemas.indexes.filter(([index, _]) =>
      equalQualifiedNames(props.table.name, {
        name: index.table_name,
        database_schema: index.name.database_schema,
      }),
    );
  });

  const indexesTable = createMemo(() => {
    return buildTable({
      columns: indexColumns,
      data: indexes().map(([index, _]) => index),
      onRowSelection: hidden()
        ? undefined
        : // eslint-disable-next-line solid/reactivity
          (rows: Row<TableIndex>[], value: boolean) => {
            const newSelection = new Set(selectedIndexes());

            for (const row of rows) {
              const qualifiedName = prettyFormatQualifiedName(
                row.original.name,
              );
              if (value) {
                newSelection.add(qualifiedName);
              } else {
                newSelection.delete(qualifiedName);
              }
            }
            setSelectedIndexes(newSelection);
          },
    });
  });

  return (
    <section id="indexes" class="bg-card rounded-md border p-4">
      <h2 class="mb-3 flex items-center gap-2 text-base font-semibold">
        Indexes
        <Show when={import.meta.env.DEV}>
          <DebugDialogButton title="Indexes" data={indexes()} />
        </Show>
      </h2>

      <SafeSheet
        open={[
          () => editIndex() !== undefined,
          (isOpen: boolean | ((value: boolean) => boolean)) => {
            if (!isOpen) {
              setEditIndex(undefined);
            }
          },
        ]}
      >
        {(sheet) => {
          return (
            <>
              <SheetContent class={sheetMaxWidth}>
                <CreateAlterIndexForm
                  schema={editIndex()}
                  table={props.table}
                  schemaRefetch={props.schemaRefetch}
                  {...sheet}
                />
              </SheetContent>

              <div class="space-y-2.5 overflow-x-auto">
                <TableComponent
                  table={indexesTable()}
                  loading={false}
                  emptyState={
                    <span class="text-muted-foreground">
                      No indexes configured
                    </span>
                  }
                  onRowClick={
                    hidden()
                      ? undefined
                      : (_idx: number, index: TableIndex) => {
                          setEditIndex(index);
                        }
                  }
                />
              </div>
            </>
          );
        }}
      </SafeSheet>

      <Show when={!hidden()}>
        <div class="mt-2 flex gap-2">
          <SafeSheet>
            {(sheet) => {
              return (
                <>
                  <SheetContent class={sheetMaxWidth}>
                    <CreateAlterIndexForm
                      schemaRefetch={props.schemaRefetch}
                      table={props.table}
                      {...sheet}
                    />
                  </SheetContent>

                  <SheetTrigger
                    as={(props: DialogTriggerProps) => (
                      <Button variant="default" size="sm" {...props}>
                        Add index
                      </Button>
                    )}
                  />
                </>
              );
            }}
          </SafeSheet>

          <Button
            variant="destructive"
            size="sm"
            disabled={selectedIndexes().size == 0}
            onClick={() => {
              const names = Array.from(selectedIndexes());
              if (names.length == 0) {
                return;
              }

              (async () => {
                try {
                  for (const name of names) {
                    await dropIndex({ name, dry_run: null });
                  }

                  setSelectedIndexes(new Set<string>());
                } catch (err) {
                  showToast({
                    title: "Deletion Error",
                    description: `${err}`,
                    variant: "error",
                  });
                } finally {
                  props.schemaRefetch();
                }
              })();
            }}
          >
            Delete indexes
          </Button>
        </div>
      </Show>
    </section>
  );
}

function TriggerTable(props: { table: Table; schemas: ListSchemasResponse }) {
  const triggers = createMemo(() => {
    return props.schemas.triggers.filter(([trig, _]) =>
      equalQualifiedNames(props.table.name, {
        name: trig.table_name,
        database_schema: trig.name.database_schema,
      }),
    );
  });

  const triggersTable = createMemo(() => {
    return buildTable({
      columns: triggerColumns,
      data: triggers().map(([trig, sql]) => ({
        ...trig,
        sql,
      })),
    });
  });

  return (
    <section id="triggers" class="bg-card rounded-md border p-4">
      <h2 class="flex items-center gap-2 text-base font-semibold">
        Triggers
        <Show when={import.meta.env.DEV}>
          <DebugDialogButton title="Triggers" data={triggers()} />
        </Show>
      </h2>

      <div class="border-border bg-muted/30 mt-3 flex flex-col gap-2 rounded-md border p-3 text-sm sm:flex-row sm:items-center sm:justify-between">
        <p class="text-muted-foreground">
          Trigger changes are currently managed through SQL.
        </p>
        <Button as={A} href="/editor" variant="outline" size="sm">
          Open SQL Editor
        </Button>
      </div>

      <div class="mt-4 overflow-x-auto">
        <TableComponent
          loading={false}
          table={triggersTable()}
          emptyState={
            <span class="text-muted-foreground">No triggers configured</span>
          }
        />
      </div>
    </section>
  );
}

function ApiTab(props: { table: Table | View; allTables: Table[] }) {
  const config = createConfigQuery();
  const summary = createMemo(() =>
    tableApiSummary(props.table, props.allTables, config.data?.config),
  );

  const copyEndpoint = async (endpoint: string) => {
    try {
      await navigator.clipboard.writeText(endpoint);
      showToast({
        title: "Copied",
        description: endpoint,
        variant: "success",
      });
    } catch (err) {
      showToast({
        title: "Copy failed",
        description: `${err}`,
        variant: "error",
      });
    }
  };

  return (
    <div class="flex max-w-5xl flex-col gap-4">
      <section class="bg-card rounded-md border p-4">
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div class="flex items-center gap-2">
              <h2 class="text-base font-semibold">Record API</h2>
              <Badge variant={summary().enabled ? "success" : "outline"}>
                {summary().enabled ? "Enabled" : "Disabled"}
              </Badge>
            </div>
            <p class="text-muted-foreground mt-1 text-sm">
              Expose typed CRUD endpoints for this database resource.
            </p>
          </div>
          <span class="text-muted-foreground font-mono text-xs">
            {prettyFormatQualifiedName(props.table.name)}
          </span>
        </div>
      </section>

      <Show
        when={summary().supported}
        fallback={
          <Callout variant="warning">
            <CalloutTitle>Record API unavailable</CalloutTitle>
            <CalloutContent>
              <ul class="list-inside list-disc text-sm">
                <For each={summary().errors}>{(error) => <li>{error}</li>}</For>
              </ul>
            </CalloutContent>
          </Callout>
        }
      >
        <Show
          when={summary().names.length > 0}
          fallback={
            <div class="rounded-md border border-dashed p-8 text-center">
              <p class="font-medium">No Record API configured</p>
              <p class="text-muted-foreground mt-1 text-sm">
                Use Configure API above to choose permissions and access rules.
              </p>
            </div>
          }
        >
          <section aria-labelledby="api-endpoints" class="rounded-md border">
            <div class="border-b px-4 py-3">
              <h3 id="api-endpoints" class="font-medium">
                Configured endpoints
              </h3>
              <p class="text-muted-foreground mt-1 text-sm">
                Base paths for APIs backed by this resource.
              </p>
            </div>
            <div class="divide-y">
              <For each={summary().names}>
                {(name) => {
                  const endpoint = `/api/records/v1/${name}`;
                  return (
                    <div class="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
                      <div class="min-w-0">
                        <div class="flex flex-wrap items-center gap-1.5">
                          <Badge variant="outline">GET</Badge>
                          <Badge variant="outline">POST</Badge>
                          <Badge variant="outline">PATCH</Badge>
                          <Badge variant="outline">DELETE</Badge>
                        </div>
                        <code class="mt-2 block truncate text-sm">
                          {endpoint}
                        </code>
                      </div>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => copyEndpoint(endpoint)}
                      >
                        <TbOutlineCopy />
                        Copy path
                      </Button>
                    </div>
                  );
                }}
              </For>
            </div>
          </section>
        </Show>
      </Show>
    </div>
  );
}

export type WorkspaceTab = "data" | "structure" | "api";

export function normalizeWorkspaceTab(value: string | undefined): WorkspaceTab {
  return value === "structure" || value === "api" ? value : "data";
}

type SearchParams = {
  filter?: string;
  pageSize?: string;
  pageIndex?: string;
  tab?: string;
};

export function workspaceTabSearchParams<T extends object>(
  searchParams: T,
  tab: WorkspaceTab,
): T & { tab: WorkspaceTab | undefined } {
  return {
    ...searchParams,
    tab: tab === "data" ? undefined : tab,
  };
}

export function TablePane(props: {
  selectedTable: [Table, string] | [View, string];
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
  postgres: boolean;
}) {
  const selectedSchema = () => props.selectedTable[0];
  const isTable = () => tableType(selectedSchema()) === "table";

  // IMPORTANT: We need to memo the downstream search params use to treat absence and defaults
  // consistently, otherwise `undefined`->`default` may invalidate the cursors.
  const [searchParams, setSearchParams] = useSearchParams<SearchParams>();
  const filter = createMemo(() => searchParams.filter);
  const pageSize = createMemo(() => safeParseInt(searchParams.pageSize) ?? 20);
  const pageIndex = createMemo(() => safeParseInt(searchParams.pageIndex) ?? 0);

  const pagination = (): PaginationState => ({
    pageIndex: pageIndex(),
    pageSize: pageSize(),
  });
  const setPagination = (s: PaginationState) => {
    setSearchParams({
      ...searchParams,
      pageIndex: s.pageIndex,
      pageSize: s.pageSize,
    });
  };
  const setFilter = (filter: string | undefined) => {
    // Reset pagination.
    setSearchParams({
      ...searchParams,
      pageIndex: undefined,
      pageSize: undefined,
      filter,
    });
  };

  const [sorting, setSortingImpl] = createWritableMemo<SortingState>(() => {
    // TODO: Represent sorting state in `searchParams`, e.g. `?order=+col1,-col2`.
    // Meanwhile this needs it's own reset.
    const _ = [selectedSchema()];

    return [];
  });
  const setSorting = (s: Updater<SortingState>) => {
    // Reset pagination.
    setSearchParams({
      ...searchParams,
      pageIndex: undefined,
      pageSize: undefined,
    });
    setSortingImpl(s);
  };

  const cursors = createMemo<Map<number, string>>(() => {
    // Reset cursor whenever table or search params change. This is basically
    // the same as `queryKey` below minus `pageIndex`.
    const _ = [selectedSchema(), pageSize(), filter(), sorting()];
    console.debug("resetting cursor");
    return new Map();
  });

  const records: QueryObserverResult<ListRowsResponse> = useQuery(() => ({
    queryKey: [
      selectedSchema(),
      pagination(),
      filter(),
      sorting(),
    ] as ReadonlyArray<unknown>,
    queryFn: async ({ queryKey }) => {
      console.debug("Fetching data with key:", queryKey);

      try {
        const { pageSize, pageIndex } = pagination();
        const cursor = cursors().get(pageIndex - 1);

        const response = await fetchRows(
          selectedSchema().name,
          filter() ?? null,
          pageSize,
          pageIndex,
          cursor ?? null,
          formatSortingAsOrder(sorting()),
        );

        // Update cursors.
        if (sorting().length === 0 && response.cursor) {
          cursors().set(pageIndex, response.cursor);
        }

        return response;
      } catch (err) {
        // Reset.
        setSearchParams({
          ...searchParams,
          filter: undefined,
          pageSize: undefined,
          pageIndex: undefined,
        });

        throw err;
      }
    },
  }));

  const rowsRefetch = records.refetch;
  const schemaRefetch = async () => {
    // First re-fetch the schema then the data rows to trigger a re-render.
    await props.schemaRefetch();
    rowsRefetch();
  };

  const [columnPinningState, setColumnPinningState] = createSignal({});
  const activeTab = () => normalizeWorkspaceTab(searchParams.tab);

  return (
    <div class="flex min-h-0 flex-1 flex-col">
      <TableHeader
        table={props.selectedTable}
        allTables={props.schemas.tables}
        schemaRefetch={schemaRefetch}
        rowsRefetch={rowsRefetch}
        postgres={props.postgres}
        activeTab={activeTab()}
      />

      <Tabs
        class="flex min-h-0 flex-1 flex-col"
        value={activeTab()}
        onChange={(tab) =>
          setSearchParams(
            workspaceTabSearchParams(searchParams, normalizeWorkspaceTab(tab)),
          )
        }
      >
        <div class="overflow-x-auto border-b px-4">
          <TabsList class="h-10 rounded-none bg-transparent p-0">
            <TabsTrigger
              value="data"
              class="ui-selected:border-primary ui-selected:bg-transparent ui-selected:shadow-none h-10 rounded-none border-b-2 border-transparent px-4"
            >
              Data
            </TabsTrigger>
            <TabsTrigger
              value="structure"
              class="ui-selected:border-primary ui-selected:bg-transparent ui-selected:shadow-none h-10 rounded-none border-b-2 border-transparent px-4"
            >
              Structure
            </TabsTrigger>
            <TabsTrigger
              value="api"
              class="ui-selected:border-primary ui-selected:bg-transparent ui-selected:shadow-none h-10 rounded-none border-b-2 border-transparent px-4"
            >
              API
            </TabsTrigger>
          </TabsList>
        </div>

        <TabsContent value="data" class="m-0 min-h-0 flex-1 p-4">
          <Switch>
            <Match when={records.isError}>
              <div class="border-destructive/30 bg-destructive/5 rounded-md border p-4">
                <p class="font-medium">Failed to fetch rows</p>
                <p class="text-muted-foreground mt-1 text-sm">
                  {`${records.error}`}
                </p>
                <Button class="mt-3" onClick={() => records.refetch()}>
                  Retry
                </Button>
              </div>
            </Match>

            <Match when={true}>
              <RecordTable
                selectedSchema={selectedSchema()}
                records={records.isSuccess ? records.data : undefined}
                pagination={[pagination, setPagination]}
                filter={[filter, setFilter]}
                columnPinningState={[columnPinningState, setColumnPinningState]}
                sorting={[sorting, setSorting]}
                rowsRefetch={rowsRefetch}
              />
            </Match>
          </Switch>
        </TabsContent>

        <TabsContent value="structure" class="m-0 flex flex-col gap-8 p-4">
          <Show
            when={isTable()}
            fallback={
              <div class="text-muted-foreground rounded-md border p-4 text-sm">
                Structure editing is unavailable for this resource type.
              </div>
            }
          >
            <StructureTab
              table={selectedSchema() as Table}
              schemas={props.schemas}
              schemaRefetch={props.schemaRefetch}
            />
          </Show>
        </TabsContent>

        <TabsContent value="api" class="m-0 p-4">
          <ApiTab
            table={selectedSchema()}
            allTables={props.schemas.tables.map(([table]) => table)}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function UnsatisfiedApiRequirementsTooltip(props: {
  type: TableType;
  errors: string[];
}) {
  return (
    <div class="flex flex-col">
      <p>
        This ${props.type.toLocaleUpperCase()} does not satisfy Record API
        requirements, due to:
      </p>

      <div class="px-4 py-2 break-all">
        <ul class="list-disc">
          <For each={props.errors}>{(err) => <li>{err}</li>}</For>
        </ul>
      </div>
    </div>
  );
}

const sheetMaxWidth = "sm:max-w-[520px]";

const indexColumns = [
  {
    header: "name",
    accessorFn: (index: TableIndex) => index.name.name,
  },
  {
    header: "columns",
    accessorFn: (index: TableIndex) => {
      return index.columns.map((c) => c.column_name).join(", ");
    },
  },
  {
    header: "unique",
    accessorKey: "unique",
  },
  {
    header: "predicate",
    accessorFn: (index: TableIndex) => {
      return index.predicate?.replaceAll("<>", "!=");
    },
  },
] as ColumnDef<TableIndex>[];

type TableTriggerAndSql = TableTrigger & {
  sql: string;
};

const triggerColumnHelper = createColumnHelper<TableTriggerAndSql>();
const triggerColumns = [
  triggerColumnHelper.accessor("name", {
    header: "name",
    cell: (props) => <p class="max-w-[20dvw]">{props.getValue().name}</p>,
  }),
  triggerColumnHelper.accessor("sql", {
    header: "statement",
    cell: (props) => <p class="max-w-[20dvw]">{props.getValue()}</p>,
  }),
] as ColumnDef<TableTriggerAndSql>[];
