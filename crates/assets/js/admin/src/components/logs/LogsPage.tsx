import { Show, createMemo, createSignal, onCleanup } from "solid-js";
import { useSearchParams } from "@solidjs/router";
import type {
  ColumnDef,
  PaginationState,
  SortingState,
} from "@tanstack/solid-table";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineRefresh } from "solid-icons/tb";
import { Header } from "@/components/Header";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Callout, CalloutContent, CalloutTitle } from "@/components/ui/callout";
import { Table, buildTable } from "@/components/Table";
import type { Updater } from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";
import { fetchLogs, fetchStats } from "@/lib/api/logs";
import { safeParseInt } from "@/lib/utils";
import { formatSortingAsOrder } from "@/lib/list";
import {
  formatLogLatency,
  formatLogTimestamp,
  logClientLabel,
  logStatusTone,
} from "./logs";
import { LogsInsights } from "./LogsInsights";
import { LogDetailsSheet } from "./LogDetailsSheet";
import type { LogJson } from "@bindings/LogJson";

type SearchParams = {
  filter?: string;
  pageSize?: string;
  pageIndex?: string;
  order?: string;
};

const columns: ColumnDef<LogJson>[] = [
  {
    header: "Time",
    accessorKey: "created",
    size: 150,
    cell: (ctx) => {
      const t = formatLogTimestamp(ctx.row.original.created);
      return (
        <time datetime={t.iso} title={t.iso}>
          <span>
            {t.date} {t.time}
          </span>
        </time>
      );
    },
  },
  {
    header: "Status",
    accessorKey: "status",
    size: 80,
    cell: (ctx) => (
      <Badge
        variant={(() => {
          const tone = logStatusTone(ctx.row.original.status);
          return tone === "destructive"
            ? "error"
            : tone === "muted"
              ? "secondary"
              : tone;
        })()}
      >
        {String(ctx.row.original.status)}
      </Badge>
    ),
  },
  {
    header: "Method",
    accessorKey: "method",
    size: 90,
    cell: (ctx) => <Badge variant="outline">{ctx.row.original.method}</Badge>,
  },
  {
    header: "Request",
    accessorKey: "url",
    size: 340,
    cell: (ctx) => (
      <span class="font-medium select-text">{ctx.row.original.url}</span>
    ),
  },
  {
    header: "Latency",
    id: "latency",
    accessorKey: "latency_ms",
    size: 100,
    cell: (ctx) => formatLogLatency(ctx.row.original.latency_ms),
  },
  {
    header: "Client",
    accessorKey: "client_ip",
    size: 180,
    cell: (ctx) => {
      const l = ctx.row.original;
      const label = logClientLabel(l);
      return (
        <span>
          <span>{label}</span>
          <Show when={label !== l.client_ip}>
            <small class="text-muted-foreground block">{l.client_ip}</small>
          </Show>
        </span>
      );
    },
  },
  {
    header: "User",
    accessorKey: "user_id",
    size: 180,
    cell: (ctx) => ctx.row.original.user_id ?? "—",
  },
];

function LogsPage() {
  const [searchParams, setSearchParams] = useSearchParams<SearchParams>();
  const filter = createMemo(() => searchParams.filter);
  const pageSize = createMemo(() => safeParseInt(searchParams.pageSize) ?? 20);
  const pageIndex = createMemo(() => safeParseInt(searchParams.pageIndex) ?? 0);
  const order = createMemo(() => searchParams.order ?? "");
  const [sorting, setSortingImpl] = createSignal<SortingState>([]);
  const [cursors, setCursors] = createSignal(new Map<number, string>());
  let cursorKey = "";
  const resetCursors = () => {
    const key = `${pageSize()}|${filter() ?? ""}|${order()}`;
    if (key !== cursorKey) {
      cursorKey = key;
      setCursors(new Map());
    }
  };
  const pagination = (): PaginationState => ({
    pageIndex: pageIndex(),
    pageSize: pageSize(),
  });
  const setPagination = (p: PaginationState) =>
    setSearchParams({
      pageIndex: p.pageIndex || undefined,
      pageSize: p.pageSize === 20 ? undefined : p.pageSize,
      filter: filter(),
      order: order() || undefined,
    });
  const setFilter = (value: string | undefined) => {
    setCursors(new Map());
    setSearchParams({
      filter: value || undefined,
      pageIndex: undefined,
      pageSize: undefined,
      order: order() || undefined,
    });
  };
  const setSorting = (value: Updater<SortingState>) => {
    const next = typeof value === "function" ? value(sorting()) : value;
    setSortingImpl(next);
    setCursors(new Map());
    setSearchParams({
      filter: filter(),
      pageIndex: undefined,
      pageSize: undefined,
      order: formatSortingAsOrder(next) || undefined,
    });
  };

  const logsFetch = useQuery(() => {
    resetCursors();
    const size = pageSize();
    const index = pageIndex();
    const currentFilter = filter() ?? "";
    const currentOrder = order();
    return {
      queryKey: ["logs", size, index, currentFilter, currentOrder],
      queryFn: async () => {
        const response = await fetchLogs(
          size,
          index,
          currentFilter || undefined,
          cursors().get(index - 1),
          currentOrder || undefined,
        );
        if (response.cursor)
          setCursors((old) => new Map(old).set(index, response.cursor!));
        return response;
      },
    };
  });
  const statsFetch = useQuery(() => ({
    queryKey: ["log-stats", filter() ?? ""],
    queryFn: () => fetchStats(filter() || undefined),
  }));
  const logsData = createMemo(() => {
    try {
      return logsFetch.data;
    } catch {
      return undefined;
    }
  });
  const statsData = createMemo(() => {
    try {
      return statsFetch.data;
    } catch {
      return undefined;
    }
  });
  const refresh = async () => {
    await Promise.all([logsFetch.refetch(), statsFetch.refetch()]);
  };
  const busy = () =>
    logsFetch.isFetching ||
    logsFetch.isRefetching ||
    statsFetch.isFetching ||
    statsFetch.isRefetching;
  const [selected, setSelected] = createSignal<LogJson>();
  let trigger: HTMLTableRowElement | undefined;
  const closeDetails = () => {
    setSelected(undefined);
    queueMicrotask(() => trigger?.isConnected && trigger.focus());
  };
  onCleanup(() => {
    trigger = undefined;
  });
  const table = createMemo(() =>
    buildTable(
      {
        columns,
        data: logsData()?.entries ?? [],
        rowCount: Number(logsData()?.total_row_count ?? -1),
        pagination: pagination(),
        onPaginationChange: setPagination,
      },
      {
        manualSorting: true,
        state: { sorting: sorting() },
        onSortingChange: setSorting,
      },
    ),
  );
  const emptyState = () =>
    filter() ? (
      <div class="space-y-2">
        <p>No requests match</p>
        <Button variant="link" onClick={() => setFilter(undefined)}>
          Clear filter
        </Button>
      </div>
    ) : (
      <span>No requests have been recorded</span>
    );

  return (
    <div class="h-full">
      <Header
        title="Logs"
        description={
          <Show when={filter()}>
            <span>{String(logsData()?.total_row_count ?? 0)} requests</span>
          </Show>
        }
        right={
          <Button
            variant="outline"
            onClick={() => void refresh()}
            disabled={busy()}
            aria-label="Refresh logs"
          >
            <TbOutlineRefresh />
            {busy() ? "Refreshing…" : "Refresh logs"}
          </Button>
        }
      />
      <div class="flex flex-col gap-4 p-4">
        <LogsInsights
          rates={statsData()?.rates ?? []}
          countryCodes={statsData()?.country_codes ?? null}
          loading={statsFetch.isLoading}
          error={statsFetch.error}
          onRetry={() => statsFetch.refetch()}
        />
        <FilterBar
          initial={filter()}
          onSubmit={(value) =>
            value === (filter() ?? "") ? void refresh() : setFilter(value)
          }
          placeholder={
            "Filter, e.g.: '(latency > 2 || status >= 400) && method = \"GET\"'"
          }
        />
        <details>
          <summary class="cursor-pointer text-sm font-medium">
            Filter syntax
          </summary>
          <p class="text-muted-foreground mt-2 text-sm">
            Examples: <code>status &gt;= 400</code>, <code>method = "GET"</code>
            , <code>latency &gt; 2</code>, <code>url contains "/api"</code>,{" "}
            <code>client_ip = "127.0.0.1"</code>,{" "}
            <code>user_id = "user_123"</code>.
          </p>
        </details>
        <Show when={logsFetch.error}>
          <Callout variant="error">
            <CalloutTitle>Unable to load requests</CalloutTitle>
            <CalloutContent>
              <Button variant="outline" onClick={() => logsFetch.refetch()}>
                Retry
              </Button>
            </CalloutContent>
          </Callout>
        </Show>
        <div class="min-w-0 overflow-x-auto">
          <Table
            table={table()}
            loading={logsFetch.isLoading}
            dense
            paginationPosition="top"
            emptyState={emptyState()}
            onRowClick={(_, row, el) => {
              trigger = el;
              setSelected(row);
            }}
          />
        </div>
      </div>
      <LogDetailsSheet log={selected()} onClose={closeDetails} />
    </div>
  );
}
export default LogsPage;
