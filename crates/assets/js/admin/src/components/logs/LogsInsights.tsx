import {
  Match,
  Switch,
  Show,
  For,
  createSignal,
  onCleanup,
  createMemo,
  createEffect,
  createUniqueId,
} from "solid-js";
import { Chart } from "chart.js/auto";
import type { TooltipItem } from "chart.js/auto";
import { numericToAlpha2, getAlpha2Codes } from "i18n-iso-countries";
import type { FeatureCollection } from "geojson";
import * as maplibregl from "maplibre-gl";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url";
import countriesGeoJSON from "@/assets/countries-110m.json";
import type { StatsResponse } from "@bindings/StatsResponse";
import { createIsMobile } from "@/lib/signals";
import "maplibre-gl/dist/maplibre-gl.css";
maplibregl.setWorkerUrl(workerUrl);

type CountryCodes = Exclude<StatsResponse["country_codes"], null>;
export type LogsInsightsProps = {
  rates: StatsResponse["rates"];
  countryCodes: StatsResponse["country_codes"];
  loading?: boolean;
  error?: unknown;
  onRetry?: () => void;
};

function buildMap(opts: {
  countryCodes: CountryCodes;
  setMapDialog: (value: string | undefined) => void;
  maxScale: number;
  container: HTMLDivElement;
  isActive: () => boolean;
}): maplibregl.Map {
  const primary =
    getComputedStyle(document.documentElement)
      .getPropertyValue("--primary")
      .trim() || "currentColor";
  const map = new maplibregl.Map({
    container: opts.container,
    hash: false, // Don't manipulate url to place coordinates.
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

  map.on("error", (event) => {
    console.error("Map rendering error", event.error);
  });

  map.on("style.load", () => {
    if (!opts.isActive()) return;
    map.setProjection({
      type: "globe",
    });
  });

  map.on("load", () => {
    if (!opts.isActive()) return;
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
      if (!opts.isActive()) return;
      map.getCanvas().style.cursor = "pointer";
    });

    map.on("mousemove", layerId, (e) => {
      if (!opts.isActive()) return;
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
      if (!opts.isActive()) return;
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
      <div class="bg-background/70 absolute top-2 left-2 w-[100px] rounded-sm p-1 text-sm">
        <div class="flex h-[20px] w-full">
          <div class="bg-muted-foreground h-full w-px" />
          <div class="from-success to-primary flex h-full grow justify-center bg-linear-to-r" />
          <div class="bg-muted-foreground h-full w-px" />
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
          <div class="bg-background">
            <div class="bg-success h-[20px] w-[20]" />
            <div class="bg-primary h-[20px] w-[20]" />
          </div>
        </Show>
      </div>

      {/* hover label */}
      <div class="bg-background/70 absolute bottom-2 left-2 min-w-[120px] shrink rounded-sm p-1 text-center text-sm">
        <Switch>
          <Match when={props.mapDialog !== undefined}>
            <p class="min-h-4 text-wrap">{props.mapDialog}</p>
          </Match>

          <Match when={true}>
            <p class="text-muted-foreground min-h-4 text-wrap">
              {"hover country"}
            </p>
          </Match>
        </Switch>
      </div>
    </>
  );
}

function WorldMap(props: { countryCodes: CountryCodes }) {
  const [mapDialog, setMapDialog] = createSignal<string>();
  const codes = createMemo(() =>
    import.meta.env.DEV
      ? appendDevData(props.countryCodes)
      : props.countryCodes,
  );
  const maxScale = createMemo(() =>
    Math.max(
      1000,
      ...Object.entries(codes())
        .filter(([c]) => c !== "unattributed")
        .map(([, n]) => n),
    ),
  );
  let map: maplibregl.Map | undefined;
  let container!: HTMLDivElement;
  let generation = 0;
  createEffect(() => {
    const currentGeneration = ++generation;
    map?.remove();
    map = undefined;
    try {
      map = buildMap({
        countryCodes: codes(),
        setMapDialog,
        maxScale: maxScale(),
        container,
        isActive: () => generation === currentGeneration,
      });
    } catch (error) {
      console.error("Unable to render map", error);
    }
  });
  onCleanup(() => {
    generation++;
    map?.remove();
    map = undefined;
  });
  return (
    <div class="relative">
      <div class="pointer-events-none absolute z-10 size-full overflow-hidden">
        <MapOverlay mapDialog={mapDialog()} scaleMax={maxScale()} />
      </div>
      <div
        ref={container}
        aria-label="Request geography map"
        role="img"
        class="z-0 h-[180px] w-full rounded-sm"
      />
      <ul class="sr-only" aria-label="Requests by country">
        <For each={Object.entries(props.countryCodes)}>
          {([code, requests]) => (
            <li>
              {code}: {requests} requests
            </li>
          )}
        </For>
      </ul>
    </div>
  );
}

function LogsGraph(props: { rates: StatsResponse["rates"] }) {
  let canvas!: HTMLCanvasElement;
  let chart: Chart | undefined;
  createEffect(() => {
    const rates = props.rates;
    chart?.destroy();
    chart = undefined;
    if (!canvas) return;
    const primary =
      getComputedStyle(document.documentElement)
        .getPropertyValue("--primary")
        .trim() || "currentColor";
    try {
      chart = new Chart(canvas, {
        type: "scatter",
        data: {
          labels: rates.map(([ts]) => Number(ts) * 1000),
          datasets: [
            {
              type: "bar",
              label: "Request rate",
              data: rates.map(([, v]) => v),
              backgroundColor: primary,
            },
          ],
        },
        options: {
          maintainAspectRatio: false,
          scales: {
            y: { beginAtZero: true },
            x: {
              ticks: {
                callback: (value: number | string) =>
                  new Date(value as number).toLocaleTimeString(undefined, {
                    hourCycle: "h24",
                  }),
              },
            },
          },
          borderColor: primary,
          plugins: {
            legend: { display: false },
            tooltip: {
              callbacks: {
                title: (items: TooltipItem<"scatter">[]) =>
                  items.map((item) =>
                    new Date(item.parsed.x ?? 0).toUTCString(),
                  ),
                label: (item: TooltipItem<"scatter">) =>
                  `rate: ${(item.parsed.y ?? 0).toPrecision(2)}/s`,
              },
            },
          },
        },
      });
    } catch (error) {
      console.error("Unable to render chart", error);
    }
  });
  onCleanup(() => {
    chart?.destroy();
    chart = undefined;
  });
  return (
    <div class="h-[180px]">
      <p class="sr-only">
        Request rate: {props.rates.length} samples; latest{" "}
        {props.rates.at(-1)?.[1] ?? 0}/s.
      </p>
      <canvas ref={canvas} aria-label="Request rate over time" role="img" />
    </div>
  );
}

export function LogsInsights(props: LogsInsightsProps) {
  const isMobile = createIsMobile();
  const [expanded, setExpanded] = createSignal(!isMobile());
  const contentId = createUniqueId();

  return (
    <section aria-label="Activity" class="w-full">
      <button
        type="button"
        aria-expanded={expanded()}
        aria-controls={contentId}
        onClick={() => setExpanded((v) => !v)}
        class="w-full text-left font-medium"
      >
        Activity
      </button>
      <Show when={expanded()}>
        <div id={contentId} class="mt-2 flex flex-col gap-2 md:flex-row">
          <Switch>
            <Match when={props.error}>
              <div role="alert">
                Unable to load activity.{" "}
                <button type="button" onClick={() => props.onRetry?.()}>
                  Retry
                </button>
              </div>
            </Match>
            <Match when={props.loading}>
              <p role="status">Loading activity…</p>
            </Match>
            <Match when={true}>
              <Show
                when={props.countryCodes}
                fallback={
                  <p>
                    Geography unavailable. Place{" "}
                    <code>&lt;traildepot&gt;/GeoLite2-Country.mmdb</code> to
                    enable geographic insights.
                  </p>
                }
              >
                <div class="md:w-1/2">
                  <WorldMap countryCodes={props.countryCodes!} />
                </div>
              </Show>
              <div class="md:w-1/2">
                <LogsGraph rates={props.rates ?? []} />
              </div>
            </Match>
          </Switch>
        </div>
      </Show>
    </section>
  );
}

const sourceId = "countriesSource" as const;
const layerId = "countriesLayer" as const;

const emerald100 = "#d0fae5" as const;

function appendDevData(countryCodes: CountryCodes): CountryCodes {
  const copy = { ...countryCodes };

  for (const code of Object.keys(getAlpha2Codes())) {
    copy[code] = (copy[code] ?? 0) + Math.round(Math.random() * 2000);
  }

  return copy;
}
