import { Match, Switch, Show, createSignal, createMemo } from "solid-js";
import { useSearchParams } from "@solidjs/router";
import type {
  ColumnDef,
  PaginationState,
  SortingState,
} from "@tanstack/solid-table";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineRefresh } from "solid-icons/tb";

import { Header } from "@/components/Header";
import { IconButton } from "@/components/IconButton";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Table, buildTable } from "@/components/Table";
import type { Updater } from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";

import { fetchLogs, fetchStats } from "@/lib/api/logs";
import { copyToClipboard, safeParseInt } from "@/lib/utils";
import { formatSortingAsOrder } from "@/lib/list";
import { LogsInsights } from "./LogsInsights";

import type { LogJson } from "@bindings/LogJson";

const columns: ColumnDef<LogJson>[] = [
  // NOTE: ISO string contains milliseconds.
  {
    header: "created",
    accessorKey: "created",
    size: 120,
    cell: (ctx) => {
      const secondsSinceEpoch = ctx.row.original.created;
      const timestamp = new Date(secondsSinceEpoch * 1000);
      return (
        <div class="flex items-center">
          <Tooltip>
            <TooltipTrigger as="div">
              {timestamp.toISOString().replace(/T/, " ")}
            </TooltipTrigger>

            <TooltipContent>
              <p>
                {timestamp.toLocaleString(undefined, {
                  timeZoneName: "short",
                  hour12: false,
                })}
              </p>
              <p>{secondsSinceEpoch.toFixed(0)}s since epoch</p>
            </TooltipContent>
          </Tooltip>
        </div>
      );
    },
  },
  {
    accessorKey: "status",
    size: 60,
  },
  {
    accessorKey: "method",
    size: 80,
  },
  {
    accessorKey: "url",
    size: 340,
  },
  {
    // Used for sorting.
    id: "latency",
    header: "latency (ms)",
    // Used for accessing the request (there's a rename from latency in DB to latency_ms in response)
    accessorKey: "latency_ms",
    size: 80,
    cell: (ctx) => ctx.row.original.latency_ms.toFixed(6),
  },
  {
    accessorKey: "client_ip",
    size: 120,
  },
  {
    header: "GeoIP",
    enableSorting: false,
    cell: (ctx) => {
      const city = ctx.row.original.client_geoip_city;
      if (city) {
        return `${city.name} (${city.country_code})`;
      }
      return ctx.row.original.client_geoip_cc;
    },
    size: -1,
  },
  {
    accessorKey: "referer",
    size: 200,
  },
  {
    accessorKey: "user_agent",
    size: 200,
    cell: (ctx) => {
      return (
        <div class="flex items-center">
          <Tooltip>
            <TooltipTrigger>
              <div class="line-clamp-2 text-left text-ellipsis">
                {ctx.row.original.user_agent}
              </div>
            </TooltipTrigger>

            <TooltipContent>{ctx.row.original.user_agent}</TooltipContent>
          </Tooltip>
        </div>
      );
    },
  },
  {
    accessorKey: "user_id",
    size: 300,
    cell: (ctx) => {
      const userId = () => ctx.row.original.user_id;
      return (
        <Show when={userId()}>
          <div
            class="hover:text-muted-foreground"
            onClick={() => copyToClipboard(userId() ?? "")}
          >
            {userId()}
          </div>
        </Show>
      );
    },
  },
];

type SearchParams = {
  filter?: string;
  pageSize?: string;
  pageIndex?: string;
};

// Value is the previous value in case this isn't the first fetch.
function LogsPage() {
  // IMPORTANT: We need to memo the search params to treat absence and defaults
  // consistently, otherwise `undefined`->`default` may invalidate the cursors.
  const [searchParams, setSearchParams] = useSearchParams<SearchParams>();
  const filter = createMemo(() => searchParams.filter);
  const pageSize = createMemo(() => safeParseInt(searchParams.pageSize) ?? 20);
  const pageIndex = createMemo(() => safeParseInt(searchParams.pageIndex) ?? 0);

  const reset = () => {
    console.warn("resetting search params");
    setSearchParams({
      filter: undefined,
      pageSize: undefined,
      pageIndex: undefined,
    });
  };
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
      pageIndex: undefined,
      pageSize: undefined,
      filter,
    });
  };

  const [sorting, setSortingImpl] = createSignal<SortingState>([]);
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
    const _ = [pageSize(), filter(), sorting()];
    console.debug("resetting cursor");
    return new Map();
  });

  // NOTE: admin user endpoint doesn't support offset, we have to cursor through
  // and cannot just jump to page N.
  const logsFetch = useQuery(() => ({
    queryKey: [pagination(), filter(), sorting()],
    queryFn: async ({ queryKey }) => {
      console.debug("Fetching logs with key:", queryKey);

      try {
        const { pageSize, pageIndex } = pagination();
        const cursor = cursors().get(pageIndex - 1);

        const response = await fetchLogs(
          pageSize,
          pageIndex,
          filter(),
          cursor,
          formatSortingAsOrder(sorting()),
        );

        // Update cursors.
        if (sorting().length === 0 && response.cursor) {
          cursors().set(pageIndex, response.cursor);
        }

        return response;
      } catch (err) {
        reset();
        throw err;
      }
    },
  }));

  const statsFetch = useQuery(() => ({
    queryKey: [filter()],
    queryFn: async ({ queryKey }) => {
      try {
        console.debug("Fetching stats with key:", queryKey);
        return await fetchStats(filter());
      } catch (err) {
        reset();
        throw err;
      }
    },
  }));

  const refetch = () => {
    logsFetch.refetch();
    statsFetch.refetch();
  };

  const [columnPinningState, setColumnPinningState] = createSignal({});

  const logsTable = createMemo(() => {
    return buildTable(
      {
        columns,
        data: logsFetch.data?.entries ?? [],
        columnPinning: columnPinningState,
        onColumnPinningChange: setColumnPinningState,
        rowCount: Number(logsFetch.data?.total_row_count ?? -1),
        pagination: pagination(),
        onPaginationChange: setPagination,
      },
      {
        manualSorting: true,
        state: { sorting: sorting() },
        onSortingChange: setSorting,
      },
    );
  });

  return (
    <div class="h-full">
      <Header
        title="Logs"
        left={
          <IconButton onClick={refetch} tooltip="Refresh Logs">
            <TbOutlineRefresh />
          </IconButton>
        }
      />
      <div class="flex flex-col gap-4 p-4">
        <Switch>
          <Match when={logsFetch.error}>Error {`${logsFetch.error}`}</Match>
          <Match when={true}>
            <LogsInsights
              rates={statsFetch.data?.rates ?? []}
              countryCodes={statsFetch.data?.country_codes ?? null}
              loading={statsFetch.isLoading}
              error={statsFetch.error}
              onRetry={() => statsFetch.refetch()}
            />
            <FilterBar
              initial={filter()}
              onSubmit={(value: string) => {
                if (value === filter()) {
                  refetch();
                } else {
                  setFilter(value);
                }
              }}
              placeholder={`Filter, e.g.: '(latency > 2 || status >= 400) && method = "GET"'`}
            />

            <Table table={logsTable()} loading={logsFetch.isLoading} />
          </Match>
        </Switch>
      </div>
    </div>
  );
}

// Needed for dynamic load.
export default LogsPage;
