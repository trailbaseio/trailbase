import { Match, Switch, createMemo, createSignal, JSX } from "solid-js";
import { TbRefresh, TbTable, TbTrash } from "solid-icons/tb";
import { useQuery, useQueryClient } from "@tanstack/solid-query";
import { useSearchParams } from "@solidjs/router";
import { urlSafeBase64Decode } from "trailbase";
import type {
  ColumnDef,
  PaginationState,
  CellContext,
} from "@tanstack/solid-table";
import { createWritableMemo } from "@solid-primitives/memo";
import { createColumnHelper } from "@tanstack/solid-table";
import type { Row } from "@tanstack/solid-table";
import type { DialogTriggerProps } from "@kobalte/core/dialog";

import { Header } from "@/components/Header";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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
import {
  type FileUpload,
  type FileUploads,
  UploadedFile,
  UploadedFiles,
} from "@/components/tables/Files";

import { createConfigQuery, invalidateConfig } from "@/lib/api/config";
import type { Record, ArrayRecord } from "@/lib/record";
import { hashSqlValue } from "@/lib/value";
import { urlSafeBase64ToUuid, toHex } from "@/lib/utils";
import { equalQualifiedNames } from "@/lib/schema";
import { dropTable, dropIndex } from "@/lib/api/table";
import { deleteRows, fetchRows } from "@/lib/api/row";
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
  const table = () => props.table;
  const hidden = () => hiddenTable(table());
  const type = () => tableType(table());
  const satisfiesRecordApi = createMemo(() =>
    tableOrViewSatisfiesRecordApiRequirements(props.table, props.allTables),
  );

  const queryClient = useQueryClient();
  const config = createConfigQuery();
  const hasRecordApi = () => hasRecordApis(config?.data?.config, table().name);

  return (
    <div class="flex items-center justify-end gap-2">
      {/* Delete table button */}
      {!hidden() && (
        <DestructiveActionButton
          size="sm"
          action={() =>
            (async () => {
              await dropTable({
                name: prettyFormatQualifiedName(table().name),
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

function TableHeaderLeftButtons(props: {
  table: Table | View;
  indexes: TableIndex[];
  triggers: TableTrigger[];
  allTables: Table[];
  rowsRefetch: () => void;
}) {
  const type = () => tableType(props.table);
  const config = createConfigQuery();
  const apis = createMemo(() =>
    getRecordApis(config?.data?.config, props.table.name),
  );

  return (
    <>
      <IconButton tooltip="Refresh Data" onClick={props.rowsRefetch}>
        <TbRefresh />
      </IconButton>

      {apis().length > 0 && (
        <SchemaDialog
          tableName={prettyFormatQualifiedName(props.table.name)}
          apis={apis()}
        />
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
      leading={<SidebarTrigger />}
      title={headerTitle()}
      titleSelect={prettyFormatQualifiedName(props.table.name)}
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

  return {
    selected,
    response,
  };
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
  tableName: QualifiedName,
  pkColumnIndex: number,
  columns: Column[],
  blobEncoding: BlobEncoding,
): ColumnDef<ArrayRecord, SqlValue>[] {
  return columns.map((col, idx) => {
    const fk = getForeignKey(col.options);
    const notNull = isNotNull(col.options);
    const type = deriveCellType(col);

    const typeName = notNull ? type : type + "?";
    const fkSuffix = fk ? ` ‣ ${fk.foreign_table}[${fk.referred_columns}]` : "";
    const header = `${col.name} [${typeName}] ${fkSuffix}`;

    return {
      header,
      cell: (context) =>
        renderCell(
          context,
          tableName,
          columns,
          pkColumnIndex,
          {
            column: col,
            type,
          },
          blobEncoding,
        ),
      accessorFn: (row: ArrayRecord) => row[idx],
    } as ColumnDef<ArrayRecord, SqlValue>;
  });
}

function ArrayRecordTable(props: {
  state: TableState;
  pagination: SimpleSignal<PaginationState>;
  filter: SimpleSignal<string | undefined>;
  rowsRefetch: () => void;
}) {
  const [blobEncoding, setBlobEncoding] = createSignal<BlobEncoding>("mixed");
  const [editRow, setEditRow] = createSignal<Record | undefined>();
  const [selectedRows, setSelectedRows] = createSignal(
    new Map<string, SqlValue>(),
  );

  const table = () => props.state.selected;
  const mutable = () => tableType(table()) === "table" && !hiddenTable(table());

  const rowsRefetch = () => props.rowsRefetch();
  const columns = (): Column[] => props.state.response.columns;
  const totalRowCount = () => props.state.response.total_row_count;

  const pkColumnIndex = createMemo(() => findPrimaryKeyColumnIndex(columns()));
  const columnDefs = createMemo(() =>
    buildColumnDefs(
      table().name,
      pkColumnIndex(),
      props.state.response.columns,
      blobEncoding(),
    ),
  );

  return (
    <div>
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
                placeholder={`Filter Query, e.g. '(col0 > 5 || col0 = 0) || col1 ~ "%like"'`}
              />

              <div class="overflow-x-auto pt-4">
                <DataTable
                  // NOTE: The formatting is done via the columnsDefs.
                  columns={columnDefs}
                  data={() => props.state.response.rows}
                  rowCount={Number(totalRowCount())}
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
                      ? (_idx: number, row: ArrayRecord) => {
                          setEditRow(rowDataToRow(columns(), row));
                        }
                      : undefined
                  }
                  onRowSelection={
                    mutable()
                      ? (rows: Row<ArrayRecord>[], value: boolean) => {
                          const newSelection = new Map<string, SqlValue>(
                            selectedRows(),
                          );

                          for (const row of rows) {
                            const pkValue: SqlValue =
                              row.original[pkColumnIndex()];
                            const key = hashSqlValue(pkValue);

                            if (value) {
                              newSelection.set(key, pkValue);
                            } else {
                              newSelection.delete(key);
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

      <div class="my-2 flex justify-between gap-2">
        {mutable() && (
          <div class="flex gap-2">
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
                const ids = [...selectedRows().values()];
                if (ids.length === 0) {
                  return;
                }

                setSelectedRows(new Map<string, SqlValue>());
                deleteRows(prettyFormatQualifiedName(table().name), {
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
        </div>
      </div>
    </div>
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
  const indexes = createMemo(() => {
    return props.schemas.indexes.filter((idx) =>
      equalQualifiedNames(
        {
          name: idx.table_name,
          database_schema: idx.name.database_schema,
        },
        table().name,
      ),
    );
  });
  const triggers = createMemo(() => {
    return props.schemas.triggers.filter((trig) =>
      equalQualifiedNames(
        {
          name: trig.table_name,
          database_schema: trig.name.database_schema,
        },
        table().name,
      ),
    );
  });

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
      prettyFormatQualifiedName(props.selectedTable.name),
      searchParams.filter,
      pagination().pageIndex,
      pagination().pageSize,
    ],
    queryFn: async ({ queryKey }) => {
      const p = pagination();
      const c = cursors();
      console.debug(
        `Fetching data for key: ${queryKey}, index: ${p.pageIndex}, cursors: ${c}`,
      );

      try {
        const state = await buildTableState(
          props.selectedTable,
          searchParams.filter ?? null,
          p.pageSize,
          p.pageIndex,
          c[p.pageIndex - 1],
        );

        const cursor = state.response.cursor;
        if (cursor && p.pageIndex >= c.length) {
          setCursors([...c, cursor]);
        }

        return state;
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
            <ArrayRecordTable
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

                    <div class="space-y-2.5 overflow-x-auto">
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
                                  const qualifiedName =
                                    prettyFormatQualifiedName(
                                      row.original.name,
                                    );
                                  if (value) {
                                    newSelection.add(qualifiedName);
                                  } else {
                                    newSelection.delete(qualifiedName);
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
  triggerColumnHelper.accessor("name", {
    header: "name",
    cell: (props) => <p class="max-w-[20dvw]">{props.getValue().name}</p>,
  }),
  triggerColumnHelper.accessor("sql", {
    header: "statement",
    cell: (props) => <pre class="text-xs">{props.getValue()}</pre>,
  }),
] as ColumnDef<TableTrigger>[];
