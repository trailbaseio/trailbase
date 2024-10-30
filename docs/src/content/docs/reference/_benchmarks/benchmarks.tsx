import { type ChartData, type ChartDataset, type Tick } from "chart.js/auto";

import { BarChart } from "@/components/BarChart.tsx";
import { LineChart } from "@/components/LineChart.tsx";

import { data as supabaseUtilization } from "./supabase_utilization";
import { data as pocketbaseUtilization } from "./pocketbase_utilization";
import { data as trailbaseUtilization } from "./trailbase_utilization";

const colors = {
  supabase: "rgb(62, 207, 142)",
  pocketbase0: "rgb(230, 128, 30)",
  pocketbase1: "rgb(238, 175, 72)",
  trailbase0: "rgb(0, 115, 170)",
  trailbase1: "rgb(71, 161, 205)",
  trailbase2: "rgb(146, 209, 242)",
  drizzle: "rgb(249, 39, 100)",
};

function transformTimeTicks(factor: number = 0.5) {
  return (_value: number | string, index: number): string | undefined => {
    if (index % 10 === 0) {
      // WARN: These are estimate time due to how we measure: periodic
      // polling every 0.5s using `top` or `docker stats`, which themselves
      // have sampling intervals. The actual value shouldn't matter that
      // much, since we measure the actual duration in-situ. We do this
      // transformation only to make the time scale more intuitive than
      // just "time at sample X".
      return `~${index * factor}s`;
    }
  };
}

const durations100k = [
  {
    label: "SupaBase",
    data: [151],
    backgroundColor: colors.supabase,
  },
  {
    label: "PocketBase TS",
    data: [67.721],
    backgroundColor: colors.pocketbase0,
  },
  // {
  //   label: "PocketBase Dart (AOT)",
  //   data: [62.8136],
  // },
  {
    label: "PocketBase Dart (JIT)",
    data: [61.687],
    backgroundColor: colors.pocketbase1,
  },
  {
    label: "TrailBase TS",
    data: [16.742],
    backgroundColor: colors.trailbase0,
  },
  // {
  //   label: "TrailBase Dart (AOT)",
  //   data: [11.1],
  // },
  {
    // label: "TrailBase Dart (JIT)",
    label: "TrailBase Dart",
    data: [9.4247],
    backgroundColor: colors.trailbase1,
  },
  // {
  //   label: "TrailBase Dart (JIT + PGO)",
  //   data: [10.05],
  // },
  // {
  //   label: "TrailBase Dart (INT PK)",
  //   data: [8.5249],
  //   backgroundColor: colors.trailbase2,
  // },
  {
    label: "In-process SQLite (Drizzle)",
    data: [8.803],
    backgroundColor: colors.drizzle,
  },
];

export function Duration100kInsertsChart() {
  const data: ChartData<"bar"> = {
    labels: ["Time [s] (lower is better)"],
    datasets: durations100k as ChartDataset<"bar">[],
  };

  return <BarChart data={data} />;
}

export function PocketBaseAndTrailBaseReadLatencies() {
  // 2024-10-12
  // Read 1000000 messages, took 0:00:57.952120 (limit=64)
  const readTrailbaseMicroS = {
    p50: 3504,
    p75: 3947,
    p90: 4393,
    p95: 4725,
  };

  // 2024-10-12
  // Read 100000 messages, took 0:00:20.273054 (limit=64)
  const readPocketbaseMicroS = {
    p50: 12740,
    p75: 13718,
    p90: 14755,
    p95: 15495,
  };

  const latenciesMs = (d: any) =>
    [d.p50, d.p75, d.p90, d.p95].map((p) => p / 1000);

  const data: ChartData<"bar"> = {
    labels: ["p50", "p75", "p90", "p95"],
    datasets: [
      {
        label: "PocketBase",
        data: latenciesMs(readPocketbaseMicroS),
        backgroundColor: colors.pocketbase0,
      },
      {
        label: "TrailBase",
        data: latenciesMs(readTrailbaseMicroS),
        backgroundColor: colors.trailbase0,
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
            text: "Read Latency [ms]",
          },
        },
      }}
    />
  );
}

export function PocketBaseAndTrailBaseInsertLatencies() {
  // 2024-10-12
  // Inserted 10000 messages, took 0:00:01.654810 (limit=64)
  const insertTrailbaseMicroS = {
    p50: 8107,
    p75: 10897,
    p90: 15327,
    p95: 19627,
  };
  // 2024-10-12
  //Inserted 10000 messages, took 0:00:07.759677 (limit=64)
  const insertPocketbaseMicroS = {
    p50: 28160,
    p75: 58570,
    p90: 108325,
    p95: 157601,
  };

  const latenciesMs = (d: any) =>
    [d.p50, d.p75, d.p90, d.p95].map((p) => p / 1000);

  const data: ChartData<"bar"> = {
    labels: ["p50", "p75", "p90", "p95"],
    datasets: [
      {
        label: "PocketBase",
        data: latenciesMs(insertPocketbaseMicroS),
        backgroundColor: colors.pocketbase0,
      },
      {
        label: "TrailBase",
        data: latenciesMs(insertTrailbaseMicroS),
        backgroundColor: colors.trailbase0,
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
            text: "Insert Latency [ms]",
          },
        },
      }}
    />
  );
}

export function SupaBaseMemoryUsageChart() {
  const data: ChartData<"line"> = {
    labels: [...Array(330).keys()],
    datasets: Object.keys(supabaseUtilization).map((key) => {
      const data = supabaseUtilization[key].map((datum, index) => ({
        x: index,
        y: datum.memUsageKb,
      }));

      return {
        label: key.replace("supabase-", ""),
        data: data,
        fill: true,
        showLine: false,
        pointStyle: false,
      };
    }),
  };

  return (
    <LineChart
      data={data}
      scales={{
        y: {
          stacked: true,
          title: {
            display: true,
            text: "Memory Usage [GB]",
          },
          ticks: {
            callback: (
              value: number | string,
              _index: number,
              _ticks: Tick[],
            ): string | undefined => {
              const v = value as number;
              return `${(v / 1024 / 1024).toFixed(0)}`;
            },
          },
        },
        x: {
          ticks: {
            display: true,
            callback: transformTimeTicks(),
          },
        },
      }}
    />
  );
}

export function SupaBaseCpuUsageChart() {
  const data: ChartData<"line"> = {
    labels: [...Array(330).keys()],
    datasets: Object.keys(supabaseUtilization).map((key) => {
      const data = supabaseUtilization[key].map((datum, index) => ({
        x: index,
        y: datum.cpuPercent ?? 0,
      }));

      return {
        label: key.replace("supabase-", ""),
        data: data,
        fill: true,
        showLine: false,
        pointStyle: false,
      };
    }),
  };

  return (
    <LineChart
      data={data}
      scales={{
        y: {
          stacked: true,
          title: {
            display: true,
            text: "CPU Cores",
          },
        },
        x: {
          ticks: {
            display: true,
            callback: transformTimeTicks(),
          },
        },
      }}
    />
  );
}

export function PocketBaseAndTrailBaseUsageChart() {
  // To roughly align start of benchmark on the time axis.
  const xOffset = 3;

  const data: ChartData<"line"> = {
    labels: [...Array(134).keys()],
    datasets: [
      {
        yAxisID: "yLeft",
        label: "PocketBase CPU",
        data: pocketbaseUtilization.slice(xOffset).map((datum, index) => ({
          x: index,
          y: datum.cpu,
        })),
        borderColor: colors.pocketbase0,
        backgroundColor: colors.pocketbase0,
      },
      {
        yAxisID: "yRight",
        label: "PocketBase RSS",
        data: pocketbaseUtilization.slice(xOffset).map((datum, index) => ({
          x: index,
          y: datum.rss,
        })),
        borderColor: colors.pocketbase1,
        backgroundColor: colors.pocketbase1,
      },
      {
        yAxisID: "yLeft",
        label: "TrailBase CPU",
        data: trailbaseUtilization.map((datum, index) => ({
          x: index,
          y: datum.cpu,
        })),
        borderColor: colors.trailbase0,
        backgroundColor: colors.trailbase0,
      },
      {
        yAxisID: "yRight",
        label: "TrailBase RSS",
        data: trailbaseUtilization.map((datum, index) => ({
          x: index,
          y: datum.rss,
        })),
        borderColor: colors.trailbase1,
        backgroundColor: colors.trailbase1,
      },
    ],
  };

  return (
    <LineChart
      data={data}
      scales={{
        yLeft: {
          position: "left",
          title: {
            display: true,
            text: "CPU Cores",
          },
        },
        yRight: {
          position: "right",
          title: {
            display: true,
            text: "Resident Memory Size [MB]",
          },
          ticks: {
            callback: (
              value: number | string,
              _index: number,
              _ticks: Tick[],
            ): string | undefined => {
              const v = value as number;
              return `${(v / 1024).toFixed(0)}`;
            },
          },
        },
        x: {
          ticks: {
            display: true,
            callback: transformTimeTicks(0.6),
          },
        },
      }}
    />
  );
}
