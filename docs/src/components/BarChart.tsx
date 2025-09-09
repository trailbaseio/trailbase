import { onCleanup, createEffect } from "solid-js";
import {
  Chart,
  type ChartData,
  type Tick,
  type ScaleOptions,
} from "chart.js/auto";
import {
  BarWithErrorBarsController,
  BarWithErrorBar,
} from "chartjs-chart-error-bars";
import ChartDeferred from "chartjs-plugin-deferred";

import { createDarkMode } from "@/lib/darkmode";

Chart.register(BarWithErrorBarsController, BarWithErrorBar, ChartDeferred);

interface BarChartProps {
  data: ChartData<"bar">;
  scales?: {
    [key: string]: ScaleOptions<"linear"> | ScaleOptions<"logarithmic">;
  };
}

export function BarChart(props: BarChartProps) {
  const darkMode = createDarkMode();

  let ref: HTMLCanvasElement | undefined;
  let chart: Chart | undefined;

  createEffect(() => {
    chart?.destroy();

    chart = new Chart<"bar">(ref!, {
      type: "bar",
      data: props.data,
      options: {
        scales: adjustScaleColor(darkMode(), {
          y: {},
          x: {},
          ...props.scales,
        }),
        maintainAspectRatio: false,
        plugins: {
          // Defers rendering and animation until on screen.
          deferred: {
            yOffset: "30%", // defer until 50% of the canvas height are inside the viewport
            delay: 200, // delay of 500 ms after the canvas is considered inside the viewport
          },
          colors: {
            enabled: true,
            forceOverride: false,
          },
          legend: {
            position: "bottom",
            labels: {
              color: darkMode() ? "white" : undefined,
            },
          },
        },
      },
    });
  });

  onCleanup(() => chart?.destroy());

  return (
    <div id="canvas-container" class="size-full">
      <canvas ref={ref} />
    </div>
  );
}

function adjustScaleColor(
  dark: boolean,
  scales: { [key: string]: ScaleOptions<"linear"> },
) {
  for (const axis of Object.keys(scales)) {
    const scale = scales[axis];

    scale.ticks = {
      ...scales[axis].ticks,
      color: dark ? "white" : undefined,
    };

    scale.title = {
      ...scale.title,
      color: dark ? "white" : undefined,
    };
  }

  return scales;
}

interface BarChartWithErrorsProps {
  data: ChartData<"barWithErrorBars">;
  yTickFormatter?: (
    value: number | string,
    index: number,
    ticks: Tick[],
  ) => string;
}

export function BarChartWithErrors(props: BarChartWithErrorsProps) {
  const darkMode = createDarkMode();

  let ref: HTMLCanvasElement | undefined;
  let chart: Chart | undefined;

  createEffect(() => {
    chart?.destroy();

    const scaleIds = props.data.datasets.map((e) => e.yAxisID ?? "y");
    const yScaleStyle = {
      ticks: {
        color: darkMode() ? "white" : undefined,
        display: true,
        callback: props.yTickFormatter,
      },
      grid: {
        display: true,
        lineWidth: 0,
        tickWidth: 0.5,
        tickLength: 2,
        tickColor: darkMode() ? "white" : "black",
      },
    };

    chart = new Chart<"barWithErrorBars">(ref!, {
      type: BarWithErrorBarsController.id,
      data: props.data,
      options: {
        scales: {
          x: {
            ticks: {
              color: darkMode() ? "white" : undefined,
            },
          },
          ...Object.fromEntries(scaleIds.map((id) => [id, yScaleStyle])),
        },
        maintainAspectRatio: false,
        plugins: {
          // Defers rendering and animation until on screen.
          deferred: {
            yOffset: "30%", // defer until 50% of the canvas height are inside the viewport
            delay: 200, // delay of 500 ms after the canvas is considered inside the viewport
          },
          colors: {
            enabled: true,
            forceOverride: false,
          },
          legend: {
            position: "bottom",
            labels: {
              color: darkMode() ? "white" : undefined,
            },
          },
        },
      },
    });
  });

  onCleanup(() => chart?.destroy());

  return (
    <div id="canvas-container" class="size-full">
      <canvas ref={ref} />
    </div>
  );
}
