import { Match, Show, Switch, createMemo, createSignal, JSX } from "solid-js";
import type { Signal } from "solid-js";
import { createWritableMemo } from "@solid-primitives/memo";
import { TbRefresh, TbTable, TbTrash, TbColumns } from "solid-icons/tb";
import { useSearchParams } from "@solidjs/router";
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

import { Header } from "@/components/Header";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { SheetContent, SheetTrigger } from "@/components/ui/sheet";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { SidebarTrigger } from "@/components/ui/sidebar";
import { showToast } from "@/components/ui/toast";

import { DebugDialogButton } from "@/components/tables/SchemaDownload";
import { CreateAlterTableForm } from "@/components/tables/CreateAlterTable";
import { CreateAlterIndexForm } from "@/components/tables/CreateAlterIndex";
import { Table as TableComponent, buildTable } from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";
import { DestructiveActionButton } from "@/components/DestructiveActionButton";
import { IconButton } from "@/components/IconButton";
import { InsertUpdateRowForm } from "@/components/tables/InsertUpdateRow";
import {
  RecordApiSettingsForm,
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
import type { Record, ArrayRecord } from "@/lib/record";
import { hashSqlValue } from "@/lib/value";
import { urlSafeBase64ToUuid, toHex, safeParseInt } from "@/lib/utils";
import { equalQualifiedNames } from "@/lib/schema";
import { dropTable, dropIndex } from "@/lib/api/table";
import { deleteRows, fetchRows } from "@/lib/api/row";
import { formatSortingAsOrder } from "@/lib/list";
import {
  findPrimaryKeyColumnIndex,
  getForeignKey,
  isFileUploadColumn,
  isFileUploadsColumn,
  isJSONColumn,
  isNotNull,
  isUUIDColumn,
  hiddenTable,
  tableType,
  tableSatisfiesRecordApiRequirements,
  viewSatisfiesRecordApiRequirements,
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

export type SimpleSignal<T> = [get: () => T, set: (state: T) => void];

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
): JSX.Element {
  const value: SqlValue = context.getValue();
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
      if (cell.type === "UUID") {
        return (
          <Uuid
            base64UrlSafeBlob={blob.Base64UrlSafe}
            blobEncoding={blobEncoding}
          />
        );
      }

      if (blobEncoding === "hex") {
        return toHex(urlSafeBase64Decode(blob.Base64UrlSafe));
      }
      return blob.Base64UrlSafe;
    }
    throw Error("Expected Base64UrlSafe");
  }

  if ("Text" in value) {
    if (cell.type === "File") {
      const file = JSON.parse(value.Text) as FileUpload;
      const pkCol = columns[pkIndex].name;
      const pkVal = context.row.original[pkIndex];

      return (
        <UploadedFile
          file={file}
          tableName={tableName}
          columnName={cell.column.name}
          pk={{ columnName: pkCol, value: pkVal }}
        />
      );
    } else if (cell.type === "File[]") {
      const files = JSON.parse(value.Text) as FileUploads;
      const pkCol = columns[pkIndex].name;
      const pkVal = context.row.original[pkIndex];

      return (
        <UploadedFiles
          files={files}
          tableName={tableName}
          columnName={cell.column.name}
          pk={{ columnName: pkCol, value: pkVal }}
        />
      );
    }

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

function tableOrViewSatisfiesRecordApiRequirements(
  table: Table | View,
  allTables: Table[],
): boolean {
  const type = tableType(table);

  if (type === "table") {
    return tableSatisfiesRecordApiRequirements(table as Table, allTables);
  } else if (type === "view") {
    return viewSatisfiesRecordApiRequirements(table as View, allTables);
  }

  return false;
}

function TableHeaderRightHandButtons(props: {
  table: Table | View;
  allTables: Table[];
  schemaRefetch: () => Promise<void>;
}) {
  const selectedSchema = () => props.table;
  const hidden = () => hiddenTable(selectedSchema());
  const type = () => tableType(selectedSchema());
  const satisfiesRecordApi = createMemo(() =>
    tableOrViewSatisfiesRecordApiRequirements(props.table, props.allTables),
  );
  const hasRecordApi = () =>
    hasRecordApis(config?.data?.config, selectedSchema().name);

  const config = createConfigQuery();

  return (
    <div class="flex items-center justify-end gap-2">
      {/* Delete table button */}
      {!hidden() && (
        <DestructiveActionButton
          size="sm"
          action={() => {
            return (async () => {
              try {
                await dropTable({
                  name: prettyFormatQualifiedName(selectedSchema().name),
                  dry_run: null,
                });
              } finally {
                await config.refetch();
                await props.schemaRefetch();
              }
            })();
          }}
          msg="Deleting a table will irreversibly delete all the data contained. Are you sure you'd like to continue?"
        >
          <div class="flex items-center gap-2">
            Delete <TbTrash />
          </div>
        </DestructiveActionButton>
      )}

      {/* Record API settings*/}
      {(type() === "table" || type() === "view") && !hidden() && (
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
                          API
                          <Checkbox
                            disabled={!satisfiesRecordApi()}
                            checked={hasRecordApi()}
                          />
                        </Button>
                      </TooltipTrigger>

                      <TooltipContent>
                        {satisfiesRecordApi() ? (
                          <p>Create a Record API endpoint for this table.</p>
                        ) : (
                          <p>
                            This table does not satisfy the requirements for
                            exposing a Record API: strictly typed {"&"} integer
                            or UUID primary key column.
                          </p>
                        )}
                      </TooltipContent>
                    </Tooltip>
                  )}
                />
              </>
            );
          }}
        />
      )}

      {type() === "table" && !hidden() && (
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
                        Alter <TbTable />
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
}) {
  const allTables = createMemo(() => props.allTables.map(([t, _]) => t));
  const selectedSchema = () => props.table[0];

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

  return (
    <Header
      leading={<SidebarTrigger />}
      title={headerTitle()}
      titleSelect={prettyFormatQualifiedName(selectedSchema().name)}
      left={
        <div class="flex items-center">
          <IconButton tooltip="Refresh Data" onClick={props.rowsRefetch}>
            <TbRefresh />
          </IconButton>

          <Dialog id="sql-schema">
            <DialogTrigger>
              <IconButton tooltip="SQL Schema">
                <TbColumns />
              </IconButton>
            </DialogTrigger>

            <DialogContent class="max-w-[80dvw]">
              <DialogHeader>
                <DialogTitle>SQL Schema</DialogTitle>
              </DialogHeader>

              <span class="font-mono text-sm whitespace-pre-wrap">
                {props.table[1]}
              </span>
            </DialogContent>
          </Dialog>
        </div>
      }
      right={
        <TableHeaderRightHandButtons
          table={selectedSchema()}
          allTables={allTables()}
          schemaRefetch={props.schemaRefetch}
        />
      }
    />
  );
}

type CellType = "UUID" | "JSON" | "File" | "File[]" | ColumnDataType;

function deriveCellType(column: Column): CellType {
  if (isUUIDColumn(column)) {
    return "UUID";
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
): ColumnDef<ArrayRecord, SqlValue>[] {
  if (columns === undefined) {
    // Fallback to schema (rather than response) column defintions.
    if (tableType(selectedSchema) === "table") {
      return (selectedSchema as Table).columns.map((c) => ({
        id: c.name,
        header: c.name,
      }));
    }

    // We don't have any schema column defs. Fallback to single col.
    return [
      {
        header: "",
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
  const [selectedRows, setSelectedRows] = createSignal(
    new Map<string, SqlValue>(),
  );

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
    );

    return buildTable(
      {
        // NOTE: The cell rendering is constrolled via the columnsDefs.
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

              <FilterBar
                initial={props.filter[0]()}
                onSubmit={(value: string) => {
                  if (value === props.filter[0]()) {
                    rowsRefetch();
                  } else {
                    props.filter[1](value);
                  }
                }}
                placeholder={`Filter Query, e.g. '(col0 > 5 || col0 = 0) || col1 ~ "%like"'`}
              />

              <div class="overflow-x-auto pt-4">
                <TableComponent
                  table={table()}
                  loading={props.records === undefined}
                  onRowClick={
                    mutable()
                      ? (_idx: number, row: ArrayRecord) => {
                          setEditRow(rowDataToRow(columns() ?? [], row));
                        }
                      : undefined
                  }
                />
              </div>
            </>
          );
        }}
      />

      <div class="my-2 flex flex-wrap justify-between gap-2">
        {mutable() && (
          <div class="flex gap-2">
            {/* Insert Rows */}
            <SafeSheet
              children={(sheet) => {
                return (
                  <>
                    <SheetContent class={sheetMaxWidth}>
                      <InsertUpdateRowForm
                        schema={selectedSchema() as Table}
                        rowsRefetch={rowsRefetch}
                        {...sheet}
                      />
                    </SheetContent>

                    <SheetTrigger
                      as={(props: DialogTriggerProps) => (
                        <Button variant="default" {...props}>
                          Insert Row
                        </Button>
                      )}
                    />
                  </>
                );
              }}
            />

            {/* Delete rows */}
            <Button
              variant="destructive"
              disabled={selectedRows().size === 0}
              onClick={() => {
                const ids = [...selectedRows().values()];
                if (ids.length === 0) {
                  return;
                }

                (async () => {
                  try {
                    await deleteRows(
                      prettyFormatQualifiedName(selectedSchema().name),
                      {
                        primary_key_column:
                          columns()?.[pkColumnIndex()].name ?? "??",
                        values: ids,
                      },
                    );

                    setSelectedRows(new Map<string, SqlValue>());
                  } catch (err) {
                    showToast({
                      title: "Deletion Error",
                      description: `${err}`,
                      variant: "error",
                    });
                  } finally {
                    rowsRefetch();
                  }
                })();
              }}
            >
              Delete rows
            </Button>
          </div>
        )}

        <div class="flex items-center gap-2">
          <Label>Blobs:</Label>

          <Select
            multiple={false}
            options={[...blobEncodings]}
            value={blobEncoding()}
            itemComponent={(props) => (
              <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
            )}
            onChange={(encoding: BlobEncoding | null) => {
              if (encoding !== null) {
                setBlobEncoding(encoding);
              }
            }}
          >
            <SelectTrigger>
              <SelectValue<string>>
                {(state) => state.selectedOption()}
              </SelectValue>
            </SelectTrigger>

            <SelectContent />
          </Select>

          <Show when={import.meta.env.DEV}>
            <DebugDialogButton title="Schema" data={data() ?? []} />
          </Show>
        </div>
      </div>
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
    <div id="indexes">
      <h2>
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
                      <Button variant="default" {...props}>
                        Add Index
                      </Button>
                    )}
                  />
                </>
              );
            }}
          </SafeSheet>

          <Button
            variant="destructive"
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
    </div>
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
    <div id="triggers">
      <h2>
        Triggers
        <Show when={import.meta.env.DEV}>
          <DebugDialogButton title="Triggers" data={triggers()} />
        </Show>
      </h2>

      <p class="text-sm">
        The admin dashboard currently does not support modifying triggers.
        Please use the editor to{" "}
        <a href="https://www.sqlite.org/lang_createtrigger.html">create</a> new
        triggers or <a href="https://sqlite.org/lang_droptrigger.html">drop</a>{" "}
        existing ones.
      </p>

      <div class="mt-4">
        <TableComponent loading={false} table={triggersTable()} />
      </div>
    </div>
  );
}

export function TablePane(props: {
  selectedTable: [Table, string] | [View, string];
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const selectedSchema = () => props.selectedTable[0];
  const isTable = () => tableType(selectedSchema()) === "table";

  const [searchParams, setSearchParams] = useSearchParams<{
    filter?: string;
    pageSize?: string;
    pageIndex?: string;
  }>();

  const [cursors, setCursors] = createWritableMemo<string[]>(() => {
    // Reset cursor whenever table or search params change.
    const _ = [props.selectedTable, searchParams.pageSize, searchParams.filter];
    console.debug("resetting cursor");
    return [];
  });

  const [filter, setFilter] = [
    () => searchParams.filter,
    (filter: string | undefined) => {
      setSearchParams({
        ...searchParams,
        filter,
      });
    },
  ];

  const [pagination, setPagination] = [
    (): PaginationState => {
      return {
        pageSize: safeParseInt(searchParams.pageSize) ?? 20,
        pageIndex: safeParseInt(searchParams.pageIndex) ?? 0,
      };
    },
    (s: PaginationState) => {
      setSearchParams({
        ...searchParams,
        pageSize: s.pageSize,
        pageIndex: s.pageIndex,
      });
    },
  ];

  const [sorting, setSorting] = createSignal<SortingState>([]);

  const records: QueryObserverResult<ListRowsResponse> = useQuery(() => ({
    queryKey: [
      selectedSchema().name,
      searchParams.filter,
      pagination(),
      sorting(),
    ] as ReadonlyArray<unknown>,
    queryFn: async ({ queryKey }) => {
      console.debug(`Fetching data with key: ${queryKey}`);

      try {
        const { pageSize, pageIndex } = pagination();

        const response = await fetchRows(
          selectedSchema().name,
          searchParams.filter ?? null,
          pageSize,
          pageIndex,
          cursors()[pageIndex - 1],
          formatSortingAsOrder(sorting()),
        );

        const newCursor = response.cursor;
        if (newCursor && pageIndex >= cursors().length) {
          setCursors([...cursors(), newCursor]);
        }

        return response;
      } catch (err) {
        // Reset.
        setSearchParams({
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

  return (
    <>
      <TableHeader
        table={props.selectedTable}
        allTables={props.schemas.tables}
        schemaRefetch={schemaRefetch}
        rowsRefetch={rowsRefetch}
      />

      <div class="flex flex-col gap-8 p-4">
        <Switch>
          <Match when={records.isError}>
            <div class="my-2 flex flex-col gap-4">
              Failed to fetch rows: {`${records.error}`}
              <div>
                <Button onClick={() => window.location.reload()}>Reload</Button>
              </div>
            </div>
          </Match>

          <Match when={records.isLoading}>
            <RecordTable
              selectedSchema={selectedSchema()}
              records={undefined}
              pagination={[pagination, setPagination]}
              filter={[filter, setFilter]}
              columnPinningState={[columnPinningState, setColumnPinningState]}
              sorting={[sorting, setSorting]}
              rowsRefetch={rowsRefetch}
            />
          </Match>

          <Match when={records.isSuccess}>
            <RecordTable
              selectedSchema={selectedSchema()}
              records={records.data}
              pagination={[pagination, setPagination]}
              filter={[filter, setFilter]}
              columnPinningState={[columnPinningState, setColumnPinningState]}
              sorting={[sorting, setSorting]}
              rowsRefetch={rowsRefetch}
            />
          </Match>
        </Switch>

        <Show when={isTable()}>
          <IndexTable
            table={selectedSchema() as Table}
            schemas={props.schemas}
            schemaRefetch={props.schemaRefetch}
          />
        </Show>

        <Show when={isTable()}>
          <TriggerTable
            table={selectedSchema() as Table}
            schemas={props.schemas}
          />
        </Show>
      </div>
    </>
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
