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
import { useSearchParams } from "@solidjs/router";
import type { ColumnDef } from "@tanstack/solid-table";
import {
  flexRender,
  createSolidTable,
  getCoreRowModel,
} from "@tanstack/solid-table";
import type {
  Table as TableType,
  PaginationState,
  OnChangeFn,
} from "@tanstack/table-core";

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

type SearchParams = {
  pageIndex: string;
  pageSize: string;
};

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

export function defaultPaginationState(opts?: {
  index?: number;
  size?: number;
}): PaginationState {
  return {
    pageIndex: opts?.index ?? 0,
    pageSize: opts?.size ?? 20,
  };
}

type Props<TData, TValue> = {
  columns: Accessor<ColumnDef<TData, TValue>[]>;
  data: Accessor<TData[] | undefined>;

  rowCount?: number;
  initialPagination?: PaginationState;
  onPaginationChange?: OnChangeFn<PaginationState>;

  onRowSelection?: (idx: number, row: TData, value: boolean) => void;
  onRowClick?: (idx: number, row: TData) => void;
};

export function DataTable<TData, TValue>(props: Props<TData, TValue>) {
  const [local] = splitProps(props, ["columns", "data"]);
  const [rowSelection, setRowSelection] = createSignal({});

  const [searchParams, setSearchParams] = useSearchParams<SearchParams>();
  const paginationEnabled = props.onPaginationChange !== undefined;

  function initPaginationState(): PaginationState {
    return {
      pageIndex:
        safeParseInt(searchParams.pageIndex) ??
        props.initialPagination?.pageIndex ??
        0,
      pageSize:
        safeParseInt(searchParams.pageSize) ??
        props.initialPagination?.pageSize ??
        20,
    };
  }

  const [paginationState, setPaginationState] =
    createWritableMemo<PaginationState>(() => {
      // Whenever column definitions change, reset pagination state.
      //
      // FIXME: column definition is an insufficient proxy, we should also reset
      // when switching between tables/views with matching schemas. Maybe we
      // should just inject a Signal<PaginationState>
      const _c = props.columns();

      return initPaginationState();
    });
  createEffect(() => {
    setSearchParams({ ...paginationState() });
  });

  const columns = () =>
    props.onRowSelection
      ? [
          {
            id: "select",
            header: ({ table }) => (
              <Checkbox
                indeterminate={table.getIsSomePageRowsSelected()}
                checked={table.getIsAllPageRowsSelected()}
                onChange={(value) => table.toggleAllPageRowsSelected(!!value)}
                aria-label="Select all"
              />
            ),
            cell: ({ row }) => (
              <Checkbox
                checked={row.getIsSelected()}
                onChange={(value) => {
                  const cb = props.onRowSelection!;
                  cb(row.index, row.original, value);
                  row.toggleSelected(value);
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
        ]
      : local.columns();

  const table = createMemo(() =>
    createSolidTable({
      get data() {
        return local.data() || [];
      },
      state: {
        pagination: paginationState(),
        rowSelection: rowSelection(),
      },
      columns: columns(),
      getCoreRowModel: getCoreRowModel(),

      // NOTE: requires setting up the header cells with resize handles.
      // enableColumnResizing: true,
      // columnResizeMode: 'onChange',

      // pagination {
      manualPagination: paginationEnabled,
      onPaginationChange: (state) => {
        setPaginationState(state);
        const handler = props.onPaginationChange;
        if (handler) {
          handler(state);
        }
      },
      rowCount: props.rowCount,
      // } pagination

      // Just means, the input data is already filtered.
      manualFiltering: true,

      // If set to true, pagination will be reset to the first page when page-altering state changes
      // eg. data is updated, filters change, grouping changes, etc.
      //
      // NOTE: In our current setup this causes infinite reload cycles when paginating.
      autoResetPageIndex: false,

      enableMultiRowSelection: props.onRowSelection ? true : false,
      onRowSelectionChange: setRowSelection,
    }),
  );

  return (
    <>
      {paginationEnabled && (
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
              when={table().getRowModel().rows?.length}
              fallback={
                <TableRow>
                  <TableCell colSpan={local.columns.length}>
                    <span>No results.</span>
                  </TableCell>
                </TableRow>
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
                            <div class="max-h-[80px] overflow-y-auto">
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
        {paginationEnabled && (
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
    <div class="flex items-center space-x-2">
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
        </div>

        <div class="flex items-center justify-center whitespace-nowrap text-sm font-medium">
          <PaginationInfoText />
        </div>
      </div>

      <PerPage />
    </div>
  );
}
