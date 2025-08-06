import { For, Match, Switch, createMemo, createSignal } from "solid-js";
import { useQuery, useQueryClient } from "@tanstack/solid-query";
import { useSearchParams } from "@solidjs/router";
import type {
  ColumnDef,
  PaginationState,
  CellContext,
} from "@tanstack/solid-table";
import { createWritableMemo } from "@solid-primitives/memo";
import { createColumnHelper } from "@tanstack/solid-table";
import type { Row } from "@tanstack/solid-table";
import type { DialogTriggerProps } from "@kobalte/core/dialog";
import { asyncBase64Encode } from "trailbase";

import { Header } from "@/components/Header";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { SheetContent, SheetTrigger } from "@/components/ui/sheet";
import { TbRefresh, TbTable, TbTrash } from "solid-icons/tb";

import {
  SchemaDialog,
  DebugSchemaDialogButton,
} from "@/components/tables/SchemaDownload";
import { CreateAlterTableForm } from "@/components/tables/CreateAlterTable";
import { CreateAlterIndexForm } from "@/components/tables/CreateAlterIndex";
import { DataTable, safeParseInt } from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";
import { DestructiveActionButton } from "@/components/DestructiveActionButton";
import { IconButton } from "@/components/IconButton";
import { InsertUpdateRowForm } from "@/components/tables/InsertUpdateRow";
import {
  RecordApiSettingsForm,
  hasRecordApis,
  getRecordApis,
} from "@/components/tables/RecordApiSettings";
import { SafeSheet } from "@/components/SafeSheet";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { createConfigQuery, invalidateConfig } from "@/lib/config";
import { type FormRow, RowData } from "@/lib/convert";
import { adminFetch } from "@/lib/fetch";
import { urlSafeBase64ToUuid } from "@/lib/utils";
import { dropTable, dropIndex } from "@/lib/table";
import { deleteRows, fetchRows } from "@/lib/row";
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
  type TableType,
} from "@/lib/schema";

import type { Column } from "@bindings/Column";
import type { ListRowsResponse } from "@bindings/ListRowsResponse";
import type { ListSchemasResponse } from "@bindings/ListSchemasResponse";
import type { Table } from "@bindings/Table";
import type { TableIndex } from "@bindings/TableIndex";
import type { TableTrigger } from "@bindings/TableTrigger";
import type { View } from "@bindings/View";
import { QualifiedName } from "@bindings/QualifiedName";

export type SimpleSignal<T> = [get: () => T, set: (state: T) => void];

type FileUpload = {
  id: string;
  filename: string | undefined;
  content_type: string | undefined;
  mime_type: string | string;
};

type FileUploads = FileUpload[];

function rowDataToRow(columns: Column[], row: RowData): FormRow {
  const result: FormRow = {};
  for (let i = 0; i < row.length; ++i) {
    result[columns[i].name] = row[i];
  }
  return result;
}

function renderCell(
  context: CellContext<RowData, unknown>,
  tableName: QualifiedName,
  columns: Column[],
  pkIndex: number,
  cell: {
    col: Column;
    isUUID: boolean;
    isJSON: boolean;
    isFile: boolean;
    isFiles: boolean;
  },
): unknown {
  const value = context.getValue();
  if (value === null) {
    return "NULL";
  }

  if (typeof value === "string") {
    if (cell.isUUID) {
      return urlSafeBase64ToUuid(value);
    }

    const imageMime = (f: FileUpload) => {
      const mime = f.mime_type;
      return mime === "image/jpeg" || mime === "image/png";
    };

    if (cell.isFile) {
      const fileUpload = JSON.parse(value) as FileUpload;
      if (imageMime(fileUpload)) {
        const pkCol = columns[pkIndex].name;
        const pkVal = context.row.original[pkIndex] as string;
        const url = imageUrl({
          tableName,
          pkCol,
          pkVal,
          fileColName: cell.col.name,
        });

        return <Image url={url} mime={fileUpload.mime_type} />;
      }
    } else if (cell.isFiles) {
      const fileUploads = JSON.parse(value) as FileUploads;

      const indexes: number[] = [];
      for (let i = 0; i < fileUploads.length; ++i) {
        const file = fileUploads[i];
        if (imageMime(file)) {
          indexes.push(i);
        }

        if (indexes.length >= 3) break;
      }

      if (indexes.length > 0) {
        const pkCol = columns[pkIndex].name;
        const pkVal = context.row.original[pkIndex] as string;
        return (
          <div class="flex gap-2">
            <For each={indexes}>
              {(index: number) => {
                const fileUpload = fileUploads[index];
                const url = imageUrl({
                  tableName,
                  pkCol,
                  pkVal,
                  fileColName: cell.col.name,
                  index,
                });

                return <Image url={url} mime={fileUpload.mime_type} />;
              }}
            </For>
          </div>
        );
      }
    }
  }

  return value;
}

function Image(props: { url: string; mime: string }) {
  const imageData = useQuery(() => ({
    queryKey: ["tableImage", props.url],
    queryFn: async () => {
      const response = await adminFetch(props.url);
      return await asyncBase64Encode(await response.blob());
    },
  }));

  return (
    <Switch>
      <Match when={imageData.isError}>{`${imageData.error}`}</Match>

      <Match when={imageData.isLoading}>Loading</Match>

      <Match when={imageData.data}>
        <img class="size-[50px]" src={imageData.data} />
      </Match>
    </Switch>
  );
}

function imageUrl(opts: {
  tableName: QualifiedName;
  pkCol: string;
  pkVal: string;
  fileColName: string;
  index?: number;
}): string {
  const tableName: string = prettyFormatQualifiedName(opts.tableName);
  const uri = `/table/${tableName}/files?pk_column=${opts.pkCol}&pk_value=${opts.pkVal}&file_column_name=${opts.fileColName}`;
  const index = opts.index;
  if (index) {
    return `${uri}&file_index=${index}`;
  }
  return uri;
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
  const table = () => props.table;
  const hidden = () => hiddenTable(table());
  const type = () => tableType(table());
  const satisfiesRecordApi = createMemo(() =>
    tableOrViewSatisfiesRecordApiRequirements(props.table, props.allTables),
  );

  const queryClient = useQueryClient();
  const config = createConfigQuery();
  const hasRecordApi = () =>
    hasRecordApis(config?.data?.config, table().name.name);

  return (
    <div class="flex items-center justify-end gap-2">
      {/* Delete table button */}
      {!hidden() && (
        <DestructiveActionButton
          action={() =>
            (async () => {
              await dropTable({
                name: table().name.name,
                dry_run: null,
              });

              invalidateConfig(queryClient);
              await props.schemaRefetch();
            })().catch(console.error)
          }
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
                    <Button variant="default" {...props}>
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

function TableHeaderLeftButtons(props: {
  table: Table | View;
  indexes: TableIndex[];
  triggers: TableTrigger[];
  allTables: Table[];
  rowsRefetch: () => void;
}) {
  const type = () => tableType(props.table);
  const config = createConfigQuery();
  const tableName = () => props.table.name.name;
  const apis = createMemo(() =>
    getRecordApis(config?.data?.config, tableName()),
  );

  return (
    <>
      <IconButton tooltip="Refresh Data" onClick={props.rowsRefetch}>
        <TbRefresh size={18} />
      </IconButton>

      {apis().length > 0 && (
        <SchemaDialog tableName={tableName()} apis={apis()} />
      )}

      {import.meta.env.DEV && type() === "table" && (
        <DebugSchemaDialogButton
          table={props.table as Table}
          indexes={props.indexes}
          triggers={props.triggers}
        />
      )}
    </>
  );
}

function TableHeader(props: {
  table: Table | View;
  indexes: TableIndex[];
  triggers: TableTrigger[];
  allTables: Table[];
  schemaRefetch: () => Promise<void>;
  rowsRefetch: () => void;
}) {
  const headerTitle = () => {
    switch (tableType(props.table)) {
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
      title={headerTitle()}
      titleSelect={props.table.name.name}
      left={
        <TableHeaderLeftButtons
          table={props.table}
          indexes={props.indexes}
          triggers={props.triggers}
          allTables={props.allTables}
          rowsRefetch={props.rowsRefetch}
        />
      }
      right={
        <TableHeaderRightHandButtons
          table={props.table}
          allTables={props.allTables}
          schemaRefetch={props.schemaRefetch}
        />
      }
    />
  );
}

type TableState = {
  selected: Table | View;

  // Derived
  pkColumnIndex: number;
  columnDefs: ColumnDef<RowData>[];

  response: ListRowsResponse;
};

async function buildTableState(
  selected: Table | View,
  filter: string | null,
  pageSize: number,
  pageIndex: number,
  cursor: string | null,
): Promise<TableState> {
  const response = await fetchRows(
    selected.name,
    filter,
    pageSize,
    pageIndex,
    cursor,
  );

  const pkColumnIndex = findPrimaryKeyColumnIndex(response.columns);
  const columnDefs = buildColumnDefs(
    selected.name,
    tableType(selected),
    pkColumnIndex,
    response.columns,
  );

  return {
    selected,
    pkColumnIndex,
    columnDefs,
    response,
  };
}

function buildColumnDefs(
  tableName: QualifiedName,
  tableType: TableType,
  pkColumn: number,
  columns: Column[],
): ColumnDef<RowData>[] {
  return columns.map((col, idx) => {
    const fk = getForeignKey(col.options);
    const notNull = isNotNull(col.options);
    const isJSON = isJSONColumn(col);
    const isUUID = isUUIDColumn(col);
    const isFile = isFileUploadColumn(col);
    const isFiles = isFileUploadsColumn(col);

    // TODO: Add support for custom json schemas or generally JSON types.
    const type = ((): string => {
      if (isUUID) return "UUID";
      if (isJSON) return "JSON";
      if (isFile) return "File";
      if (isFiles) return "File[]";
      return col.data_type;
    })();

    const typeName = notNull ? type : type + "?";
    const fkSuffix = fk ? ` ‣ ${fk.foreign_table}[${fk.referred_columns}]` : "";
    const header = `${col.name} [${typeName}] ${fkSuffix}`;

    return {
      header,
      cell: (context) =>
        renderCell(context, tableName, columns, pkColumn, {
          col: col,
          isUUID,
          isJSON,
          // FIXME: Whether or not an image can be rendered depends on whether
          // Record API read-access is configured and not the tableType. We
          // could also consider to decouple by providing a dedicated admin
          // file-access endpoint.
          isFile: isFile && tableType !== "view",
          isFiles: isFiles && tableType !== "view",
        }),
      accessorFn: (row: RowData) => row[idx],
    };
  });
}

function RowDataTable(props: {
  state: TableState;
  pagination: SimpleSignal<PaginationState>;
  filter: SimpleSignal<string | undefined>;
  rowsRefetch: () => void;
}) {
  const [editRow, setEditRow] = createSignal<FormRow | undefined>();
  const [selectedRows, setSelectedRows] = createSignal(new Set<string>());

  const table = () => props.state.selected;
  const mutable = () => tableType(table()) === "table" && !hiddenTable(table());

  const rowsRefetch = () => props.rowsRefetch();
  const columns = (): Column[] => props.state.response.columns;
  const totalRowCount = () => Number(props.state.response.total_row_count);
  const pkColumnIndex = () => props.state.pkColumnIndex;

  return (
    <>
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
                  schema={table() as Table}
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
                placeholder={`Filter Query, e.g. '(col0 > 5 && col0 < 20) || col1 = "val"'`}
              />

              <div class="space-y-2 overflow-auto">
                <DataTable
                  columns={() => props.state.columnDefs}
                  data={() => props.state.response.rows}
                  rowCount={totalRowCount()}
                  pagination={props.pagination[0]()}
                  onPaginationChange={(
                    p:
                      | PaginationState
                      | ((old: PaginationState) => PaginationState),
                  ) => {
                    if (typeof p === "function") {
                      const state = p(props.pagination[0]());
                      props.pagination[1](state);
                    } else {
                      props.pagination[1](p);
                    }
                  }}
                  onRowClick={
                    mutable()
                      ? (_idx: number, row: RowData) => {
                          setEditRow(rowDataToRow(columns(), row));
                        }
                      : undefined
                  }
                  onRowSelection={
                    mutable()
                      ? (rows: Row<RowData>[], value: boolean) => {
                          const newSelection = new Set(selectedRows());
                          for (const row of rows) {
                            const rowId = row.original[
                              pkColumnIndex()
                            ] as string;
                            if (value) {
                              newSelection.add(rowId);
                            } else {
                              newSelection.delete(rowId);
                            }
                          }
                          setSelectedRows(newSelection);
                        }
                      : undefined
                  }
                />
              </div>
            </>
          );
        }}
      />

      {mutable() && (
        <div class="my-2 flex gap-2">
          {/* Insert Rows */}
          <SafeSheet
            children={(sheet) => {
              return (
                <>
                  <SheetContent class={sheetMaxWidth}>
                    <InsertUpdateRowForm
                      schema={table() as Table}
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
              const ids = [...selectedRows()];
              if (ids.length === 0) {
                return;
              }

              setSelectedRows(new Set<string>());
              deleteRows(table().name.name, {
                primary_key_column: columns()[pkColumnIndex()].name,
                values: ids,
              })
                .finally(rowsRefetch)
                .catch(console.error);
            }}
          >
            Delete rows
          </Button>
        </div>
      )}
    </>
  );
}

export function TablePane(props: {
  selectedTable: Table | View;
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const [editIndex, setEditIndex] = createSignal<TableIndex | undefined>();
  const [selectedIndexes, setSelectedIndexes] = createSignal(new Set<string>());

  const table = () => props.selectedTable;
  const indexes = () =>
    props.schemas.indexes.filter((idx) => {
      const tbl = table();
      return (
        (idx.name.database_schema ?? "main") ==
          (tbl.name.database_schema ?? "main") &&
        idx.table_name === tbl.name.name
      );
    });
  const triggers = () =>
    props.schemas.triggers.filter(
      (trig) => trig.table_name === table().name.name,
    );

  // Derived table() props.
  const type = () => tableType(table());
  const hidden = () => hiddenTable(table());

  const [searchParams, setSearchParams] = useSearchParams<{
    filter?: string;
    pageSize?: string;
  }>();

  // Reset when table or search params change
  const reset = () => {
    return [props.selectedTable, searchParams.pageSize, searchParams.filter];
  };
  const [pageIndex, setPageIndex] = createWritableMemo<number>(() => {
    reset();
    return 0;
  });
  const [cursors, setCursors] = createWritableMemo<string[]>(() => {
    reset();
    return [];
  });

  const filter = () => searchParams.filter;
  const setFilter = (filter: string | undefined) => {
    setSearchParams({
      ...searchParams,
      filter,
    });
  };
  const pagination = (): PaginationState => {
    return {
      pageSize: safeParseInt(searchParams.pageSize) ?? 20,
      pageIndex: pageIndex(),
    };
  };

  const state = useQuery(() => ({
    queryKey: [
      "tableData",
      searchParams.filter,
      pagination().pageIndex,
      pagination().pageSize,
      prettyFormatQualifiedName(props.selectedTable.name),
    ],
    queryFn: async ({ queryKey }) => {
      const p = pagination();
      const c = cursors();
      console.debug(
        `Fetching data for key: ${queryKey}, index: ${p.pageIndex}, cursors: ${c}`,
      );

      try {
        const response = await buildTableState(
          props.selectedTable,
          searchParams.filter ?? null,
          p.pageSize,
          p.pageIndex,
          c[p.pageIndex - 1],
        );

        const cursor = response.response.cursor;
        if (cursor && p.pageIndex >= c.length) {
          setCursors([...c, cursor]);
        }

        return response;
      } catch (err) {
        // Reset.
        setPageIndex(0);
        setSearchParams({
          filter: undefined,
          pageSize: undefined,
        });

        throw err;
      }
    },
  }));

  const client = useQueryClient();
  const rowsRefetch = () => {
    // Refetches the actual table contents above.
    client.invalidateQueries({
      queryKey: ["tableData"],
    });
  };
  const schemaRefetch = async () => {
    // First re-fetch the schema then the data rows to trigger a re-render.
    await props.schemaRefetch();
    rowsRefetch();
  };

  const setPagination = (s: PaginationState) => {
    const current = pagination();
    if (current.pageSize !== s.pageSize) {
      setSearchParams({
        ...searchParams,
        pageSize: s.pageSize,
      });
      return;
    }

    if (current.pageIndex != s.pageIndex) {
      setPageIndex(s.pageIndex);
    }
  };

  const Fallback = () => {
    // TODO: Return a shimmery table to reduce visual jank.
    return <>Loading...</>;
  };

  return (
    <>
      <TableHeader
        table={table()}
        indexes={indexes()}
        triggers={triggers()}
        allTables={props.schemas.tables}
        schemaRefetch={schemaRefetch}
        rowsRefetch={rowsRefetch}
      />

      <div class="flex flex-col gap-8 p-4">
        <Switch fallback={Fallback()}>
          <Match when={state.isError}>
            <div class="my-2 flex flex-col gap-4">
              Failed to fetch rows: {`${state.error}`}
              <div>
                <Button onClick={() => window.location.reload()}>Reload</Button>
              </div>
            </div>
          </Match>

          <Match when={state.data}>
            <RowDataTable
              state={state.data!}
              pagination={[pagination, setPagination]}
              filter={[filter, setFilter]}
              rowsRefetch={rowsRefetch}
            />
          </Match>
        </Switch>

        {type() === "table" && (
          <div id="indexes">
            <h2>Indexes</h2>

            <SafeSheet
              open={[
                () => editIndex() !== undefined,
                (isOpen: boolean | ((value: boolean) => boolean)) => {
                  if (!isOpen) {
                    setEditIndex(undefined);
                  }
                },
              ]}
              children={(sheet) => {
                return (
                  <>
                    <SheetContent class={sheetMaxWidth}>
                      <CreateAlterIndexForm
                        schema={editIndex()}
                        table={table() as Table}
                        schemaRefetch={props.schemaRefetch}
                        {...sheet}
                      />
                    </SheetContent>

                    <div class="space-y-2.5 overflow-auto">
                      <DataTable
                        columns={() => indexColumns}
                        data={indexes}
                        onRowClick={
                          hidden()
                            ? undefined
                            : (_idx: number, index: TableIndex) => {
                                setEditIndex(index);
                              }
                        }
                        onRowSelection={
                          hidden()
                            ? undefined
                            : (rows: Row<TableIndex>[], value: boolean) => {
                                const newSelection = new Set(selectedIndexes());
                                for (const row of rows) {
                                  const name = row.original.name.name;
                                  if (value) {
                                    newSelection.add(name);
                                  } else {
                                    newSelection.delete(name);
                                  }
                                }
                                setSelectedIndexes(newSelection);
                              }
                        }
                      />
                    </div>
                  </>
                );
              }}
            />

            {!hidden() && (
              <div class="mt-2 flex gap-2">
                <SafeSheet
                  children={(sheet) => {
                    return (
                      <>
                        <SheetContent class={sheetMaxWidth}>
                          <CreateAlterIndexForm
                            schemaRefetch={props.schemaRefetch}
                            table={table() as Table}
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
                />

                <Button
                  variant="destructive"
                  disabled={selectedIndexes().size == 0}
                  onClick={() => {
                    const names = Array.from(selectedIndexes());
                    if (names.length == 0) {
                      return;
                    }

                    const deleteIndexes = async () => {
                      for (const name of names) {
                        await dropIndex({ name, dry_run: null });
                      }

                      setSelectedIndexes(new Set<string>());
                      props.schemaRefetch();
                    };

                    deleteIndexes().catch(console.error);
                  }}
                >
                  Delete indexes
                </Button>
              </div>
            )}
          </div>
        )}

        {type() === "table" && (
          <div id="triggers">
            <h2>Triggers</h2>

            <p class="text-sm">
              The admin dashboard currently does not support modifying triggers.
              Please use the editor to{" "}
              <a href="https://www.sqlite.org/lang_createtrigger.html">
                create
              </a>{" "}
              new triggers or{" "}
              <a href="https://sqlite.org/lang_droptrigger.html">drop</a>{" "}
              existing ones.
            </p>

            <div class="mt-4">
              <DataTable columns={() => triggerColumns} data={triggers} />
            </div>
          </div>
        )}
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

const triggerColumnHelper = createColumnHelper<TableTrigger>();
const triggerColumns = [
  triggerColumnHelper.accessor("name", {}),
  triggerColumnHelper.accessor("sql", {
    header: "statement",
    cell: (props) => <pre class="text-xs">{props.getValue()}</pre>,
  }),
] as ColumnDef<TableTrigger>[];
