import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  splitProps,
  type Accessor,
} from "solid-js";
import { createWritableMemo } from "@solid-primitives/memo";
import {
  flexRender,
  createSolidTable,
  getCoreRowModel,
} from "@tanstack/solid-table";
import type {
  ColumnDef,
  OnChangeFn,
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
import { createWindowWidth } from "@/components/SplitView";

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
  onPaginationChange?: OnChangeFn<PaginationState>;

  onRowSelection?: (rows: Row<TData>[], value: boolean) => void;
  onRowClick?: (idx: number, row: TData) => void;
};

export function DataTable<TData, TValue>(props: Props<TData, TValue>) {
  const [local] = splitProps(props, ["columns", "data"]);
  const [rowSelection, setRowSelection] = createSignal<RowSelectionState>({});
  createEffect(() => {
    // NOTE: because we use our own state for row selection, reset it when data changes.
    local.data();
    setRowSelection({});
  });

  const paginationEnabled = () => props.onPaginationChange !== undefined;
  const [paginationState, setPaginationState] =
    createWritableMemo<PaginationState>(() => {
      // Whenever column definitions change, reset pagination state.
      //
      // FIXME: We should probably just not use a memo and and receive columns/data by value to rebuild instead.
      const _c = props.columns();

      return {
        pageIndex: props.pagination?.pageIndex ?? 0,
        pageSize: props.pagination?.pageSize ?? 20,
      };
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

    return createSolidTable({
      data: local.data() || [],
      state: {
        pagination: paginationState(),
        rowSelection: rowSelection(),
      },
      columns: columns(),
      getCoreRowModel: getCoreRowModel(),

      // NOTE: requires setting up the header cells with resize handles.
      // enableColumnResizing: true,
      // columnResizeMode: 'onChange',

      // pagination:
      manualPagination: paginationEnabled(),
      onPaginationChange: (state) => {
        setPaginationState(state);
        const handler = props.onPaginationChange;
        if (handler) {
          handler(state);
        }
      },
      rowCount: props.rowCount,

      // Just means, the input data is already filtered.
      manualFiltering: true,

      // If set to true, pagination will be reset to the first page when page-altering state changes
      // eg. data is updated, filters change, grouping changes, etc.
      //
      // NOTE: In our current setup this causes infinite reload cycles when paginating.
      autoResetPageIndex: false,

      enableRowSelection: true,
      enableMultiRowSelection: props.onRowSelection ? true : false,
      onRowSelectionChange: setRowSelection,
    });
  });

  return (
    <>
      {paginationEnabled() && (
        <PaginationControl table={table()} rowCount={props.rowCount} />
      )}

      <div class="rounded-md border">
        <Table>
          <TableHeader>
            <For each={table().getHeaderGroups()}>
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
            <Show
              when={table().getRowModel().rows?.length > 0}
              fallback={
                paginationState().pageIndex > 0 ? (
                  <TableRow>
                    <TableCell colSpan={local.columns.length}>
                      <span>Loading...</span>
                    </TableCell>
                  </TableRow>
                ) : (
                  <TableRow>
                    <TableCell colSpan={local.columns.length}>
                      <span>No results.</span>
                    </TableCell>
                  </TableRow>
                )
              }
            >
              <For each={table().getRowModel().rows}>
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
                            <div class="max-h-[80px] overflow-y-auto overflow-x-hidden break-words">
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
            </Show>
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
        value={table().getState().pagination.pageSize}
        onChange={(value) => {
          if (value) {
            table().setPageSize(value);
          }
        }}
        options={[10, 20, 30, 40, 50]}
        itemComponent={(props) => (
          <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
        )}
      >
        <SelectTrigger class="h-8 w-[4.5rem]">
          <SelectValue<string>>{(state) => state.selectedOption()}</SelectValue>
        </SelectTrigger>
        <SelectContent />
      </Select>
      <span class="whitespace-nowrap text-sm font-medium">per page</span>
    </div>
  );

  const PaginationInfoText = () => {
    const width = createWindowWidth();

    const pageIndex = () => table().getState().pagination.pageIndex;
    const pageCount = () => table().getPageCount();
    const rowCount = () => props.rowCount;

    return (
      <>
        {rowCount() && width() > 578
          ? `page ${pageIndex() + 1} of ${pageCount()} (${rowCount()} rows total)`
          : `page ${pageIndex() + 1} of ${pageCount()}`}
      </>
    );
  };

  return (
    <div class="flex justify-between">
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

          {/*
            // Doesn't work for cursors.
          <Button
            aria-label="Go to last page"
            variant="outline"
            size="icon"
            class="flex size-8"
            onClick={() => table().setPageIndex(table().getPageCount() - 1)}
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
                d="m7 7l5 5l-5 5m6-10l5 5l-5 5"
              />
            </svg>
          </Button>
          */}
        </div>

        <div class="flex items-center justify-center whitespace-nowrap text-sm font-medium">
          <PaginationInfoText />
        </div>
      </div>

      <PerPage />
    </div>
  );
}
