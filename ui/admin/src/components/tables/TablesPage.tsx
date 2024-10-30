import {
  type Signal,
  type ResourceFetcherInfo,
  For,
  Match,
  Show,
  Switch,
  createMemo,
  createEffect,
  createResource,
  createSignal,
} from "solid-js";
import { createStore, type Store, type SetStoreFunction } from "solid-js/store";
import { useSearchParams } from "@solidjs/router";
import { persistentAtom } from "@nanostores/persistent";
import { useStore } from "@nanostores/solid";
import type {
  ColumnDef,
  PaginationState,
  CellContext,
} from "@tanstack/solid-table";
import { createColumnHelper } from "@tanstack/solid-table";
import type { DialogTriggerProps } from "@kobalte/core/dialog";
import { asyncBase64Encode } from "trailbase";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Separator } from "@/components/ui/separator";
import { SheetContent, SheetTrigger } from "@/components/ui/sheet";
import {
  Switch as SwitchUi,
  SwitchControl,
  SwitchLabel,
  SwitchThumb,
} from "@/components/ui/switch";
import {
  TbColumns,
  TbDownload,
  TbRefresh,
  TbTable,
  TbTrash,
  TbLock,
  TbEye,
  TbWand,
} from "solid-icons/tb";

import { CreateAlterTableForm } from "@/components/tables/CreateAlterTable";
import { CreateAlterIndexForm } from "@/components/tables/CreateAlterIndex";
import {
  DataTable,
  defaultPaginationState,
  safeParseInt,
} from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";
import { DestructiveActionButton } from "@/components/DestructiveActionButton";
import { InsertAlterRowForm } from "@/components/tables/InsertAlterRow";
import { RecordApiSettingsForm } from "@/components/tables/RecordApiSettings";
import { SplitView } from "@/components/SplitView";
import { SafeSheet } from "@/components/SafeSheet";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { createConfigQuery } from "@/lib/config";
import { adminFetch } from "@/lib/fetch";
import { urlSafeBase64ToUuid, showSaveFileDialog } from "@/lib/utils";
import { RecordApiConfig } from "@proto/config";
import { getAllTableSchemas, dropTable, dropIndex } from "@/lib/table";

import type {
  Column,
  DeleteRowsRequest,
  FileUpload,
  FileUploads,
  ListRowsResponse,
  ListSchemasResponse,
  Table,
  TableIndex,
  TableTrigger,
  View,
} from "@/lib/bindings";
import {
  findPrimaryKeyColumnIndex,
  isFileUploadColumn,
  isFileUploadsColumn,
  isJSONColumn,
  isNotNull,
  isUUIDv7Column,
  hiddenTable,
  tableType,
  type TableType,
  tableSatisfiesRecordApiRequirements,
  viewSatisfiesRecordApiRequirements,
} from "@/lib/schema";

// We deliberately want to use `Object` over `object` which includes primitive types such as string.
// eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
type RowData = (Object | undefined)[];
// eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
type Row = { [key: string]: Object | undefined };

function rowDataToRow(columns: Column[], row: RowData): Row {
  const result: Row = {};
  for (let i = 0; i < row.length; ++i) {
    result[columns[i].name] = row[i];
  }
  return result;
}

function renderCell(
  context: CellContext<RowData, unknown>,
  tableName: string,
  columns: Column[],
  pkIndex: number,
  cell: {
    col: Column;
    isUUIDv7: boolean;
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
    if (cell.isUUIDv7) {
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

async function deleteRows(tableName: string, request: DeleteRowsRequest) {
  const response = await adminFetch(`/table/${tableName}/rows`, {
    method: "DELETE",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(request),
  });
  return await response.text();
}

function Image(props: { url: string; mime: string }) {
  const [imageData] = createResource(async () => {
    const response = await adminFetch(props.url);
    return await asyncBase64Encode(await response.blob());
  });

  return (
    <Switch>
      <Match when={imageData.error}>{imageData.error}</Match>

      <Match when={imageData.loading}>Loading</Match>

      <Match when={imageData()}>
        <img class="size-[50px]" src={imageData()} />
      </Match>
    </Switch>
  );
}

function imageUrl(opts: {
  tableName: string;
  pkCol: string;
  pkVal: string;
  fileColName: string;
  index?: number;
}): string {
  const uri = `/table/${opts.tableName}/files?pk_column=${opts.pkCol}&pk_value=${opts.pkVal}&file_column_name=${opts.fileColName}`;
  const index = opts.index;
  if (index) {
    return `${uri}&file_index=${index}`;
  }
  return uri;
}

function SchemaDialogButton(props: {
  table: Table;
  indexes: TableIndex[];
  triggers: TableTrigger[];
}) {
  const columns = () => props.table.columns;
  const indexes = () => props.indexes;
  const triggers = () => props.triggers;
  const fks = () => props.table.foreign_keys;

  return (
    <div class="size-[28px] flex justify-center items-center rounded hover:bg-gray-200">
      <Dialog id="schema">
        <DialogTrigger>
          <TbColumns size={20} />
        </DialogTrigger>

        <DialogContent class="min-w-[80dvw]">
          <DialogHeader>
            <DialogTitle>Schema</DialogTitle>
          </DialogHeader>

          <div class="max-h-[80dvh] overflow-auto">
            <div class="mx-2 flex flex-col gap-2">
              <h2>Columns</h2>
              <pre class="w-[70vw] overflow-x-hidden text-xs">
                {JSON.stringify(columns(), null, 2)}
              </pre>

              <h2>Foreign Keys</h2>
              <pre class="w-[70vw] overflow-x-hidden text-xs">
                {JSON.stringify(fks(), null, 2)}
              </pre>

              <h2>Indexes</h2>
              <pre class="w-[70vw] overflow-x-hidden text-xs">
                {JSON.stringify(indexes(), null, 2)}
              </pre>

              <h2>Triggers</h2>
              <pre class="w-[70vw] overflow-x-hidden text-xs">
                {JSON.stringify(triggers(), null, 2)}
              </pre>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function TableHeaderRightHandButtons(props: {
  table: Table | View;
  allTables: Table[];
  schemaRefetch: () => Promise<void>;
}) {
  const table = () => props.table;
  const hidden = () => hiddenTable(table());
  const type = () => tableType(table());

  const satisfiesRecordApi = createMemo(() => {
    const t = type();
    if (t === "table") {
      return tableSatisfiesRecordApiRequirements(
        props.table as Table,
        props.allTables,
      );
    } else if (t === "view") {
      return viewSatisfiesRecordApiRequirements(
        props.table as View,
        props.allTables,
      );
    }

    return false;
  });

  const config = createConfigQuery();
  const recordApi = (): RecordApiConfig | undefined => {
    for (const c of config.data?.config?.recordApis ?? []) {
      if (c.tableName === table().name) {
        return c;
      }
    }
  };

  return (
    <div class="flex items-center justify-end gap-2">
      {/* Delete table button */}
      {!hidden() && (
        <DestructiveActionButton
          action={async () => {
            await dropTable({
              name: table().name,
            });
            props.schemaRefetch();
          }}
          msg="Deleting a table will irreversibly delete all the data contained. Are you sure you'd like to continue?"
        >
          <div class="flex gap-2 items-center">
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
                      <TooltipTrigger as={"div"}>
                        <Button
                          variant="outline"
                          class="flex items-center"
                          disabled={!satisfiesRecordApi()}
                          {...props}
                        >
                          API
                          <Checkbox
                            disabled={!satisfiesRecordApi()}
                            checked={recordApi() !== undefined}
                          />
                        </Button>
                      </TooltipTrigger>

                      <TooltipContent>
                        {satisfiesRecordApi() ? (
                          <p>Create a Record API endpoint for this table.</p>
                        ) : (
                          <p>
                            This table does not satisfy the requirements for
                            exposing a Record API: strictly typed {"&"} UUIDv7
                            primary key column.
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
                      <div class="flex gap-2 items-center">
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
  table: Table | View;
  indexes: TableIndex[];
  triggers: TableTrigger[];
  allTables: Table[];
  schemaRefetch: () => Promise<void>;
  rowsRefetch: () => Promise<void>;
}) {
  const table = () => props.table;
  const name = () => props.table.name;

  const type = () => tableType(table());
  const hasSchema = () => type() === "table";
  const header = () => {
    switch (type()) {
      case "view":
        return "View";
      case "virtualTable":
        return "Virtual Table";
      default:
        return "Table";
    }
  };

  return (
    <header class="flex flex-wrap items-center justify-between mx-4">
      <div class="h-[64px] flex items-center gap-2">
        <h1 class="flex gap-4 m-0">
          <span class="text-accent-600">{header()}</span>
          <span class="text-accent-600">&gt;</span>
          <span>{name()}</span>
        </h1>

        <div class="flex items-center gap-1">
          <button
            class="p-1 rounded hover:bg-gray-200"
            onClick={props.rowsRefetch}
          >
            <TbRefresh size={20} />
          </button>

          <div
            class="p-1 rounded hover:bg-gray-200"
            onClick={async () => {
              // NOTE: we cannot just have a <a download /> here since admin APIs require CSRF token.
              //
              // Not supported by firefox: https://developer.mozilla.org/en-US/docs/Web/API/Window/showSaveFilePicker#browser_compatibility
              // possible fallback: https://stackoverflow.com/a/67806663
              const response = await adminFetch(`/table/${name()}/schema.json`);
              const jsonText = await response.text();

              await showSaveFileDialog({
                contents: jsonText,
                filename: `${name()}.json`,
              });
            }}
          >
            <TbDownload size={20} />
          </div>

          {hasSchema() && (
            <SchemaDialogButton
              table={table() as Table}
              indexes={props.indexes}
              triggers={props.triggers}
            />
          )}
        </div>
      </div>

      <div class="h-[64px] min-w-[280px] content-center break-after-column">
        <TableHeaderRightHandButtons
          table={table()}
          allTables={props.allTables}
          schemaRefetch={props.schemaRefetch}
        />
      </div>
    </header>
  );
}

type TableStore = {
  selected: Table | View;
  schemas: ListSchemasResponse;

  // Filter & pagination
  filter: string | undefined;
  pagination: PaginationState;
};

type FetchArgs = {
  tableName: string;
  filter: string | undefined;
  pageSize: number;
  pageIndex: number;
  cursors: string[];
};

type TableState = {
  store: Store<TableStore>;
  setStore: SetStoreFunction<TableStore>;

  response: ListRowsResponse;

  // Derived
  pkColumnIndex: number;
  columnDefs: ColumnDef<RowData>[];
};

async function buildTableState(
  source: FetchArgs,
  store: Store<TableStore>,
  setStore: SetStoreFunction<TableStore>,
  info: ResourceFetcherInfo<TableState>,
): Promise<TableState> {
  const response = await fetchRows(source, { value: info.value?.response });

  const pkColumnIndex = findPrimaryKeyColumnIndex(response.columns);
  const columnDefs = buildColumnDefs(
    store.selected.name,
    tableType(store.selected),
    pkColumnIndex,
    response.columns,
  );

  return {
    store,
    setStore,
    response,
    pkColumnIndex,
    columnDefs,
  };
}

function buildColumnDefs(
  tableName: string,
  tableType: TableType,
  pkColumn: number,
  columns: Column[],
): ColumnDef<RowData>[] {
  return columns.map((col, idx) => {
    const notNull = isNotNull(col.options);
    const isJSON = isJSONColumn(col);
    const isUUIDv7 = isUUIDv7Column(col);
    const isFile = isFileUploadColumn(col);
    const isFiles = isFileUploadsColumn(col);

    // TODO: Add support for custom json schemas or generally JSON types.
    const type = (() => {
      if (isUUIDv7) return "UUIDv7";
      if (isJSON) return "JSON";
      if (isFile) return "File";
      if (isFiles) return "File[]";
      return col.data_type;
    })();

    return {
      header: `${col.name} [${type}${notNull ? "" : "?"}]`,
      cell: (context) =>
        renderCell(context, tableName, columns, pkColumn, {
          col: col,
          isUUIDv7,
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

async function fetchRows(
  source: FetchArgs,
  { value }: { value: ListRowsResponse | undefined },
): Promise<ListRowsResponse> {
  const pageIndex = source.pageIndex;
  const limit = source.pageSize;
  const cursors = source.cursors;

  const filter = source.filter ?? "";
  const filterQuery = filter
    .split("AND")
    .map((frag) => frag.trim().replaceAll(" ", ""))
    .join("&");

  const params = new URLSearchParams(filterQuery);
  params.set("limit", limit.toString());

  // Build the next UUIDv7 "cursor" from previous response and update local
  // cursor stack. If we're paging forward we add new cursors, otherwise we're
  // re-using previously seen cursors for consistency. We reset if we go back
  // to the start.
  if (pageIndex === 0) {
    cursors.length = 0;
  } else {
    const index = pageIndex - 1;
    if (index < cursors.length) {
      // Already known page
      params.set("cursor", cursors[index]);
    } else {
      // New page case: use cursor from previous response or fall back to more
      // expensive and inconsistent offset-based pagination.
      const cursor = value?.cursor;
      if (cursor) {
        cursors.push(cursor);
        params.set("cursor", cursor);
      } else {
        params.set("offset", `${pageIndex * source.pageSize}`);
      }
    }
  }

  try {
    const response = await adminFetch(
      `/table/${source.tableName}/rows?${params}`,
    );
    return (await response.json()) as ListRowsResponse;
  } catch (err) {
    if (value) {
      return value;
    }
    throw err;
  }
}

function RowDataTable(props: {
  state: TableState;
  rowsRefetch: () => Promise<void>;
}) {
  const [editRow, setEditRow] = createSignal<Row | undefined>();
  const [selectedRows, setSelectedRows] = createSignal(new Set<string>());

  const table = () => props.state.store.selected;
  const mutable = () => tableType(table()) === "table" && !hiddenTable(table());

  const refetch = async () => await props.rowsRefetch();
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
                <InsertAlterRowForm
                  schema={table() as Table}
                  rowsRefetch={refetch}
                  row={editRow()}
                  {...sheet}
                />
              </SheetContent>

              <FilterBar
                initial={props.state.store.filter}
                onSubmit={(value: string) => {
                  if (value === props.state.store.filter) {
                    refetch();
                  } else {
                    props.state.setStore("filter", (_prev) => value);
                  }
                }}
                example='e.g. "latency[lt]=2 AND status=200"'
              />

              <div class="space-y-2.5 overflow-contain">
                <DataTable
                  columns={() => props.state.columnDefs}
                  data={() => props.state.response.rows}
                  rowCount={totalRowCount()}
                  initialPagination={props.state.store.pagination}
                  onPaginationChange={(
                    p:
                      | PaginationState
                      | ((old: PaginationState) => PaginationState),
                  ) => {
                    props.state.setStore("pagination", p);
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
                      ? (_idx: number, row: RowData, value: boolean) => {
                          const rows = new Set(selectedRows());
                          const rowId = row[pkColumnIndex()] as string;
                          if (value) {
                            rows.add(rowId);
                          } else {
                            rows.delete(rowId);
                          }
                          setSelectedRows(rows);
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
        <div class="flex gap-2 my-2">
          {/* Insert Rows */}
          <SafeSheet
            children={(sheet) => {
              return (
                <>
                  <SheetContent class={sheetMaxWidth}>
                    <InsertAlterRowForm
                      schema={table() as Table}
                      rowsRefetch={refetch}
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
              const ids = Array.from(selectedRows());
              if (ids.length === 0) {
                return;
              }

              deleteRows(table().name, {
                primary_key_column: columns()[pkColumnIndex()].name,
                values: ids,
              })
                .then(() => {
                  setSelectedRows(new Set<string>());
                  refetch();
                })
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

function TablePane(props: {
  selectedTable: Table | View;
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const [editIndex, setEditIndex] = createSignal<TableIndex | undefined>();
  const [selectedIndexes, setSelectedIndexes] = createSignal(new Set<string>());

  const table = () => props.selectedTable;
  const indexes = () =>
    props.schemas.indexes.filter((idx) => idx.table_name === table().name);
  const triggers = () =>
    props.schemas.triggers.filter((trig) => trig.table_name === table().name);

  // Derived table() props.
  const type = () => tableType(table());
  const hidden = () => hiddenTable(table());

  const [searchParams, setSearchParams] = useSearchParams<{
    filter?: string;
    pageSize?: string;
  }>();

  function newStore(): TableStore {
    return {
      selected: props.selectedTable,
      schemas: props.schemas,
      filter: searchParams.filter ?? "",
      pagination: defaultPaginationState({
        // NOTE: We index has to start at 0 since we're building the list of
        // stable cursors as we incrementally page.
        index: 0,
        size: safeParseInt(searchParams.pageSize) ?? 20,
      }),
    };
  }

  // Cursors are deliberately kept out of the store to avoid tracking.
  let cursors: string[] = [];
  const [store, setStore] = createStore<TableStore>(newStore());
  createEffect(() => {
    if (store.selected.name !== props.selectedTable.name) {
      // Recreate the state/store when we switch tables.
      cursors = [];
      setStore(newStore());
    }

    setSearchParams({
      filter: store.filter,
    });
  });

  const buildFetchArgs = (): FetchArgs => ({
    // We need to access store properties here to react to them changing. It's
    // fine grained, so accessing a nested object like store.pagination isn't
    // enough.
    tableName: store.selected.name,
    filter: store.filter,
    pageSize: store.pagination.pageSize,
    pageIndex: store.pagination.pageIndex,
    cursors: cursors,
  });
  const [state, { refetch: rowsRefetch }] = createResource(
    buildFetchArgs,
    async (source: FetchArgs, info: ResourceFetcherInfo<TableState>) => {
      try {
        return await buildTableState(source, store, setStore, info);
      } catch (err) {
        setSearchParams({
          filter: undefined,
          pageIndex: undefined,
          pageSize: undefined,
        });

        throw err;
      }
    },
  );

  return (
    <>
      <TableHeader
        table={table()}
        indexes={indexes()}
        triggers={triggers()}
        allTables={props.schemas.tables}
        schemaRefetch={props.schemaRefetch}
        rowsRefetch={async () => {
          await rowsRefetch();
        }}
      />

      <Separator />

      <div class="flex flex-col gap-8 p-4">
        <Switch fallback={<>Loading...</>}>
          <Match when={state.error}>
            <div class="flex flex-col gap-4 my-2">
              Failed to fetch rows: {`${state.error}`}
              <div>
                <Button onClick={() => window.location.reload()}>Reload</Button>
              </div>
            </div>
          </Match>

          <Match when={state()}>
            <RowDataTable
              state={state()!}
              rowsRefetch={async () => {
                await rowsRefetch();
              }}
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

                    <div class="space-y-2.5 overflow-contain">
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
                            : (
                                _idx: number,
                                index: TableIndex,
                                value: boolean,
                              ) => {
                                const rows = new Set(selectedIndexes());
                                if (value) {
                                  rows.add(index.name);
                                } else {
                                  rows.delete(index.name);
                                }
                                setSelectedIndexes(rows);
                              }
                        }
                      />
                    </div>
                  </>
                );
              }}
            />

            {!hidden() && (
              <div class="flex gap-2 mt-2">
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
                        await dropIndex({ name });
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

function pickInitiallySelectedTable(
  tables: (Table | View)[],
  tableName: string | undefined,
): Table | View | undefined {
  if (tables.length === 0) {
    return undefined;
  }

  for (const table of tables) {
    if (tableName == table.name) {
      return table;
    }
  }
  return tables[0];
}

function TablePickerPane(props: {
  horizontal: boolean;
  tablesAndViews: (Table | View)[];
  selectedTableName: Signal<string | undefined>;
  schemaRefetch: () => Promise<void>;
}) {
  const tablesAndViews = createMemo(() =>
    props.tablesAndViews.toSorted((a, b) => {
      const aHidden = a.name.startsWith("_");
      const bHidden = b.name.startsWith("_");

      if (aHidden == bHidden) {
        return a.name.localeCompare(b.name);
      }
      // Sort hidden tables to the back.
      return aHidden ? 1 : -1;
    }),
  );
  const tables = () =>
    tablesAndViews().filter(
      (either) => (either as Table) !== undefined,
    ) as Table[];

  const showHidden = useStore($showHiddenTables);

  const [selectedTableName, setSelectedTableName] = props.selectedTableName;

  createEffect(() => {
    // Update search params.
    const tableName = selectedTableName();

    const [searchParams, setSearchParams] = useSearchParams<{
      table: string;
    }>();
    const index = tableName
      ? tablesAndViews().findIndex((t) => t.name === tableName)
      : -1;
    if (index < 0) {
      console.debug("Did not find table:", tableName);
      setSelectedTableName(
        pickInitiallySelectedTable(tablesAndViews(), searchParams.table)?.name,
      );
    }

    if (tableName !== searchParams.table) {
      setSearchParams({ table: tableName });
    }
  });

  const flexStyle = () => (props.horizontal ? "flex flex-col h-dvh" : "flex");

  return (
    <div class={`${flexStyle()} gap-2 justify-between`}>
      {/* TODO: Maybe add a thin bottom scrollbar to make overflow more apparent */}
      <div class={`${flexStyle()} gap-2 overflow-scroll hide-scrollbars p-4`}>
        <For each={tablesAndViews()}>
          {(item: Table | View) => {
            const hidden = hiddenTable(item);
            const type = tableType(item);
            const selected = () => item.name === selectedTableName();

            return (
              <Button
                variant={selected() ? "default" : "outline"}
                onClick={() => setSelectedTableName(item.name)}
                class="flex gap-2"
              >
                <span
                  class={
                    !selected() && hidden
                      ? "truncate text-gray-500"
                      : "truncate"
                  }
                >
                  {item.name}
                </span>
                {hidden && <TbLock />}
                {type === "view" && <TbEye />}
                {type === "virtualTable" && <TbWand />}
              </Button>
            );
          }}
        </For>

        <SafeSheet
          children={(sheet) => {
            return (
              <>
                <SheetContent class={sheetMaxWidth}>
                  <CreateAlterTableForm
                    schemaRefetch={props.schemaRefetch}
                    allTables={tables()}
                    setSelected={setSelectedTableName}
                    {...sheet}
                  />
                </SheetContent>

                <SheetTrigger
                  as={(props: DialogTriggerProps) => (
                    <Button variant="secondary" {...props}>
                      Add Table
                    </Button>
                  )}
                />
              </>
            );
          }}
        />
      </div>

      <SwitchUi
        class="flex items-center space-x-2 m-4"
        checked={showHidden()}
        onChange={(show: boolean) => {
          if (!show && selectedTableName()?.startsWith("_")) {
            setSelectedTableName(undefined);
          }
          console.debug("Show hidden tables:", show);
          $showHiddenTables.set(show);
        }}
      >
        <SwitchControl>
          <SwitchThumb />
        </SwitchControl>
        <SwitchLabel>Hidden Tables</SwitchLabel>
      </SwitchUi>
    </div>
  );
}

function TableSplitView(props: {
  schemas: ListSchemasResponse;
  schemaRefetch: () => Promise<void>;
}) {
  const showHidden = useStore($showHiddenTables);
  function filterHidden(
    schemas: (Table | View)[],
    showHidden: boolean,
  ): (Table | View)[] {
    return schemas.filter((s) => showHidden || !s.name.startsWith("_"));
  }
  const tablesAndViews = () =>
    filterHidden(
      [...props.schemas.tables, ...props.schemas.views],
      showHidden(),
    );

  const [searchParams] = useSearchParams<{ table: string }>();
  const selectedTableNameSignal = createSignal<string | undefined>(
    pickInitiallySelectedTable(tablesAndViews(), searchParams.table)?.name,
  );

  const selectedTable = (): Table | View | undefined => {
    const [selectedTableName] = selectedTableNameSignal;
    const table = props.schemas.tables.find(
      (t) => t.name == selectedTableName(),
    );
    if (table) {
      return table;
    }
    return props.schemas.views.find((t) => t.name == selectedTableName());
  };

  const First = (p: { horizontal: boolean }) => (
    <TablePickerPane
      horizontal={p.horizontal}
      tablesAndViews={tablesAndViews()}
      selectedTableName={selectedTableNameSignal}
      schemaRefetch={props.schemaRefetch}
    />
  );
  const Second = () => (
    <Show
      when={selectedTable() !== undefined}
      fallback={<div class="m-4">No table selected</div>}
    >
      <TablePane
        selectedTable={selectedTable()!}
        schemas={props.schemas}
        schemaRefetch={props.schemaRefetch}
      />
    </Show>
  );

  return <SplitView first={First} second={Second} />;
}

export function TablesPage() {
  const [schemaFetch, { refetch }] = createResource(getAllTableSchemas);

  return (
    <Switch>
      <Match when={schemaFetch.error}>
        <span>Schema fetch error: {JSON.stringify(schemaFetch.latest)}</span>
      </Match>

      <Match when={schemaFetch()}>
        <TableSplitView
          schemas={schemaFetch()!}
          schemaRefetch={async () => {
            const schemas = await refetch();
            console.debug("All table schemas re-fetched:", schemas);
          }}
        />
      </Match>
    </Switch>
  );
}

const sheetMaxWidth = "sm:max-w-[520px]";
const $showHiddenTables = persistentAtom<boolean>("show_hidden_tables", false, {
  encode: JSON.stringify,
  decode: JSON.parse,
});

const indexColumns = [
  {
    header: "name",
    accessorKey: "name",
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
