import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const chartInstances: Array<{ destroy: ReturnType<typeof vi.fn> }> = [];
  const chartConfigs: unknown[] = [];
  const mapInstances: Array<{
    remove: ReturnType<typeof vi.fn>;
    handlers: Map<string, (...args: never[]) => void>;
    setProjection: ReturnType<typeof vi.fn>;
    addSource: ReturnType<typeof vi.fn>;
    addLayer: ReturnType<typeof vi.fn>;
  }> = [];
  const mapConfigs: unknown[] = [];
  const events: string[] = [];

  return { chartInstances, chartConfigs, mapInstances, mapConfigs, events };
});

vi.mock("chart.js/auto", () => ({
  Chart: vi.fn(function (_canvas: HTMLCanvasElement, config: unknown) {
    const instance = {
      destroy: vi.fn(() => mocks.events.push("chart:destroy")),
    };
    mocks.events.push("chart:create");
    mocks.chartConfigs.push(config);
    mocks.chartInstances.push(instance);
    return instance;
  }),
}));

vi.mock("maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url", () => ({
  default: "mock-worker.js",
}));

vi.mock("maplibre-gl", () => {
  class MapMock {
    remove = vi.fn(() => mocks.events.push("map:remove"));
    handlers = new Map<string, (...args: never[]) => void>();
    addControl = vi.fn();
    addSource = vi.fn();
    addLayer = vi.fn();
    setProjection = vi.fn();
    setFeatureState = vi.fn();
    getCanvas = vi.fn(() => ({ style: { cursor: "" } }));

    constructor(config: unknown) {
      mocks.events.push("map:create");
      mocks.mapConfigs.push(config);
      mocks.mapInstances.push(this);
    }

    on(event: string, ...args: unknown[]) {
      this.handlers.set(event, args.at(-1) as (...args: never[]) => void);
      return this;
    }
  }

  return {
    Map: MapMock,
    NavigationControl: class {},
    GlobeControl: class {},
    FullscreenControl: class {},
    setWorkerUrl: vi.fn(),
  };
});

import { LogsInsights } from "@/components/logs/LogsInsights";

const rates: Array<[bigint, number]> = [
  [1_700_000_000n, 2.5],
  [1_700_000_060n, 4],
];

function setWidth(width: number) {
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    writable: true,
    value: width,
  });
}

describe("LogsInsights", () => {
  beforeEach(() => {
    setWidth(1024);
    mocks.chartInstances.length = 0;
    mocks.chartConfigs.length = 0;
    mocks.mapInstances.length = 0;
    mocks.mapConfigs.length = 0;
    mocks.events.length = 0;
    document.documentElement.style.removeProperty("--primary");
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("provides an accessible Activity disclosure", async () => {
    render(() => <LogsInsights rates={rates} countryCodes={null} />);

    const toggle = screen.getByRole("button", { name: /activity/i });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("img", { name: /request rate/i })).toBeVisible();

    await fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByRole("img", { name: /request rate/i }),
    ).not.toBeInTheDocument();

    await fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });

  it("starts collapsed at 390px and expanded on desktop", () => {
    setWidth(390);
    const mobile = render(() => (
      <LogsInsights rates={rates} countryCodes={null} />
    ));
    expect(screen.getByRole("button", { name: /activity/i })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    mobile.unmount();

    setWidth(1024);
    render(() => <LogsInsights rates={rates} countryCodes={null} />);
    expect(screen.getByRole("button", { name: /activity/i })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("renders a compact labeled request-rate surface", () => {
    render(() => <LogsInsights rates={rates} countryCodes={null} />);

    const canvas = screen.getByRole("img", { name: /request rate over time/i });
    expect(canvas.parentElement).toHaveClass("h-[180px]");
    expect(
      screen.getByText(/request rate: 2 samples; latest 4\/s/i),
    ).toBeInTheDocument();

    const config = mocks.chartConfigs[0] as {
      data: { datasets: Array<{ type: string; label: string }> };
      options: {
        scales: { x: { ticks: { callback: (value: number) => string } } };
        plugins: {
          tooltip: { callbacks: { label: (item: unknown) => string } };
        };
      };
    };
    expect(config.data.datasets[0]).toMatchObject({
      type: "bar",
      label: "Request rate",
    });
    expect(config.options.scales.x.ticks.callback(1_700_000_000_000)).toEqual(
      expect.any(String),
    );
    expect(
      config.options.plugins.tooltip.callbacks.label({ parsed: { y: 4 } }),
    ).toBe("rate: 4.0/s");
  });

  it("renders country data as a map with an accessible summary", () => {
    render(() => (
      <LogsInsights rates={rates} countryCodes={{ US: 12, DE: 3 }} />
    ));

    expect(
      screen.getByRole("img", { name: /request geography map/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("list", { name: /requests by country/i }),
    ).toHaveTextContent("US: 12 requests");
    expect(
      screen.getByRole("list", { name: /requests by country/i }),
    ).toHaveTextContent("DE: 3 requests");
    expect(mocks.mapConfigs).toHaveLength(1);
  });

  it("shows inline GeoIP setup guidance when geography is unavailable", () => {
    render(() => <LogsInsights rates={rates} countryCodes={null} />);

    expect(screen.getByText(/geography unavailable/i)).toBeInTheDocument();
    expect(
      screen.getByText("<traildepot>/GeoLite2-Country.mmdb"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("does not mistake loading stats for missing GeoIP data", () => {
    render(() => (
      <LogsInsights rates={[]} countryCodes={null} loading={true} />
    ));

    expect(screen.getByRole("status")).toHaveTextContent("Loading activity");
    expect(
      screen.queryByText(/geography unavailable/i),
    ).not.toBeInTheDocument();
  });

  it("contains a stats error and exposes Retry without blocking siblings", async () => {
    const onRetry = vi.fn();
    render(() => (
      <>
        <LogsInsights
          rates={[]}
          countryCodes={null}
          error={new Error("stats unavailable")}
          onRetry={onRetry}
        />
        <p>Logs table sibling</p>
      </>
    ));

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Unable to load activity",
    );
    expect(screen.getByText("Logs table sibling")).toBeVisible();
    await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("destroys the Chart before reactive replacement and on cleanup", () => {
    const [currentRates, setCurrentRates] = createSignal(rates);
    const view = render(() => (
      <LogsInsights rates={currentRates()} countryCodes={null} />
    ));
    const first = mocks.chartInstances[0];

    setCurrentRates([[1_700_000_120n, 8]]);

    expect(first.destroy).toHaveBeenCalledOnce();
    expect(mocks.events.slice(-2)).toEqual(["chart:destroy", "chart:create"]);
    expect(mocks.chartInstances).toHaveLength(2);

    const second = mocks.chartInstances[1];
    view.unmount();
    expect(second.destroy).toHaveBeenCalledOnce();
  });

  it("ignores stale MapLibre callbacks after replacement and cleanup", () => {
    const [countries, setCountries] = createSignal<Record<string, number>>({
      US: 12,
    });
    const view = render(() => (
      <LogsInsights rates={rates} countryCodes={countries()} />
    ));
    const first = mocks.mapInstances[0];
    const staleStyleLoad = first.handlers.get("style.load")!;
    const staleLoad = first.handlers.get("load")!;
    setCountries({ DE: 4 });
    expect(() => {
      staleStyleLoad();
      staleLoad();
    }).not.toThrow();
    expect(first.setProjection).not.toHaveBeenCalled();
    expect(first.addSource).not.toHaveBeenCalled();
    expect(first.addLayer).not.toHaveBeenCalled();

    view.unmount();
    expect(() => {
      staleStyleLoad();
      staleLoad();
    }).not.toThrow();
    expect(first.setProjection).not.toHaveBeenCalled();
    expect(first.addSource).not.toHaveBeenCalled();
    expect(first.addLayer).not.toHaveBeenCalled();
  });

  it("removes the Map before reactive replacement and on cleanup", () => {
    const [countries, setCountries] = createSignal<Record<string, number>>({
      US: 12,
    });
    const view = render(() => (
      <LogsInsights rates={rates} countryCodes={countries()} />
    ));
    const first = mocks.mapInstances[0];

    setCountries({ US: 13, DE: 2 });

    expect(first.remove).toHaveBeenCalledOnce();
    expect(mocks.events.slice(-2)).toEqual(["map:remove", "map:create"]);
    expect(mocks.mapInstances).toHaveLength(2);

    const second = mocks.mapInstances[1];
    view.unmount();
    expect(second.remove).toHaveBeenCalledOnce();
  });

  it("uses the computed --primary value for the chart", () => {
    document.documentElement.style.setProperty("--primary", "rgb(1, 2, 3)");
    render(() => <LogsInsights rates={rates} countryCodes={null} />);

    const config = mocks.chartConfigs[0] as {
      data: { datasets: Array<{ backgroundColor: string }> };
    };
    expect(config.data.datasets[0].backgroundColor).toBe("rgb(1, 2, 3)");
  });
});
