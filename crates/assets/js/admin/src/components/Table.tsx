import { Index, For, Match, Show, Switch } from "solid-js";
import type { Accessor } from "solid-js";
import {
  flexRender,
  createSolidTable,
  getCoreRowModel,
  createColumnHelper,
} from "@tanstack/solid-table";
import type {
  ColumnDef,
  ColumnPinningState,
  Header,
  PaginationState,
  Row,
  Table as SolidTable,
  TableOptions as SolidTableOptions,
  SortingState,
  Updater,
} from "@tanstack/solid-table";
import { TbPin, TbPinFilled, TbCaretDown, TbCaretUp } from "solid-icons/tb";

import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table as ShadcnTable,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Checkbox } from "@/components/ui/checkbox";
import { Skeleton } from "@/components/ui/skeleton";
import { createIsMobile } from "@/lib/signals";

type TableOptions<TData, TValue> = {
  data: TData[] | undefined;
  columns: ColumnDef<TData, TValue>[];

  rowCount?: number;
  pagination?: PaginationState;
  onPaginationChange?: (state: PaginationState) => void;

  onRowSelection?: (rows: Row<TData>[], value: boolean) => void;

  columnPinning?: Accessor<ColumnPinningState>;
  onColumnPinningChange?: (state: ColumnPinningState) => void;
};

export function buildTable<TData, TValue>(
  opts: TableOptions<TData, TValue>,
  overrides?: Partial<SolidTableOptions<TData>>,
) {
  console.debug("buildTable: ", opts);

  function buildColumns() {
    const onRowSelection = opts.onRowSelection;
    if (!onRowSelection) {
      return opts.columns;
    }

    const helper = createColumnHelper<TData>();

    return [
      helper.display({
        id: "__select__",
        enablePinning: true,
        enableHiding: false,
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
      }),
      // Custom/Domain-provided columns
      ...opts.columns,
    ];
  }

  function buildColumnPinningState(): ColumnPinningState {
    const state = {
      ...opts.columnPinning?.(),
    };
    if (state.left?.[0] !== "__select__") {
      state.left = ["__select__", ...(state.left ?? [])];
    }
    return state;
  }

  const enableColumnPinning =
    opts.columnPinning !== undefined && opts.columns.length > 1;

  const t = createSolidTable({
    data: opts.data ?? [],
    state: {
      pagination:
        opts.pagination !== undefined
          ? {
              pageIndex: opts.pagination.pageIndex ?? 0,
              pageSize: opts.pagination.pageSize ?? 20,
            }
          : undefined,
      // rowSelection: rowSelection(),
      columnPinning: buildColumnPinningState(),

      ...(overrides?.state ?? {}),
    },
    columns: buildColumns(),
    getCoreRowModel: getCoreRowModel(),

    // Column default sizing
    defaultColumn: {
      // We consider "-1" as use flex.
      size: -1,
      minSize: -1,
      maxSize: window.innerWidth / 2,
    },
    // TODO: Allow dynamic resizing.
    // enableColumnResizing: true,
    // columnResizeMode: 'onChange',

    // pagination:
    manualPagination: opts.onPaginationChange !== undefined,
    onPaginationChange:
      opts.onPaginationChange !== undefined
        ? (updater) => {
            const newState =
              typeof updater === "function"
                ? updater(t.getState().pagination)
                : updater;

            opts.onPaginationChange!(newState);
          }
        : undefined,
    // If set to true, pagination will be reset to the first page when page-altering state changes
    // eg. data is updated, filters change, grouping changes, etc.
    //
    // NOTE: In our current setup this causes infinite reload cycles when paginating.
    autoResetPageIndex: false,
    rowCount: opts.rowCount,

    // Just means, the input data is already filtered.
    manualFiltering: true,

    enableRowSelection: true,
    enableMultiRowSelection: opts.onRowSelection ? true : false,
    // onRowSelectionChange: setRowSelection,

    enableColumnPinning,
    onColumnPinningChange:
      opts.onColumnPinningChange !== undefined
        ? (updater) => {
            const newState =
              typeof updater === "function"
                ? updater(t.getState().columnPinning)
                : updater;

            opts.onColumnPinningChange!(newState);
          }
        : undefined,

    ...omit(overrides ?? {}, "state"),
  });

  return t;
}

function omit<T, K extends keyof T>(object: T, key: K): Omit<T, K> {
  const { [key]: _deletedKey, ...otherKeys } = object;
  return otherKeys;
}

export function Table<TData>(props: {
  table: SolidTable<TData>;
  loading: boolean;
  onRowClick?: (idx: number, row: TData) => void;
}) {
  const paginationEnabled = () => props.table.options.manualPagination ?? false;
  const paginationState = () => props.table.getState().pagination;
  const columns = () => props.table.options.columns;
  const numRows = (): number => props.table.getRowModel().rows.length;
  const enableSorting = () =>
    props.table.options.manualSorting || props.table.options.enableSorting;

  return (
    <div>
      <Show when={paginationEnabled()}>
        <PaginationControl
          table={props.table}
          rowCount={props.table.options.rowCount}
        />
      </Show>

      <div class="rounded-md border">
        <ShadcnTable>
          <TableHeader>
            <For each={props.table.getHeaderGroups()}>
              {(headerGroup) => (
                <TableRow>
                  <For each={headerGroup.headers}>
                    {(header) => (
                      <TableHeaderRow
                        header={header}
                        enabledColumnPinning={
                          props.table.options.enableColumnPinning ?? false
                        }
                        updateSorting={
                          enableSorting() ? props.table.setSorting : undefined
                        }
                      />
                    )}
                  </For>
                </TableRow>
              )}
            </For>
          </TableHeader>

          <TableBody>
            <Switch>
              <Match when={props.loading}>
                <Index each={Array(paginationState().pageSize)}>
                  {() => (
                    <TableRow>
                      <For each={props.table.getVisibleLeafColumns()}>
                        {(cell) => (
                          <TableCell>
                            <Switch>
                              <Match when={cell.id === "__select__"}>
                                <Checkbox />
                              </Match>

                              <Match when={true}>
                                <Skeleton
                                  height={16}
                                  width={cell.getSize()}
                                  radius={10}
                                />
                              </Match>
                            </Switch>
                          </TableCell>
                        )}
                      </For>
                    </TableRow>
                  )}
                </Index>
              </Match>

              <Match when={numRows() > 0}>
                <For each={props.table.getRowModel().rows}>
                  {(row) => (
                    <TableDataRow row={row} onRowClick={props.onRowClick} />
                  )}
                </For>
              </Match>

              <Match when={true}>
                <TableRow>
                  <TableCell colSpan={columns().length}>
                    <span>Empty</span>
                  </TableCell>
                </TableRow>
              </Match>
            </Switch>
          </TableBody>
        </ShadcnTable>
      </div>
    </div>
  );
}

function TableHeaderRow<TData>(props: {
  header: Header<TData, unknown>;
  enabledColumnPinning: boolean;
  updateSorting?: (updater: Updater<SortingState>) => void;
}) {
  const toggleSorting = () => {
    /* eslint-disable solid/reactivity */
    if (
      props.updateSorting === undefined ||
      !props.header.column.getCanSort()
    ) {
      return undefined;
    }

    return () => {
      props.updateSorting?.((old) => {
        const id = props.header.id;
        if (old.length === 0 || old[0].id !== id) {
          return [
            {
              id: props.header.id,
              desc: true,
            },
          ];
        }

        console.assert(old[0].id === id);

        switch (old[0].desc) {
          case true:
            return [
              {
                id: props.header.id,
                desc: false,
              },
            ];
          case false:
            return [];
        }
      });
    };
  };

  function HeadContents() {
    return (
      <Show when={!props.header.isPlaceholder}>
        {flexRender(
          props.header.column.columnDef.header,
          props.header.getContext(),
        )}
      </Show>
    );
  }

  return (
    <Switch>
      {/* Simple render for initial checkbox column. */}
      <Match when={props.header.column.columnDef.id === "__select__"}>
        <TableHead class={selectStyle}>
          <HeadContents />
        </TableHead>
      </Match>

      <Match when={true}>
        <TableHead class="relative pr-5 pl-4" onClick={toggleSorting()}>
          <HeadContents />

          {/* Sorting arrow */}
          <Switch>
            <Match when={props.header.column.getIsSorted() === "asc"}>
              <div class="absolute right-0 bottom-0 z-10">
                <TbCaretUp size={20} />
              </div>
            </Match>

            <Match when={props.header.column.getIsSorted() === "desc"}>
              <div class="absolute right-0 bottom-0 z-10">
                <TbCaretDown size={20} />
              </div>
            </Match>
          </Switch>

          {/* Pin Button */}
          <Show when={props.enabledColumnPinning}>
            <div class="absolute top-1 right-1 z-10">
              <Button
                class="size-4 bg-transparent"
                size="icon"
                variant="ghost"
                onClick={(ev) => {
                  ev.stopPropagation();

                  if (props.header.column.getIsPinned()) {
                    props.header.column.pin(false);
                  } else {
                    props.header.column.pin("left");
                  }
                }}
              >
                {props.header.column.getIsPinned() ? (
                  <TbPinFilled />
                ) : (
                  <TbPin />
                )}
              </Button>
            </div>
          </Show>
        </TableHead>
      </Match>
    </Switch>
  );
}

function TableDataRow<TData>(props: {
  row: Row<TData>;
  onRowClick?: (idx: number, row: TData) => void;
}) {
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
    handler(props.row.index, props.row.original);
  };

  return (
    <TableRow
      data-state={props.row.getIsSelected() && "selected"}
      onClick={onClick}
    >
      <For each={props.row.getVisibleCells()}>
        {(cell) => {
          const size = cell.column.getSize();
          const width = size > 0 ? `${size}px` : undefined;
          const style =
            cell.column.id == "__select__"
              ? selectStyle
              : "max-h-[80px] max-w-[50dvw] overflow-x-hidden overflow-y-auto break-words";

          return (
            <Switch>
              <Match when={width !== undefined}>
                <TableCell>
                  <div class={style} style={{ width }}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </div>
                </TableCell>
              </Match>

              <Match when={width === undefined}>
                <TableCell class={style}>
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </TableCell>
              </Match>
            </Switch>
          );
        }}
      </For>
    </TableRow>
  );
}

function PaginationControl<TData>(props: {
  table: SolidTable<TData>;
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
        <SelectTrigger class="h-8 w-18">
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

const selectStyle = "w-[40px] pl-4 pr-2";
