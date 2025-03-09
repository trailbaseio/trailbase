import {
  For,
  Match,
  Switch,
  createEffect,
  createResource,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { useSearchParams } from "@solidjs/router";
import { createColumnHelper } from "@tanstack/solid-table";
import type { ColumnDef, PaginationState } from "@tanstack/solid-table";
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

import { Button } from "@/components/ui/button";
import { Header } from "@/components/Header";
import { IconButton } from "@/components/IconButton";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { DataTable, defaultPaginationState } from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";

import { getLogs, type GetLogsProps } from "@/lib/logs";

import countriesGeoJSON from "@/assets/countries-110m.json";

import type { LogJson } from "@bindings/LogJson";
import type { Stats } from "@bindings/Stats";

const columnHelper = createColumnHelper<LogJson>();

const columns: ColumnDef<LogJson>[] = [
  columnHelper.display({
    header: "Created",
    cell: (ctx) => {
      const timestamp = new Date(ctx.row.original.created * 1000);
      return (
        <Tooltip>
          <TooltipTrigger as="div">
            <div class="w-[128px]">{timestamp.toUTCString()}</div>
          </TooltipTrigger>

          <TooltipContent>{timestamp.toLocaleString()} (Local)</TooltipContent>
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
            <div class="line-clamp-2 text-ellipsis text-left">
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

// Value is the previous value in case this isn't the first fetch.
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
      ...pagination(),
      filter: filter(),
      cursors,
    };
  };
  const [logsFetch, { refetch }] = createResource(getLogsProps, getLogs);
  const [showMap, setShowMap] = createSignal(true);
  const [showGeoipDialog, setShowGeoipDialog] = createSignal(false);

  return (
    <div class="h-dvh overflow-y-auto">
      <Header
        title="Logs"
        left={
          <IconButton onClick={refetch} tooltip="Refresh Logs">
            <TbRefresh size={18} />
          </IconButton>
        }
        right={
          pagination()?.pageIndex === 0 && (
            <Dialog
              modal={true}
              open={showGeoipDialog()}
              onOpenChange={setShowGeoipDialog}
            >
              <DialogContent>
                <DialogTitle>Geoip Database</DialogTitle>
                <p>
                  TrailBase did not report any geo information for your logs. To
                  enable this feature, place a GeoIP database in MaxMind format
                  under:
                </p>
                <span class="ml-4 font-mono">
                  {"<traildepot>/GeoLite2-Country.mmdb"}
                </span>
                .
                <DialogFooter>
                  <Button>Got it</Button>
                </DialogFooter>
              </DialogContent>

              <IconButton
                disabled={logsFetch.state !== "ready"}
                onClick={() => {
                  if (logsFetch()?.stats?.country_codes) {
                    setShowMap((v) => !v);
                  } else {
                    setShowGeoipDialog((v) => !v);
                  }
                }}
                tooltip="Toggle World Map"
              >
                <TbWorld size={20} />
              </IconButton>
            </Dialog>
          )
        }
      />

      <div class="flex flex-col gap-4 p-4">
        <Switch fallback={<p>Loading...</p>}>
          <Match when={logsFetch.error}>Error {`${logsFetch.error}`}</Match>

          <Match when={logsFetch.state === "ready"}>
            {pagination().pageIndex === 0 && logsFetch()!.stats && (
              <div class="mb-4 flex h-[300px] w-full gap-4">
                <div class={showMap() ? "w-1/2 grow" : "w-full"}>
                  <LogsChart stats={logsFetch()!.stats!} />
                </div>

                {showMap() && logsFetch()!.stats?.country_codes && (
                  <div class="flex w-1/2 max-w-[500px] items-center">
                    <WorldMap
                      country_codes={logsFetch()!.stats!.country_codes!}
                    />
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
    </div>
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

function getColor(d: number) {
  if (d > 1000) {
    return "#800026";
  } else if (d > 500) {
    return "#BD0026";
  } else if (d > 200) {
    return "#E31A1C";
  } else if (d > 100) {
    return "#FC4E2A";
  } else if (d > 50) {
    return "#FD8D3C";
  } else if (d > 20) {
    return "#FEB24C";
  } else if (d > 10) {
    return "#FED976";
  } else if (d > 0) {
    return "#FFEDA0";
  } else {
    return "#FFFFFF";
  }
}

function mapStyle(
  codes: { [key in string]?: number },
  feature: Feature | undefined,
) {
  if (!feature) return {};

  return {
    fillColor: getColor(
      codes[numericToAlpha2(feature.id as string) ?? ""] ?? 0,
    ),
    weight: 2,
    opacity: 1,
    color: "white",
    dashArray: "3",
    fillOpacity: 0.6,
  };
}

const Legend = L.Control.extend({
  options: {
    position: "bottomright",
  },
  onAdd: (_map: L.Map) => {
    const grades = [1, 20, 50, 100, 200, 500, 1000];
    return (
      <div class="flex flex-col rounded bg-white/70 p-1">
        <For each={grades}>
          {(grade: number, index: () => number) => {
            const label = () => {
              const next = grades[index() + 1];
              return next ? ` ${grade} - ${next}` : ` ${grade}+`;
            };

            return (
              <div class="flex">
                <div
                  class="mr-1 px-2 py-1"
                  style={{ background: getColor(grade) }}
                />
                {label()}
              </div>
            );
          }}
        </For>
      </div>
    );
  },
});

function WorldMap(props: { country_codes: { [key in string]?: number } }) {
  const codes = () => props.country_codes;

  let ref: HTMLDivElement | undefined;
  let map: L.Map | undefined;

  const destroy = () => {
    if (map) {
      map.off();
      map.remove();
    }
  };

  onCleanup(destroy);
  onMount(() => {
    destroy();

    const m = (map = L.map(ref!).setView([30, 0], 1.4));
    m.attributionControl.setPrefix("");

    L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
      noWrap: true,
      maxZoom: 19,
      attribution:
        '&copy; <a href="http://www.openstreetmap.org/copyright">OpenStreetMap</a>',
    }).addTo(m);

    // control that shows state info on hover
    const CustomControl = L.Control.extend({
      onAdd: (_map: L.Map) => {
        return <div class="rounded bg-white/70 p-2">Hover over a country</div>;
      },
      update: function (props?: Props) {
        /* eslint-disable solid/reactivity */
        const id = props?.id;
        const requests = codes()[numericToAlpha2(id ?? "") ?? ""] ?? 0;
        const contents = props
          ? `<b>${props.name}</b><br />${requests} req`
          : "Hover over a country";

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (this as any)._container.innerHTML = contents;
      },
    });

    const info = new CustomControl().addTo(m);
    new Legend().addTo(m);

    type Props = {
      id: string;
      name: string;
    };

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
        style: (map) => mapStyle(codes(), map),
        onEachFeature,
      },
    ).addTo(m);
  });

  return (
    <div
      class="h-[280px] w-full rounded"
      style={{ "background-color": "transparent" }}
      ref={ref}
    />
  );
}

function LogsChart(props: { stats: Stats }) {
  const stats = props.stats;

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
      <canvas ref={ref} />
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
