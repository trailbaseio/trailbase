import {
  Match,
  Switch,
  Show,
  createSignal,
  onCleanup,
  onMount,
  createMemo,
} from "solid-js";
import type { Setter } from "solid-js";
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
import { numericToAlpha2, getAlpha2Codes } from "i18n-iso-countries";
import type { FeatureCollection } from "geojson";

import "maplibre-gl/dist/maplibre-gl.css";
import maplibregl from "maplibre-gl";

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
function LogsPage() {
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
                <Switch>
                  <Match when={showMap()}>
                    <div class="md:w-1/2">
                      <WorldMap
                        countryCodes={
                          logsFetch.data?.stats?.country_codes ?? null
                        }
                      />
                    </div>
                    <div class="md:w-1/2">
                      <LogsChart rates={logsFetch.data?.stats?.rate ?? []} />
                    </div>
                  </Match>

                  <Match when={true}>
                    <div class="w-full">
                      <LogsChart rates={logsFetch.data?.stats?.rate ?? []} />
                    </div>
                  </Match>
                </Switch>
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

function buildMap(opts: {
  countryCodes: Stats["country_codes"];
  setMapDialog: Setter<string | undefined>;
  maxScale: number;
}): maplibregl.Map {
  const map = new maplibregl.Map({
    container: "map",
    hash: "map",
    zoom: 1.2,
    maxZoom: 4,
    center: [-50, 20],
    style: "https://tiles.openfreemap.org/styles/positron",
    attributionControl: {
      compact: true,
    },
  });

  map.addControl(
    new maplibregl.NavigationControl({
      visualizePitch: true,
    }),
  );

  map.addControl(new maplibregl.GlobeControl());
  map.addControl(new maplibregl.FullscreenControl());
  // map.addControl(new maplibregl.ScaleControl());

  map.on("style.load", () => {
    map.setProjection({
      type: "globe",
    });
  });

  map.on("load", () => {
    map.addSource(sourceId, {
      type: "geojson",
      data: {
        type: "FeatureCollection",
        features: (countriesGeoJSON as FeatureCollection).features.map((f) => {
          const code = numericToAlpha2(f.id as string | number) ?? "";

          return {
            ...f,
            properties: {
              ...f.properties,
              requests: opts.countryCodes?.[code],
            },
          };
        }),
      },
    });

    map.addLayer({
      id: layerId,
      type: "fill",
      source: sourceId,
      layout: {},
      paint: {
        // prettier-ignore
        "fill-color": [
          "interpolate",
          ["linear"],
          ["coalesce", ["get", "requests"], 0],
          0, "transparent",
          1, emerald100,
          opts.maxScale, primary,
        ],
        "fill-opacity": [
          "case",
          ["boolean", ["feature-state", "hover"], false],
          0.6,
          0.4,
        ],
        "fill-outline-color": [
          "case",
          ["boolean", ["feature-state", "hover"], false],
          "#000000",
          "transparent",
        ],
      },
    });

    let hoveredStateId: string | number | undefined;

    map.on("mouseenter", layerId, (_e) => {
      map.getCanvas().style.cursor = "pointer";
    });

    map.on("mousemove", layerId, (e) => {
      const first = e.features?.[0];
      if (hoveredStateId) {
        map.setFeatureState(
          { source: sourceId, id: hoveredStateId },
          { hover: false },
        );
      }

      if (first !== undefined) {
        hoveredStateId = first.id;
        map.setFeatureState(
          { source: sourceId, id: first.id },
          { hover: true },
        );

        const requests = first.properties["requests"] ?? 0;
        opts.setMapDialog(`${first.properties["name"]}: ${requests} req`);
      }
    });

    map.on("mouseleave", layerId, () => {
      map.getCanvas().style.cursor = "";

      if (hoveredStateId) {
        map.setFeatureState(
          { source: sourceId, id: hoveredStateId },
          { hover: false },
        );
      }

      opts.setMapDialog(undefined);
    });

    // map.on("click", layerId, (e) => {
    //   const first = e.features?.[0];
    //   if (first === undefined) {
    //     return;
    //   }
    //   const requests: number = first.properties["requests"] ?? 0;
    //   if (requests > 0) {
    //     const name =
    //       first.properties.name ??
    //       numericToAlpha2(first.id as string | number) ??
    //       "";
    //
    //     new maplibregl.Popup()
    //       .setLngLat(e.lngLat)
    //       .setHTML(`${name}: ${requests} req`)
    //       .addTo(map);
    //   }
    // });
  });

  return map;
}

function MapOverlay(props: {
  mapDialog: string | undefined;
  scaleMax: number;
}) {
  return (
    <>
      {/* request scale */}
      <div class="absolute top-2 left-2 w-[100px] rounded-sm bg-white/70 p-1 text-sm">
        <div class="flex h-[20px] w-full">
          <div class="h-full w-px bg-gray-600" />
          <div class="to-primary flex h-full grow justify-center bg-linear-to-r from-emerald-100" />
          <div class="h-full w-px bg-gray-600" />
        </div>

        <div class="relative h-4">
          <span class="absolute left-0">0</span>

          <span class="absolute right-0">
            {new Intl.NumberFormat("en-US", {
              notation: "compact",
              compactDisplay: "short",
            }).format(props.scaleMax)}
          </span>
        </div>

        <Show when={false}>
          <div class="bg-white">
            <div class="h-[20px] w-[20] bg-emerald-100" />
            <div class="bg-primary h-[20px] w-[20]" />
          </div>
        </Show>
      </div>

      {/* hover label */}
      <div class="absolute bottom-2 left-2 min-w-[120px] shrink rounded-sm bg-white/70 p-1 text-center text-sm">
        <Switch>
          <Match when={props.mapDialog !== undefined}>
            <p class="min-h-4 text-wrap">{props.mapDialog}</p>
          </Match>

          <Match when={true}>
            <p class="min-h-4 text-wrap text-gray-600">{"hover country"}</p>
          </Match>
        </Switch>
      </div>
    </>
  );
}

function WorldMap(props: { countryCodes: Stats["country_codes"] }) {
  const [mapDialog, setMapDialog] = createSignal<string | undefined>();
  const countryCodes = createMemo(
    () =>
      (import.meta.env.DEV
        ? appendDevData(props.countryCodes)
        : props.countryCodes) ?? {},
  );
  const maxScale = createMemo(() => {
    let maxRequests = 0;
    for (const [code, requests] of Object.entries(countryCodes())) {
      if (code !== "unattributed") {
        maxRequests = Math.max(maxRequests, requests);
      }
    }
    return Math.max(1000, maxRequests);
  });

  let map: maplibregl.Map | undefined;

  onMount(() => {
    map = buildMap({
      countryCodes: countryCodes(),
      setMapDialog,
      maxScale: maxScale(),
    });
  });

  onCleanup(() => {
    map?.remove();
    map = undefined;
  });

  return (
    <div class="relative">
      <div class="pointer-events-none absolute z-10 size-full overflow-hidden">
        <MapOverlay mapDialog={mapDialog()} scaleMax={maxScale()} />
      </div>

      <div
        id="map"
        class="z-0 h-[300px] w-full rounded-sm"
        style={{ "background-color": "transparent" }}
      />
    </div>
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

  onMount(() => {
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
                  return new Date(value).toLocaleTimeString(undefined, {
                    hourCycle: "h24",
                  });
                },
              },
            },
          },
          borderColor: primary,
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

  onCleanup(() => {
    chart?.destroy();
    chart = undefined;
  });

  return (
    <div class="h-[300px]">
      <canvas ref={ref} />
    </div>
  );
}

function appendDevData(
  countryCodes: Stats["country_codes"],
): Stats["country_codes"] {
  const copy = {
    ...countryCodes,
  };

  const allCodes = getAlpha2Codes();
  for (const code of Object.keys(allCodes)) {
    copy[code] = (copy[code] ?? 0) + Math.round(Math.random() * 2000);
  }

  return copy;
}

const sourceId = "countriesSource" as const;
const layerId = "countriesLayer" as const;

const primary = "#0073a8" as const;
const emerald100 = "#d0fae5" as const;

// Needed for dynamic load.
export default LogsPage;
