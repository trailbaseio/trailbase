import {
  For,
  Match,
  Switch,
  Show,
  createEffect,
  createSignal,
  onCleanup,
  onMount,
  createMemo,
} from "solid-js";
import { useSearchParams } from "@solidjs/router";
import type {
  ColumnDef,
  PaginationState,
  SortingState,
} from "@tanstack/solid-table";
import { useQuery } from "@tanstack/solid-query";
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
import { Table, buildTable } from "@/components/Table";
import type { Updater } from "@/components/Table";
import { FilterBar } from "@/components/FilterBar";

import { fetchLogs } from "@/lib/api/logs";
import { copyToClipboard, safeParseInt } from "@/lib/utils";
import { formatSortingAsOrder } from "@/lib/list";

import countriesGeoJSON from "@/assets/countries-110m.json";

import type { LogJson } from "@bindings/LogJson";
import type { Stats } from "@bindings/Stats";

const columns: ColumnDef<LogJson>[] = [
  // NOTE: ISO string contains milliseconds.
  {
    header: "Created",
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
            class="hover:text-gray-600"
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
export function LogsPage() {
  // IMPORTANT: We need to memo the search params to treat absence and defaults
  // consistently, otherwise `undefined`->`default` may invalidate the cursors.
  const [searchParams, setSearchParams] = useSearchParams<SearchParams>();
  const filter = createMemo(() => searchParams.filter);
  const pageSize = createMemo(() => safeParseInt(searchParams.pageSize) ?? 20);
  const pageIndex = createMemo(() => safeParseInt(searchParams.pageIndex) ?? 0);

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
      console.debug(`Fetching data with key: ${queryKey}`);

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
        // Reset.
        setSearchParams({
          filter: undefined,
          pageSize: undefined,
          pageIndex: undefined,
        });

        throw err;
      }
    },
  }));
  const refetch = () => logsFetch.refetch();

  const [showMap, setShowMap] = createSignal(true);
  const [showGeoipDialog, setShowGeoipDialog] = createSignal(false);
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
        state: {
          sorting: sorting(),
        },
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
            <TbRefresh />
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
                disabled={!logsFetch.isSuccess}
                onClick={() => {
                  if (logsFetch.data?.stats?.country_codes) {
                    setShowMap((v) => !v);
                  } else {
                    setShowGeoipDialog((v) => !v);
                  }
                }}
                tooltip="Toggle World Map"
              >
                <TbWorld />
              </IconButton>
            </Dialog>
          )
        }
      />

      <div class="flex flex-col gap-4 p-4">
        <Switch>
          <Match when={logsFetch.error}>Error {`${logsFetch.error}`}</Match>

          <Match when={true}>
            <Show when={pagination().pageIndex === 0}>
              <div class="mb-4 flex w-full flex-col gap-4 md:h-[300px] md:flex-row">
                <Show when={showMap()}>
                  <div class="flex items-center md:w-1/2 md:max-w-[500px]">
                    <WorldMap
                      country_codes={
                        logsFetch.data?.stats?.country_codes ?? null
                      }
                    />
                  </div>
                </Show>

                <div class={showMap() ? "md:w-1/2" : "w-full"}>
                  <LogsChart rates={logsFetch.data?.stats?.rate ?? []} />
                </div>
              </div>
            </Show>

            <FilterBar
              initial={filter()}
              onSubmit={(value: string) => {
                if (value === filter()) {
                  refetch();
                } else {
                  setFilter(value);
                }
              }}
              placeholder={`Filter Query, e.g. '(latency > 2 || status >= 400) && method = "GET"'`}
            />

            <Table table={logsTable()} loading={logsFetch.isLoading} />
          </Match>
        </Switch>
      </div>
    </div>
  );
}

function changeDistantPointLineColorToTransparent(
  ctx: ScriptableLineSegmentContext,
) {
  const t0 = ctx.p0.parsed.x;
  const t1 = ctx.p1.parsed.x;

  if (t0 !== null && t1 !== null) {
    const secondsApart = Math.abs(t0 - t1) / 1000;
    if (secondsApart > 1200) {
      return "transparent";
    }
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
      <div class="flex flex-col rounded-sm bg-white/70 p-1">
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

function WorldMap(props: { country_codes: Stats["country_codes"] }) {
  const codes = () => props.country_codes ?? {};

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
        return (
          <div class="rounded-sm bg-white/70 p-2">Hover over a country</div>
        );
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
      class="z-0 h-[280px] w-full rounded-sm"
      style={{ "background-color": "transparent" }}
      ref={ref}
    />
  );
}

function LogsChart(props: { rates: Stats["rate"] }) {
  const data = (): ChartData | undefined => {
    const labels = props.rates.map(([ts, _v]) => Number(ts) * 1000);
    const data = props.rates.map(([_ts, v]) => v);

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
                    const ts = new Date(item.parsed.x ?? 0);
                    return ts.toUTCString();
                  });
                },
                label: (item: TooltipItem<"scatter">) => {
                  return `rate: ${(item.parsed.y ?? 0).toPrecision(2)}/s`;
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

export default LogsPage;
