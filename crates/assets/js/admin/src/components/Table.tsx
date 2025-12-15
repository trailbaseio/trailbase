import {
  For,
  Match,
  Switch,
  createEffect,
  createMemo,
  createSignal,
  splitProps,
  type Accessor,
} from "solid-js";
import {
  flexRender,
  createSolidTable,
  getCoreRowModel,
} from "@tanstack/solid-table";
import type {
  ColumnDef,
  PaginationState,
  Row,
  RowSelectionState,
  Table as TableType,
} from "@tanstack/solid-table";

import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Checkbox } from "@/components/ui/checkbox";
import { createIsMobile } from "@/lib/signals";

export function safeParseInt(v: string | undefined): number | undefined {
  if (v !== undefined) {
    try {
      const num = parseInt(v);
      if (!isNaN(num)) {
        return num;
      }
    } catch (err) {
      console.warn(err);
    }
  }
  return undefined;
}

type Props<TData, TValue> = {
  columns: Accessor<ColumnDef<TData, TValue>[]>;
  data: Accessor<TData[] | undefined>;

  rowCount?: number;
  pagination?: PaginationState;
  onPaginationChange?: (state: PaginationState) => void;

  onRowSelection?: (rows: Row<TData>[], value: boolean) => void;
  onRowClick?: (idx: number, row: TData) => void;
};

// TODO: This entire implementation is incredibly messy. We should probably just
// receive a `createSolidTable` result, i.e. use TableImpl below. This would allow
// users direct access to the table state.
export function DataTable<TData, TValue>(props: Props<TData, TValue>) {
  const [local] = splitProps(props, ["columns", "data"]);
  const [rowSelection, setRowSelection] = createSignal<RowSelectionState>({});
  createEffect(() => {
    // NOTE: because we use our own state for row selection, reset it when data changes.
    const _ = local.data();
    setRowSelection({});
  });

  const columns = () => {
    const onRowSelection = props.onRowSelection;
    if (!onRowSelection) {
      return local.columns();
    }

    return [
      {
        id: "select",
        header: (ctx) => (
          <Checkbox
            checked={ctx.table.getIsAllPageRowsSelected()}
            onChange={(value: boolean) => {
              console.debug(
                "Select all",
                value,
                ctx.table.getIsSomeRowsSelected(),
              );
              ctx.table.toggleAllPageRowsSelected(value);

              const allRows = ctx.table.getRowModel();
              onRowSelection(allRows.rows, value);
            }}
            aria-label="Select all"
          />
        ),
        cell: (ctx) => (
          <Checkbox
            checked={ctx.row.getIsSelected()}
            onChange={(value: boolean) => {
              ctx.row.toggleSelected(value);

              onRowSelection([ctx.row], value);
            }}
            aria-label="Select row"
            onClick={(event: Event) => {
              // Prevent event from propagating and opening the edit row dialog.
              event.stopPropagation();
            }}
          />
        ),
        enableSorting: false,
        enableHiding: false,
      } as ColumnDef<TData, TValue>,
      ...local.columns(),
    ];
  };

  const table = createMemo(() => {
    console.debug("table data rebuild");

    const t = createSolidTable({
      data: local.data() || [],
      state: {
        pagination:
          props.pagination !== undefined
            ? {
                pageIndex: props.pagination.pageIndex ?? 0,
                pageSize: props.pagination.pageSize ?? 20,
              }
            : undefined,
        rowSelection: rowSelection(),
      },
      columns: columns(),
      getCoreRowModel: getCoreRowModel(),

      // NOTE: requires setting up the header cells with resize handles.
      // enableColumnResizing: true,
      // columnResizeMode: 'onChange',

      // pagination:
      manualPagination: true,
      onPaginationChange:
        props.onPaginationChange !== undefined
          ? (updater) => {
              const newState =
                typeof updater === "function"
                  ? updater(t.getState().pagination)
                  : updater;

              props.onPaginationChange!(newState);
            }
          : undefined,
      // If set to true, pagination will be reset to the first page when page-altering state changes
      // eg. data is updated, filters change, grouping changes, etc.
      //
      // NOTE: In our current setup this causes infinite reload cycles when paginating.
      autoResetPageIndex: false,
      rowCount: props.rowCount,

      // Just means, the input data is already filtered.
      manualFiltering: true,

      enableRowSelection: true,
      enableMultiRowSelection: props.onRowSelection ? true : false,
      onRowSelectionChange: setRowSelection,
    });

    return t;
  });

  return (
    <TableImpl
      table={table()}
      onRowClick={props.onRowClick}
      paginationEnabled={props.pagination !== undefined}
    />
  );
}

function TableImpl<TData>(props: {
  table: TableType<TData>;
  onRowClick?: (idx: number, row: TData) => void;
  paginationEnabled: boolean;
}) {
  const paginationEnabled = () => props.paginationEnabled;
  const paginationState = () => props.table.getState().pagination;
  const columns = () => props.table.options.columns;
  const numRows = () => props.table.getRowModel().rows?.length ?? 0;

  return (
    <>
      {paginationEnabled() && (
        <PaginationControl
          table={props.table}
          rowCount={props.table.options.rowCount}
        />
      )}

      <div class="rounded-md border">
        <Table>
          <TableHeader>
            <For each={props.table.getHeaderGroups()}>
              {(headerGroup) => (
                <TableRow>
                  <For each={headerGroup.headers}>
                    {(header) => {
                      return (
                        <TableHead>
                          {header.isPlaceholder
                            ? null
                            : flexRender(
                                header.column.columnDef.header,
                                header.getContext(),
                              )}
                        </TableHead>
                      );
                    }}
                  </For>
                </TableRow>
              )}
            </For>
          </TableHeader>

          <TableBody>
            <Switch>
              <Match when={numRows() > 0}>
                <For each={props.table.getRowModel().rows}>
                  {(row) => {
                    const onClick = () => {
                      // Don't trigger on text selection.
                      const selection = window.getSelection();
                      if (selection?.toString()) {
                        return;
                      }

                      const handler = props.onRowClick;
                      if (!handler) {
                        return;
                      }
                      handler(row.index, row.original);
                    };

                    return (
                      <TableRow
                        data-state={row.getIsSelected() && "selected"}
                        onClick={onClick}
                      >
                        <For each={row.getVisibleCells()}>
                          {(cell) => (
                            <TableCell>
                              <div class="max-h-[80px] overflow-x-hidden overflow-y-auto break-words">
                                {flexRender(
                                  cell.column.columnDef.cell,
                                  cell.getContext(),
                                )}
                              </div>
                            </TableCell>
                          )}
                        </For>
                      </TableRow>
                    );
                  }}
                </For>
              </Match>

              <Match when={paginationState().pageIndex > 0}>
                <TableRow>
                  <TableCell colSpan={columns().length}>
                    <span>Loading...</span>
                  </TableCell>
                </TableRow>
              </Match>

              <Match when={paginationState().pageIndex === 0}>
                <TableRow>
                  <TableCell colSpan={columns().length}>
                    <span>Empty</span>
                  </TableCell>
                </TableRow>
              </Match>
            </Switch>
          </TableBody>
        </Table>
      </div>

      {/*
        {paginationEnabled() && (
          <PaginationControl table={table} rowCount={props.rowCount} />
        )}
      */}
    </>
  );
}

function PaginationControl<TData>(props: {
  table: TableType<TData>;
  rowCount?: number;
}) {
  const table = () => props.table;

  const PerPage = () => (
    <div class="flex items-center space-x-2 py-1">
      <Select
        multiple={false}
        value={table().getState().pagination.pageSize}
        onChange={(value) => {
          table().setPageSize(value ?? 20);
        }}
        options={[10, 20, 50, 100]}
        itemComponent={(props) => (
          <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
        )}
      >
        <SelectTrigger class="h-8 w-[4.5rem]">
          <SelectValue<string>>{(state) => state.selectedOption()}</SelectValue>
        </SelectTrigger>
        <SelectContent />
      </Select>
      <span class="text-sm font-medium whitespace-nowrap">per page</span>
    </div>
  );

  const PaginationInfoText = () => {
    const isMobile = createIsMobile();

    const pageIndex = () => table().getState().pagination.pageIndex;
    const pageCount = () => table().getPageCount();
    const rowCount = () => props.rowCount;

    return (
      <>
        {rowCount() && !isMobile()
          ? `page ${pageIndex() + 1} of ${pageCount()} (${rowCount()} rows total)`
          : `page ${pageIndex() + 1} of ${pageCount()}`}
      </>
    );
  };

  return (
    <div class="flex flex-wrap justify-between">
      <div class="flex flex-row items-center gap-4">
        <div class="flex items-center space-x-2">
          <Button
            aria-label="Go to first page"
            variant="outline"
            class="flex size-8 p-0"
            onClick={() => table().setPageIndex(0)}
            disabled={!table().getCanPreviousPage()}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="size-4"
              aria-hidden="true"
              viewBox="0 0 24 24"
            >
              <path
                fill="none"
                stroke="currentColor"
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="m11 7l-5 5l5 5m6-10l-5 5l5 5"
              />
            </svg>
          </Button>
          <Button
            aria-label="Go to previous page"
            variant="outline"
            size="icon"
            class="size-8"
            onClick={() => table().previousPage()}
            disabled={!table().getCanPreviousPage()}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="size-4"
              aria-hidden="true"
              viewBox="0 0 24 24"
            >
              <path
                fill="none"
                stroke="currentColor"
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="m15 6l-6 6l6 6"
              />
            </svg>
          </Button>
          <Button
            aria-label="Go to next page"
            variant="outline"
            size="icon"
            class="size-8"
            onClick={() => table().nextPage()}
            disabled={!table().getCanNextPage()}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="size-4"
              aria-hidden="true"
              viewBox="0 0 24 24"
            >
              <path
                fill="none"
                stroke="currentColor"
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="m9 6l6 6l-6 6"
              />
            </svg>
          </Button>
        </div>

        <div class="flex items-center justify-center text-sm font-medium whitespace-nowrap">
          <PaginationInfoText />
        </div>
      </div>

      <PerPage />
    </div>
  );
}
