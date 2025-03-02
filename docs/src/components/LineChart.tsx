import { onCleanup, createEffect } from "solid-js";
import { Chart, type ChartData, type ScaleOptions } from "chart.js/auto";
import ChartDeferred from "chartjs-plugin-deferred";

import { createDarkMode } from "@/lib/darkmode";

Chart.register(ChartDeferred);

interface LineChartProps {
  data: ChartData<"line">;
  scales?: { [key: string]: ScaleOptions<"linear"> };
}

export function LineChart(props: LineChartProps) {
  const darkMode = createDarkMode();

  let ref: HTMLCanvasElement | undefined;
  let chart: Chart | undefined;

  createEffect(() => {
    chart?.destroy();

    chart = new Chart(ref!, {
      type: "line",
      data: props.data,
      options: {
        scales: adjustScaleColor(darkMode(), {
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
        interaction: {
          mode: "nearest",
          axis: "x",
          intersect: false,
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
