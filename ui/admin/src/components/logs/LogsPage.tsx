import {
  Match,
  Switch,
  createEffect,
  createResource,
  createSignal,
  onCleanup,
} from "solid-js";
import { useSearchParams } from "@solidjs/router";
import {
  type ColumnDef,
  createColumnHelper,
  type PaginationState,
} from "@tanstack/solid-table";
import { Chart } from "chart.js/auto";
import type {
  ChartData,
  ScriptableLineSegmentContext,
  TooltipItem,
} from "chart.js/auto";
import { TbRefresh } from "solid-icons/tb";

import { Separator } from "@/components/ui/separator";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { DataTable, defaultPaginationState } from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";
import type { LogJson, ListLogsResponse, Stats } from "@/lib/bindings";
import { adminFetch } from "@/lib/fetch";

const columnHelper = createColumnHelper<LogJson>();

const columns: ColumnDef<LogJson>[] = [
  columnHelper.display({
    header: "Created",
    cell: (ctx) => {
      const timestamp = new Date(ctx.row.original.created * 1000);
      return (
        <Tooltip>
          <TooltipTrigger>{timestamp.toUTCString()}</TooltipTrigger>

          <TooltipContent>
            <span>{timestamp.toLocaleString()} (Local)</span>
          </TooltipContent>
        </Tooltip>
      );
    },
  }),
  {
    accessorKey: "type",
    cell: (ctx) => {
      const type = ctx.row.original.type;
      if (type === 2) {
        return "HTTP";
      } else if (type === 1) {
        return "Admin API";
      } else if (type === 3) {
        return "Record API";
      }
      return type;
    },
  },
  columnHelper.display({
    header: "Level",
    cell: (ctx) => <>{levelToName.get(ctx.row.original.level)}</>,
  }),
  { accessorKey: "status" },
  { accessorKey: "method" },
  { accessorKey: "url" },
  {
    accessorKey: "latency_ms",
    header: "Latency (ms)",
  },
  { accessorKey: "client_ip" },
  { accessorKey: "referer" },
  {
    accessorKey: "user_agent",
    cell: (ctx) => {
      return (
        <Tooltip>
          <TooltipTrigger>
            <div class="text-left text-ellipsis line-clamp-2">
              {ctx.row.original.user_agent}
            </div>
          </TooltipTrigger>

          <TooltipContent>{ctx.row.original.user_agent}</TooltipContent>
        </Tooltip>
      );
    },
  },
  { accessorKey: "data" },
];

type GetLogsProps = {
  pagination: PaginationState;
  // Filter where clause to pass to the fetch.
  filter?: string;
  // Keep track of the timestamp cursor to have consistency for forwards and backwards pagination.
  cursors: string[];
};

// Value is the previous value in case this isn't the first fetch.
async function getLogs(
  source: GetLogsProps,
  { value }: { value: ListLogsResponse | undefined },
): Promise<ListLogsResponse> {
  const pageIndex = source.pagination.pageIndex;
  const limit = source.pagination.pageSize;
  const filter = source.filter ?? "";

  // Here we're setting the timestamp "cursor". If we're paging forward we add new cursors.
  // otherwise we're re-using previously seen cursors for consistency. We reset if we go back
  // to the start.
  const cursor = (() => {
    if (pageIndex === 0) {
      source.cursors.length = 0;
      return undefined;
    }

    const cursors = source.cursors;
    const index = pageIndex - 1;
    if (index < cursors.length) {
      return cursors[index];
    }

    // New page case.
    const cursor = value!.cursor;
    if (cursor) {
      cursors.push(cursor);
      return cursor;
    }
  })();

  const filterQuery = filter
    .split("AND")
    .map((frag) => frag.trim().replaceAll(" ", ""))
    .join("&");

  const params = new URLSearchParams(filterQuery);
  params.set("limit", limit.toString());
  if (cursor) {
    params.set("cursor", cursor);
  }

  console.debug("Fetching logs for ", params);
  const response = await adminFetch(`/logs?${params}`);
  return await response.json();
}

export function LogsPage() {
  const [searchParams, setSearchParams] = useSearchParams<{
    filter: string;
  }>();
  const [filter, setFilter] = createSignal<string | undefined>(
    searchParams.filter,
  );
  createEffect(() => {
    setSearchParams({ filter: filter() });
  });

  const [pagination, setPagination] = createSignal<PaginationState>(
    defaultPaginationState(),
  );
  const cursors: string[] = [];
  const getLogsProps = (): GetLogsProps => {
    return {
      pagination: pagination(),
      filter: filter(),
      cursors,
    };
  };
  const [logsFetch, { refetch }] = createResource(getLogsProps, getLogs);

  return (
    <>
      <div class="m-4 flex items-center gap-2">
        <h1 class="text-accent-600 m-0">Logs</h1>

        <button class="p-1 rounded hover:bg-gray-200" onClick={refetch}>
          <TbRefresh size={20} />
        </button>
      </div>

      <Separator />

      <div class="p-4 flex flex-col gap-8">
        <FilterBar
          onSubmit={(value: string) => {
            if (value === filter()) {
              refetch();
            } else {
              setFilter(value);
            }
          }}
          example='e.g. "latency[lt]=2 AND status=200"'
        />

        <Switch>
          <Match when={logsFetch.loading}>
            <p>Loading...</p>
          </Match>

          <Match when={logsFetch.error}>Error {`${logsFetch.error}`}</Match>

          <Match when={!logsFetch.error}>
            {pagination().pageIndex === 0 && (
              <LogsChart stats={logsFetch()!.stats!} />
            )}

            <DataTable
              columns={() => columns}
              data={() => logsFetch()?.entries}
              rowCount={Number(logsFetch()?.total_row_count ?? -1)}
              onPaginationChange={setPagination}
              initialPagination={pagination()}
            />
          </Match>
        </Switch>
      </div>
    </>
  );
}

function changeDistantPointLineColorToTransparent(
  ctx: ScriptableLineSegmentContext,
) {
  const secondsApart = Math.abs(ctx.p0.parsed.x - ctx.p1.parsed.x) / 1000;
  if (secondsApart > 1200) {
    return "transparent";
  }
  return undefined;
}

function LogsChart(props: { stats?: Stats }) {
  const stats = props.stats;
  if (!stats) {
    return null;
  }

  const data = (): ChartData | undefined => {
    const s = stats;
    if (!s) return;

    const labels = s.rate.map(([ts, _v]) => Number(ts) * 1000);
    const data = s.rate.map(([_ts, v]) => v);

    return {
      labels,
      datasets: [
        {
          data,
          label: "Rate",
          showLine: true,
          fill: false,
          segment: {
            borderColor: changeDistantPointLineColorToTransparent,
          },
          spanGaps: true,
        },
      ],
    };
  };

  let ref: HTMLCanvasElement | undefined;
  let chart: Chart | undefined;

  onCleanup(() => chart?.destroy());
  createEffect(() => {
    if (chart) {
      chart.destroy();
    }

    const d = data();
    if (d) {
      chart = new Chart(ref!, {
        type: "scatter",
        data: d,
        options: {
          // animation: false,
          maintainAspectRatio: false,
          scales: {
            x: {
              ticks: {
                callback: (value: number | string) => {
                  return new Date(value).toLocaleTimeString();
                },
              },
            },
          },
          plugins: {
            legend: {
              display: false, // This hides all text in the legend and also the labels.
            },
            // https://www.chartjs.org/docs/latest/configuration/tooltip.html
            tooltip: {
              enabled: true,
              callbacks: {
                title: (items: TooltipItem<"scatter">[]) => {
                  return items.map((item) => {
                    const ts = new Date(item.parsed.x);
                    return ts.toUTCString();
                  });
                },
                label: (item: TooltipItem<"scatter">) => {
                  return `rate: ${item.parsed.y.toPrecision(2)}/s`;
                },
              },
            },
          },
        },
      });
    }
  });

  return (
    <div class="h-[300px]">
      <canvas ref={ref}></canvas>
    </div>
  );
}

const logLevels: Array<[number, string]> = [
  [4, "TRACE"],
  [3, "DEBUG"],
  [2, "INFO"],
  [1, "WARN"],
  [0, "ERROR"],
] as const;

const levelToName: Map<number, string> = new Map(logLevels);

export default LogsPage;
