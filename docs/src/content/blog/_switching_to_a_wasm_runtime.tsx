import type { ChartData } from "chart.js/auto";
import { BarChart } from "@/components/BarChart.tsx";

// const green0 = "#008b6dff";
const green1 = "#29c299ff";
const blue = "#47a1cdff";
const purple0 = "#ba36c8ff";
const purple1 = "#c865d5ff";
const purple2 = "#db9be3ff";
const yellow = "#e6bb1eff";

export function RuntimeFib40Times() {
  const data: ChartData<"bar"> = {
    labels: ["p50"],
    datasets: [
      {
        label: "V8",
        data: [26.96],
        backgroundColor: green1,
      },
      {
        label: "Rust",
        data: [7.14],
        backgroundColor: blue,
      },
      {
        label: "SpiderMonkey",
        data: [29 * 60 + 43],
        backgroundColor: purple0,
      },
      {
        label: "SpiderMonkey + weval",
        data: [18 * 60 + 47],
        backgroundColor: purple1,
      },
      {
        label: "Custom QuickJS",
        data: [11 * 60 + 36],
        backgroundColor: purple2,
      },
      {
        label: "PocketBase",
        data: [16 * 60 + 12],
        backgroundColor: yellow,
      },
    ],
  };

  return (
    <BarChart
      data={data}
      scales={{
        y: {
          title: {
            display: true,
            text: "Time 100 fib(40) [s]",
          },
        },
      }}
    />
  );
}
