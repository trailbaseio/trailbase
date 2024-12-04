import { type ChartData, type ChartDataset, type Tick } from "chart.js/auto";

import { BarChart } from "@/components/BarChart.tsx";
import { LineChart } from "@/components/LineChart.tsx";
import { ScatterChart } from "@/components/ScatterChart.tsx";

import { data as supabaseUtilization } from "./supabase_utilization";
import insertTrailBase from "./insert_tb.json";
import insertPocketBase from "./insert_pb.json";
import fibTrailBase from "./fib_tb.json";
import fibPocketBase from "./fib_pb.json";

type Datum = {
  cpu: number;
  rss: number;
  // Milliseconds
  elapsed: number;
};

const colors = {
  payload: "rgb(0, 101, 101)",
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

function transformMillisecondTicks(
  value: number | string,
  _index: number,
): string | undefined {
  const v = +value;
  if (v % 1000 === 0) {
    // WARN: These are estimate time due to how we measure: periodic
    // polling every 0.5s using `top` or `docker stats`, which themselves
    // have sampling intervals. The actual value shouldn't matter that
    // much, since we measure the actual duration in-situ. We do this
    // transformation only to make the time scale more intuitive than
    // just "time at sample X".
    return `${v / 1000}s`;
  }
}

const durations100k = {
  payload: {
    label: "Payload v3+SQLite",
    data: [656.09],
    backgroundColor: colors.payload,
    hidden: true,
  },
  supabase: {
    label: "SupaBase",
    data: [151],
    backgroundColor: colors.supabase,
  },
  pocketbase_ts: {
    label: "PocketBase TS",
    data: [67.721],
    backgroundColor: colors.pocketbase0,
  },
  pocketbase_dart_aot: {
    label: "PocketBase Dart (AOT)",
    data: [62.8136],
  },
  pocketbase_dart_jit: {
    label: "PocketBase Dart",
    data: [61.687],
    backgroundColor: colors.pocketbase1,
  },
  trailbase_ts: {
    label: "TrailBase TS",
    data: [16.502],
    backgroundColor: colors.trailbase0,
  },
  trailbase_dart_aot: {
    // AOT
    label: "TrailBase Dart",
    data: [7.0869],
    backgroundColor: colors.trailbase1,
  },
  trailbase_dart_jit: {
    // JIT
    label: "TrailBase Dart",
    data: [8.0667],
    backgroundColor: colors.trailbase1,
  },
  // {
  //   label: "TrailBase Dart (JIT + PGO)",
  //   data: [10.05],
  // },
  // STALE:
  // trailbase_dart_jit_int_pk: {
  //   label: "TrailBase Dart (INT PK)",
  //   data: [8.5249],
  //   backgroundColor: colors.trailbase2,
  // },
  trailbase_dart_dotnet: {
    // Inserted 100000 messages, took 00:00:05.7071809 (limit=64) (C#)
    label: "TrailBase C#",
    data: [5.7071],
    backgroundColor: colors.trailbase2,
  },
  drizzle: {
    label: "Drizzel SQLite (Node.js)",
    data: [8.803],
    backgroundColor: colors.drizzle,
  },
};

export function Duration100kInsertsChart() {
  const data: ChartData<"bar"> = {
    labels: ["Time in seconds (lower is faster)"],
    datasets: [
      durations100k.payload,
      durations100k.supabase,
      durations100k.pocketbase_ts,
      durations100k.pocketbase_dart_jit,
      durations100k.trailbase_ts,
      durations100k.trailbase_dart_aot,
      durations100k.trailbase_dart_dotnet,
      durations100k.drizzle,
    ] as ChartDataset<"bar">[],
  };

  return (
    <BarChart
      data={data}
      scales={{
        y: {
          display: true,
          // type: "logarithmic",
        },
      }}
    />
  );
}

export function PocketBaseAndTrailBaseReadLatencies() {
  // 2024-10-12
  // TB: Read 1 000 000 messages, took 0:00:57.952120 (limit=64) (Dart JIT)
  // const readTrailbaseMicroS = {
  //   p50: 3504,
  //   p75: 3947,
  //   p90: 4393,
  //   p95: 4725,
  // };
  // 2024-12-04
  // TB: Read 1 000 000 messages, took 0:00:55.025486 (limit=64) (Dart AOT)
  const readTrailbaseMicroS = {
    p50: 3291,
    p75: 3590,
    p90: 4027,
    p95: 4428,
  };

  // 2024-12-05
  // TB: Read 1 000 000 messages, took 00:00:21.8387601 (limit=64) (C#)
  const readTrailbaseDotnetMicroS = {
    p50: 947.5,
    p75: 1240.9,
    p90: 1553.3,
    p95: 1895.4,
  };

  // 2024-10-12
  // PB: Read 100 000 messages, took 0:00:20.273054 (limit=64)
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
        backgroundColor: colors.trailbase1,
      },
      {
        label: "TrailBase C#",
        data: latenciesMs(readTrailbaseDotnetMicroS),
        backgroundColor: colors.trailbase2,
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
  // TB: Inserted 10 000 messages, took 0:00:01.654810 (limit=64) (Dart JIT)
  // const insertTrailbaseMicroS = {
  //   p50: 8107,
  //   p75: 10897,
  //   p90: 15327,
  //   p95: 19627,
  // };
  // 2024-12-04
  // TB: Inserted 10 000 messages, took 0:00:00.863628 (limit=64) (Dart AOT)
  const insertTrailbaseMicroS = {
    p50: 5070,
    p75: 5695,
    p90: 6615,
    p95: 7371,
  };

  // 2024-12-05
  // TB: Inserted 10 000 messages, took 00:00:00.5542653 (limit=64) (C#)
  const insertTrailbaseDotnetMicroS = {
    p50: 3348.9,
    p75: 3810,
    p90: 4246.5,
    p95: 4489.9,
  };

  // 2024-10-12
  // PB: Inserted 10 000 messages, took 0:00:07.759677 (limit=64)
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
        backgroundColor: colors.trailbase1,
      },
      {
        label: "TrailBase C#",
        data: latenciesMs(insertTrailbaseDotnetMicroS),
        backgroundColor: colors.trailbase2,
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
  // specific run:
  //   PB: Inserted 100000 messages, took 0:01:02.549495 (limit=64)
  //   TB: Inserted 100000 messages, took 0:00:07.086891 (limit=64) (Dart AOT)
  //   TB: Inserted 100000 messages, took 00:00:05.7039362 (limit=64) (C#)

  const trailbaseUtilization = insertTrailBase as Datum[];
  const pocketbaseUtilization = insertPocketBase as Datum[];

  const trailbaseTimeOffset = -5.1 * 1000;

  const data: ChartData<"scatter"> = {
    datasets: [
      {
        yAxisID: "yLeft",
        label: "TrailBase CPU",
        data: trailbaseUtilization.map((datum) => ({
          x: datum.elapsed + trailbaseTimeOffset,
          y: datum.cpu,
        })),
        borderColor: colors.trailbase0,
        backgroundColor: colors.trailbase0,
        showLine: true,
      },
      {
        yAxisID: "yRight",
        label: "TrailBase RSS",
        data: trailbaseUtilization.map((datum) => ({
          x: datum.elapsed + trailbaseTimeOffset,
          y: datum.rss,
        })),
        borderColor: colors.trailbase1,
        backgroundColor: colors.trailbase1,
        showLine: true,
      },
      {
        yAxisID: "yLeft",
        label: "PocketBase CPU",
        data: pocketbaseUtilization.map((datum) => ({
          x: datum.elapsed,
          y: datum.cpu,
        })),
        borderColor: colors.pocketbase0,
        backgroundColor: colors.pocketbase0,
        showLine: true,
      },
      {
        yAxisID: "yRight",
        label: "PocketBase RSS",
        data: pocketbaseUtilization.map((datum) => ({
          x: datum.elapsed,
          y: datum.rss,
        })),
        borderColor: colors.pocketbase1,
        backgroundColor: colors.pocketbase1,
        showLine: true,
      },
    ],
  };

  return (
    <ScatterChart
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
          min: 0,
          ticks: {
            display: true,
            callback: transformMillisecondTicks,
          },
        },
      }}
    />
  );
}

export function FibonacciPocketBaseAndTrailBaseUsageChart() {
  const fibTrailBaseUtilization = fibTrailBase as Datum[];
  const fibPocketBaseUtilization = fibPocketBase as Datum[];

  const data: ChartData<"scatter"> = {
    datasets: [
      {
        yAxisID: "yLeft",
        label: "TrailBase CPU",
        data: fibTrailBaseUtilization.map((datum) => ({
          x: datum.elapsed,
          y: datum.cpu,
        })),
        showLine: true,
        borderColor: colors.trailbase0,
        backgroundColor: colors.trailbase0,
      },
      {
        yAxisID: "yRight",
        label: "TrailBase RSS",
        data: fibTrailBaseUtilization.map((datum) => ({
          x: datum.elapsed,
          y: datum.rss,
        })),
        showLine: true,
        borderColor: colors.trailbase1,
        backgroundColor: colors.trailbase1,
      },
      {
        yAxisID: "yLeft",
        label: "PocketBase CPU",
        data: fibPocketBaseUtilization.map((datum) => ({
          x: datum.elapsed,
          y: datum.cpu,
        })),
        showLine: true,
        borderColor: colors.pocketbase0,
        backgroundColor: colors.pocketbase0,
      },
      {
        yAxisID: "yRight",
        label: "PocketBase RSS",
        data: fibPocketBaseUtilization.map((datum) => ({
          x: datum.elapsed,
          y: datum.rss,
        })),
        showLine: true,
        borderColor: colors.pocketbase1,
        backgroundColor: colors.pocketbase1,
      },
    ],
  };

  return (
    <ScatterChart
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
          max: 650 * 1000,
          ticks: {
            display: true,
            callback: transformMillisecondTicks,
          },
        },
      }}
    />
  );
}
