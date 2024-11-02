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
  ChartConfiguration,
  ChartData,
  ScriptableLineSegmentContext,
  TooltipItem,
} from "chart.js/auto";
import {
  ChoroplethChart,
  ChoroplethController,
  ProjectionScale,
  ColorScale,
  GeoFeature,
  topojson,
} from "chartjs-chart-geo";
import type { Feature } from "chartjs-chart-geo";
import { TbRefresh, TbWorld } from "solid-icons/tb";
import type { FeatureCollection, GeoJsonProperties } from "geojson";
import countries50m from "world-atlas/countries-50m.json";
import type { GeometryCollection, Topology } from "topojson-specification";
import { numericToAlpha2 } from "i18n-iso-countries";

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

Chart.register(
  ChoroplethChart,
  ChoroplethController,
  ProjectionScale,
  ColorScale,
  GeoFeature,
);

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
  {
    accessorKey: "client_cc",
    header: "Country Code",
  },
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
  const [showMap, setShowMap] = createSignal(false);

  return (
    <>
      <div class="m-4 flex justify-between items-center gap-2">
        <div class="flex items-center gap-2">
          <h1 class="text-accent-600 m-0">Logs</h1>

          <button class="p-1 rounded hover:bg-gray-200" onClick={refetch}>
            <TbRefresh size={20} />
          </button>
        </div>

        <button
          class="p-1 rounded hover:bg-gray-200"
          onClick={() => setShowMap(!showMap())}
        >
          <TbWorld size={20} />
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
            {showMap() && <WorldChart stats={logsFetch()!.stats} />}

            {pagination().pageIndex === 0 && (
              <LogsChart stats={logsFetch()!.stats} />
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

const x = countries50m.objects
  .countries as GeometryCollection<GeoJsonProperties>;
const collection: FeatureCollection = topojson.feature(
  countries50m as unknown as Topology,
  x,
) as unknown as FeatureCollection;
const countries: Feature = collection.features;

function WorldChart(props: { stats: Stats | null }) {
  const stats = props.stats;
  if (!stats) {
    return null;
  }

  const codes = stats.country_codes;
  // if (Object.keys(codes).length <= 1) {
  //   return null;
  // }

  let ref: HTMLCanvasElement | undefined;
  let chart: Chart | undefined;

  onCleanup(() => chart?.destroy());
  createEffect(() => {
    if (chart) {
      chart.destroy();
    }

    const data: ChartConfiguration<"choropleth">["data"] = {
      labels: countries.map((d: any) => d.properties.name),
      datasets: [
        {
          label: "Countries",
          data: countries.map((d: any) => {
            let value = 0;
            const id: string | undefined = d.id;
            if (id) {
              const cc = numericToAlpha2(id);
              if (cc) {
                value = codes[cc] ?? 0;
              }
            }

            return {
              feature: d,
              value,
            };
          }),
        },
      ],
    };

    chart = new Chart<"choropleth">(ref!, {
      type: "choropleth",
      data,
      options: {
        showOutline: true,
        showGraticule: true,
        scales: {
          projection: {
            axis: "x",
            projection: "equalEarth",
          },
        },
        plugins: {
          legend: {
            display: false,
          },
        },
        onClick: (_evt, _elems) => {
          // console.log(elems.map((elem) => elem.element.feature.properties.name));
        },
      },
    });
  });

  return (
    <div class="h-[300px] w-[600px]">
      <canvas ref={ref}></canvas>
    </div>
  );
}

function LogsChart(props: { stats: Stats | null }) {
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
