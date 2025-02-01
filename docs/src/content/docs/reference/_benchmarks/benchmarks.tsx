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

  // 2025-01-18
  // TB: Read 1 000 000 messages, took 00:00:21.8387601 (limit=64) (C#)
  const readTrailbaseRustMicroS = {
    p50: 379.254,
    p75: 438.446,
    p90: 506.493,
    p95: 554.533,
  };

  // 2024-10-12
  // PB: Read 1 000 000 rows in 26.724486507s (Rust).
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
      {
        label: "TrailBase Rust",
        data: latenciesMs(readTrailbaseRustMicroS),
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

  // 2025-01-18
  // TB: Inserted 10 000 rows in 873.530967ms
  const insertTrailbaseRustMicroS = {
    p50: 808.301,
    p75: 897.399,
    p90: 1001.876,
    p95: 1071.336,
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
      {
        label: "TrailBase Rust",
        data: latenciesMs(insertTrailbaseRustMicroS),
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
  //   TB: Inserted 100000 rows in 6.351865901s (rust)

  const trailbaseUtilization = insertTrailBase as Datum[];
  const pocketbaseUtilization = insertPocketBase as Datum[];

  const trailbaseTimeOffset = -0.35 * 1000;

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
          min: 0,
          max: 5,
          position: "left",
          title: {
            display: true,
            text: "CPU Cores",
          },
          ticks: {
            stepSize: 1,
          },
        },
        yRight: {
          min: 30 * 1024,
          max: 120 * 1024,
          position: "right",
          title: {
            display: true,
            text: "Resident Memory Size [MB]",
          },
          grid: {
            display: false,
          },
          ticks: {
            stepSize: 10 * 1024,
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
          min: 0,
          max: 16,
          ticks: {
            stepSize: 2,
          },
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
          min: 0,
          max: 400 * 1024,
          ticks: {
            stepSize: 50 * 1024,
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

const fsColors = {
  ext4: "#008b6dff",
  xfs: "#29c299ff",
  bcachefs: "#47a1cdff",
  btrfsNoComp: "#ba36c8ff",
  btrfsZstd1: "#c865d5ff",
  btrfsLzo: "#db9be3ff",
  zfs: "#e6bb1eff",
};

export function TrailBaseFileSystemReadLatency() {
  //                i100k   i10k      ip50 ip75 ip90 ip95    rp50 rp75 rp90 rp95
  // ext4	          2.415	  0.23942  	355	 425	476	 499	   169	196	 226	249
  // zfs	          3.532	  0.35463  	535	 581	655	 730	   170	197	 229	253
  // xfs	          2.3789	0.24695  	372	 441	481	 503	   168	195	 226	248
  // btrfs no compr	3.2142	0.32212  	475	 533	646	 689	   168	195	 226	249
  // btrfs zstd:1	  3.1774	0.31789  	475	 523	607	 659	   167	194	 225	249
  // btrfs lzo	    3.2673	0.34607  	513	 609	687	 726	   167	194	 224	247
  // bcachefs	      2.6001	0.27165  	398	 489	547	 572	   169	195	 226	249

  // 2025-02-01
  const readLatenciesMicroSec = {
    ext4: [169, 196, 226, 249],
    zfs: [170, 197, 229, 253],
    xfs: [168, 195, 226, 248],
    btrfsNoComp: [168, 195, 226, 249],
    btrfsZstd1: [167, 194, 225, 249],
    btrfsLzo: [167, 194, 224, 247],
    bcachefs: [169, 195, 226, 249],
  };

  const data: ChartData<"bar"> = {
    labels: ["p50", "p75", "p90", "p95"],
    datasets: [
      {
        label: "ext4",
        data: readLatenciesMicroSec.ext4,
        backgroundColor: fsColors.ext4,
      },
      {
        label: "xfs",
        data: readLatenciesMicroSec.xfs,
        backgroundColor: fsColors.xfs,
      },
      {
        label: "bcachefs",
        data: readLatenciesMicroSec.bcachefs,
        backgroundColor: fsColors.bcachefs,
      },
      {
        label: "btrfs w/o compression",
        data: readLatenciesMicroSec.btrfsNoComp,
        backgroundColor: fsColors.btrfsNoComp,
      },
      {
        label: "btrfs zstd:1",
        data: readLatenciesMicroSec.btrfsZstd1,
        backgroundColor: fsColors.btrfsZstd1,
      },
      {
        label: "btrfs lzo",
        data: readLatenciesMicroSec.btrfsLzo,
        backgroundColor: fsColors.btrfsLzo,
      },
      {
        label: "zfs",
        data: readLatenciesMicroSec.zfs,
        backgroundColor: fsColors.zfs,
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
            text: "Read Latency [µs]",
          },
        },
      }}
    />
  );
}

export function TrailBaseFileSystemWriteLatency() {
  //                i100k   i10k      ip50 ip75 ip90 ip95    rp50 rp75 rp90 rp95
  // ext4	          2.415	  0.23942  	355	 425	476	 499	   169	196	 226	249
  // zfs	          3.532	  0.35463  	535	 581	655	 730	   170	197	 229	253
  // xfs	          2.3789	0.24695  	372	 441	481	 503	   168	195	 226	248
  // btrfs no compr	3.2142	0.32212  	475	 533	646	 689	   168	195	 226	249
  // btrfs zstd:1	  3.1774	0.31789  	475	 523	607	 659	   167	194	 225	249
  // btrfs lzo	    3.2673	0.34607  	513	 609	687	 726	   167	194	 224	247
  // bcachefs	      2.6001	0.27165  	398	 489	547	 572	   169	195	 226	249

  // 2025-02-01
  const writeLatenciesMicroSec = {
    ext4: [355, 425, 476, 499],
    zfs: [535, 581, 655, 730],
    xfs: [372, 441, 481, 503],
    btrfsNoComp: [475, 533, 646, 689],
    btrfsZstd1: [475, 523, 607, 659],
    btrfsLzo: [513, 609, 687, 726],
    bcachefs: [398, 489, 547, 572],
  };

  const data: ChartData<"bar"> = {
    labels: ["p50", "p75", "p90", "p95"],
    datasets: [
      {
        label: "ext4",
        data: writeLatenciesMicroSec.ext4,
        backgroundColor: fsColors.ext4,
      },
      {
        label: "xfs",
        data: writeLatenciesMicroSec.xfs,
        backgroundColor: fsColors.xfs,
      },
      {
        label: "bcachefs",
        data: writeLatenciesMicroSec.bcachefs,
        backgroundColor: fsColors.bcachefs,
      },
      {
        label: "btrfs w/o compression",
        data: writeLatenciesMicroSec.btrfsNoComp,
        backgroundColor: fsColors.btrfsNoComp,
      },
      {
        label: "btrfs zstd:1",
        data: writeLatenciesMicroSec.btrfsZstd1,
        backgroundColor: fsColors.btrfsZstd1,
      },
      {
        label: "btrfs lzo",
        data: writeLatenciesMicroSec.btrfsLzo,
        backgroundColor: fsColors.btrfsLzo,
      },
      {
        label: "zfs",
        data: writeLatenciesMicroSec.zfs,
        backgroundColor: fsColors.zfs,
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
            text: "Write Latency [µs]",
          },
        },
      }}
    />
  );
}
