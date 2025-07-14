import { type ChartData, type ChartDataset, type Tick } from "chart.js/auto";

import { BarChart } from "@/components/BarChart.tsx";
import { LineChart } from "@/components/LineChart.tsx";
import { ScatterChart } from "@/components/ScatterChart.tsx";

import { data as supabaseUtilization } from "./supabase_utilization";
import insertTrailBase from "./insert_tb_2025-04-03.json";
import insertPocketBase from "./insert_pb_2025-02-05.json";
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
    data: [45.408],
    backgroundColor: colors.pocketbase0,
  },
  pocketbase_dart_jit: {
    label: "PocketBase Dart",
    data: [42.896],
    backgroundColor: colors.pocketbase1,
  },
  trailbase_ts: {
    label: "TrailBase TS",
    data: [16.502],
    backgroundColor: colors.trailbase0,
  },
  trailbase_dart_aot: {
    // TB: Inserted 100000 messages, took 0:00:07.104429 (limit=64) 2025-04-03
    label: "TrailBase Dart",
    data: [7.0869],
    backgroundColor: colors.trailbase1,
  },
  trailbase_dart_jit: {
    // TB: Inserted 100000 messages, took 0:00:08.050607 (limit=64) 2025-04-03
    label: "TrailBase Dart",
    data: [8.0506],
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
  trailbase_dotnet: {
    // Inserted 100000 messages, took 00:00:05.7071809 (limit=64) (C#)
    // Inserted 100000 messages, took 00:00:04.3672948 (limit=64) (C#) 2025-04-03
    label: "TrailBase C#",
    data: [4.3673],
    backgroundColor: colors.trailbase2,
  },
  // TB: Inserted 100000 rows in 3.855656919s (2025-04-03)
  trailbase_rust: {
    // Inserted 100000 messages, took 00:00:05.7071809 (limit=64) (C#)
    label: "TrailBase Rust",
    data: [3.8556],
    backgroundColor: colors.trailbase2,
  },
  drizzle: {
    label: "Drizzle SQLite (Node.js)",
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
      durations100k.trailbase_dotnet,
      durations100k.trailbase_rust,
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

type Percentiles = {
  p50: number;
  p75: number;
  p90: number;
  p95: number;
};

export function PocketBaseAndTrailBaseReadLatencies() {
  // 2024-10-12
  // TB: Read 1 000 000 messages, took 0:00:57.952120 (limit=64) (Dart JIT)
  // const readTrailbaseDartMicroS = {
  //   p50: 3504,
  //   p75: 3947,
  //   p90: 4393,
  //   p95: 4725,
  // };
  // 2024-12-04
  // TB: Read 1 000 000 messages, took 0:00:55.025486 (limit=64) (Dart AOT)
  //
  // 2025-04-03
  // Read 1000000 messages, took 0:00:55.674628 (limit=64) (Dart AOT)
  // Latencies:
  //         p50=3362us
  //         p75=3597us
  //         p90=4003us
  //         p95=4307us
  const readTrailbaseDartMicroS = {
    p50: 3362,
    p75: 3597,
    p90: 4003,
    p95: 4307,
  };

  // 2024-12-05
  // TB: Read 1 000 000 messages, took 00:00:21.8387601 (limit=64) (C#)
  //
  // TB: Read 1000000 messages, took 00:00:20.6719154 (limit=64) 2025-04-03
  // Latencies:
  //       p50=784.1us
  //       p75=1328.4us
  //       p90=2059.1us
  //       p95=2656.6us
  const readTrailbaseDotnetMicroS = {
    p50: 784.1,
    p75: 1328.4,
    p90: 2059.1,
    p95: 2656.6,
  };

  // 2025-04-03
  // TB: Read 1000000 rows in 18.319852573s
  // Latencies:
  //         p50=259.522µs
  //         p75=301.269µs
  //         p90=347.464µs
  //         p95=381.137µs
  const readTrailbaseRustMicroS = {
    p50: 259.522,
    p75: 301.269,
    p90: 347.464,
    p95: 381.137,
  };

  // 2024-10-12
  // PB: Read 1 000 000 rows in 26.724486507s
  // const readPocketbaseMicroS = {
  //   p50: 12740,
  //   p75: 13718,
  //   p90: 14755,
  //   p95: 15495,
  // };
  //
  // 2025-02-05
  // PB v0.25.0: Read 100000 messages, took 0:00:26.628162 (limit=64)
  const readPocketbaseMicroS = {
    p50: 16678,
    p75: 18133,
    p90: 19599,
    p95: 20503,
  };

  const latenciesMs = (d: Percentiles) =>
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
        data: latenciesMs(readTrailbaseDartMicroS),
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
  // const insertTrailbaseDartMicroS = {
  //   p50: 8107,
  //   p75: 10897,
  //   p90: 15327,
  //   p95: 19627,
  // };
  // 2024-12-04
  // TB: Inserted 10 000 messages, took 0:00:00.863628 (limit=64) (Dart AOT)
  //
  // 2025-04-03
  // Inserted 10000 messages, took 0:00:00.753518 (limit=64) (Dart AOT)
  // Latencies:
  //         p50=4470us
  //         p75=5029us
  //         p90=5618us
  //         p95=6406us
  const insertTrailbaseDartMicroS = {
    p50: 4470,
    p75: 5029,
    p90: 5618,
    p95: 6406,
  };

  // 2024-12-05
  // TB: Inserted 10 000 messages, took 00:00:00.5542653 (limit=64) (C#)
  //
  // TB: Inserted 10000 messages, took 00:00:00.4230439 (limit=64) (C#) 2025-04-03
  // Latencies:
  //       p50=2652.8us
  //       p75=2880us
  //       p90=3113.8us
  //       p95=3479.8us
  const insertTrailbaseDotnetMicroS = {
    p50: 2652.8,
    p75: 2880,
    p90: 3113.8,
    p95: 2479.8,
  };

  // 2025-03-04
  // TB: Inserted 10000 rows in 376.664587ms
  // Latencies:
  //         p50=567.222µs
  //         p75=636.6µs
  //         p90=703.655µs
  //         p95=742.587µs
  const insertTrailbaseRustMicroS = {
    p50: 567.222,
    p75: 636.6,
    p90: 703.655,
    p95: 742.587,
  };

  // 2024-10-12
  // PB: Inserted 10 000 messages, took 0:00:07.759677 (limit=64)
  // const insertPocketbaseMicroS = {
  //   p50: 28160,
  //   p75: 58570,
  //   p90: 108325,
  //   p95: 157601,
  // };

  // 2025-02-05
  // PB: Inserted 10000 messages, took 0:00:04.245788 (limit=64)
  const insertPocketbaseMicroS = {
    p50: 22356,
    p75: 27123,
    p90: 49613,
    p95: 61512,
  };

  const latenciesMs = (d: Percentiles) =>
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
        data: latenciesMs(insertTrailbaseDartMicroS),
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
  //   PB: Inserted 100000 messages, took 0:00:44.702654 (limit=64)
  //   TB: Inserted 100000 messages, took 0:00:07.086891 (limit=64) (Dart AOT)
  //   TB: Inserted 100000 messages, took 00:00:05.7039362 (limit=64) (C#)
  //   TB: Inserted 100000 rows in 6.351865901s (rust)

  const trailbaseUtilization = insertTrailBase as Datum[];
  const pocketbaseUtilization = insertPocketBase as Datum[];

  const trailbaseTimeOffset = -0.35 * 1000;
  const pocketbaseTimeOffset = -1.3 * 1000;

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
          x: datum.elapsed + pocketbaseTimeOffset,
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
          x: datum.elapsed + pocketbaseTimeOffset,
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
          max: 7,
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
          max: 160 * 1024,
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
          max: 52 * 1000,
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
