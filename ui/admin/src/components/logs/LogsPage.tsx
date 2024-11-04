import {
  Match,
  Switch,
  createEffect,
  createResource,
  createSignal,
  onCleanup,
  onMount,
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
import { TbRefresh, TbWorld } from "solid-icons/tb";
import { numericToAlpha2 } from "i18n-iso-countries";
import type { FeatureCollection, Feature } from "geojson";
import "leaflet/dist/leaflet.css";
import * as L from "leaflet";

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

import countriesGeoJSON from "@/assets/countries-110m.json";

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
  const [showMap, setShowMap] = createSignal(true);

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
        <Switch fallback={<p>Loading...</p>}>
          <Match when={logsFetch.error}>Error {`${logsFetch.error}`}</Match>

          <Match when={!logsFetch.loading}>
            {/*
              {showMap() && }
            */}

            {pagination().pageIndex === 0 && (
              <div class="flex w-full h-[300px]">
                <div class={showMap() ? "w-1/2" : "w-full"}>
                  <LogsChart stats={logsFetch()!.stats} />
                </div>

                {showMap() && (
                  <div class="w-1/2 flex items-center">
                    <WorldChart stats={logsFetch()!.stats} />
                  </div>
                )}
              </div>
            )}

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

function WorldChart(props: { stats: Stats | null }) {
  const stats = props.stats;
  if (!stats) {
    return null;
  }

  const codes = stats.country_codes;
  // if (Object.keys(codes).length <= 1) {
  //   return null;
  // }

  let ref: HTMLDivElement | undefined;
  let map: L.Map | undefined;

  const destroy = () => {
    if (map) {
      map.off();
      map.remove();
    }
  };

  function getColor(d: number) {
    return d > 1000
      ? "#800026"
      : d > 500
        ? "#BD0026"
        : d > 200
          ? "#E31A1C"
          : d > 100
            ? "#FC4E2A"
            : d > 50
              ? "#FD8D3C"
              : d > 20
                ? "#FEB24C"
                : d > 10
                  ? "#FED976"
                  : "#FFEDA0";
  }

  function mapStyle(feature: Feature | undefined) {
    if (!feature) return {};

    return {
      fillColor: getColor(
        codes[numericToAlpha2(feature.id as string) ?? ""] ?? 0,
      ),
      weight: 2,
      opacity: 1,
      color: "white",
      dashArray: "3",
      fillOpacity: 0.7,
    };
  }

  onCleanup(destroy);
  onMount(() => {
    destroy();

    const m = (map = L.map(ref!, {}).setView([30, 0], 1.4));

    L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
      noWrap: true,
      maxZoom: 19,
      attribution:
        '&copy; <a href="http://www.openstreetmap.org/copyright">OpenStreetMap</a>',
    }).addTo(m);

    // control that shows state info on hover
    const CustomControl = L.Control.extend({
      onAdd: (_map: L.Map) => {
        return L.DomUtil.create("div", "info");
      },
      update: function (props?: Props) {
        const id = props?.id;
        const requests = codes[numericToAlpha2(id ?? "") ?? ""] ?? 0;
        const contents = props
          ? `<b>${props.name}</b><br />${requests} req`
          : "Hover over a country";

        (this as any)._container.innerHTML = `<h4>Requests</h4>${contents}`;
      },
    });

    const info = new CustomControl();

    type Props = {
      id: string;
      name: string;
    };

    info.addTo(m);

    const highlightFeature = (e: L.LeafletMouseEvent) => {
      const layer = e.target;

      layer.setStyle({
        weight: 2,
        color: "#666",
        dashArray: "",
        fillOpacity: 0.7,
      });

      layer.bringToFront();

      info.update({
        id: layer.feature.id,
        name: layer.feature.properties.name,
      } as Props);
    };

    function onEachFeature(_feature: Feature, layer: L.Layer) {
      layer.on({
        mouseover: highlightFeature,
        mouseout: (e: L.LeafletMouseEvent) => {
          geojson.resetStyle(e.target);
          info.update();
        },
        click: (e: L.LeafletMouseEvent) => m.fitBounds(e.target.getBounds()),
      });
    }

    const geojson = L.geoJson(
      (countriesGeoJSON as FeatureCollection).features,
      {
        style: mapStyle,
        onEachFeature,
      },
    ).addTo(m);
  });

  return <div class="rounded-xl w-full h-[280px]" ref={ref} />;
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
