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
  /* Fibonacci(40) benchmarks: rust, JS, JS + AOT.
   * single-run rust: 0.3s (100runs: 14.1s)
   * Called "/fibonacci" for fib(40) 100 times, took 0:00:07.142883 (limit=64)
   * single-run JS no-AOT: 45.3s
   * Called "/fibonacci" for fib(40) 100 times, took 0:29:43.038271 (limit=64)
   * single-run JS AOT: 29.4s (10runs 1:46.9)
   * Called "/fibonacci" for fib(40) 100 times, took 0:18:47.238479 (limit=64)
   * Custom QuickJS:
   * Called "/fibonacci" for fib(40) 100 times, took 0:11:36.009678 (limit=64)
   * PocketBase
   * Called "/fibonacci" for fib(40) 100 times, took 0:16:12.853627 (limit=64)
   * single-run V8: 0.9s
   * Called "/fibonacci" for fib(40) 100 times, took 0:00:26.959904 (limit=64)
   */

  const data: ChartData<"bar"> = {
    labels: ["100 runs fib(40) [less is faster]"],
    datasets: [
      {
        label: "V8",
        data: [26.96],
        backgroundColor: green1,
      },
      {
        label: "WASM Rust",
        data: [7.14],
        backgroundColor: blue,
      },
      {
        label: "WASM SpiderMonkey JS",
        data: [29 * 60 + 43],
        backgroundColor: purple0,
      },
      {
        label: "WASM SpiderMonkey JS + weval",
        data: [18 * 60 + 47],
        backgroundColor: purple1,
      },
      {
        label: "WASM custom QuickJS",
        data: [11 * 60 + 36],
        backgroundColor: purple2,
      },
      {
        label: "PocketBase (Goja JS)",
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
          type: "logarithmic",
          title: {
            display: true,
            text: "Time [s]",
          },
        },
      }}
    />
  );
}
