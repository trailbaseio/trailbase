import { For } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import type { IconTypes } from "solid-icons";
import {
  TbOutlineDatabase,
  TbOutlineEdit,
  TbOutlineChartDots3,
  TbOutlineUsers,
  TbOutlinePackage,
  TbOutlineTimeline,
  TbOutlineApi,
  TbOutlineSettings,
} from "solid-icons/tb";

import { executeSql } from "@/lib/api/execute";
import type { SqlValue } from "@/lib/value";

import { Header } from "@/components/Header";
import { Card, CardContent, CardTitle } from "@/components/ui/card";

type Element = {
  icon: IconTypes;
  content: string;
  href: string;
};

const BASE = import.meta.env.BASE_URL;
const elements = [
  {
    icon: TbOutlineDatabase,
    content: "Browse, create or alter your Tables, Indexes, and Views.",
    href: `${BASE}/table`,
  },
  {
    icon: TbOutlineEdit,
    content: "Untethered script access letting you execute arbitrary SQL.",
    href: `${BASE}/editor`,
  },
  {
    icon: TbOutlineChartDots3,
    content: "Visualize Database Schema as Entity-Relationship-Diagram",
    href: `${BASE}/erd`,
  },
  {
    icon: TbOutlineUsers,
    content: "Browse and manage your application's user registry.",
    href: `${BASE}/auth`,
  },
  {
    icon: TbOutlinePackage,
    content: "Loaded WASM modules",
    href: `${BASE}/wasm`,
  },
  {
    icon: TbOutlineTimeline,
    content: "Access logs for your application",
    href: `${BASE}/logs`,
  },
  {
    icon: TbOutlineApi,
    content: "OpenAPI documentation",
    href: `${BASE}/openapi`,
  },
  {
    icon: TbOutlineSettings,
    content: "Server settings",
    href: `${BASE}/settings`,
  },
] as Element[];

type Data = {
  dbSize: bigint;
  numTables: bigint;
  numViews: bigint;
  numUsers: bigint;
};

function FactCard(props: { title: string; content: string; href?: string }) {
  const FCard = () => (
    <Card class="h-full">
      <CardContent>
        <CardTitle>{props.title}</CardTitle>

        <div class="text-muted-foreground text-xl font-bold">
          {props.content}
        </div>
      </CardContent>
    </Card>
  );

  return (
    <>
      {props.href ? (
        <a class="grow no-underline" href={props.href}>
          <FCard />
        </a>
      ) : (
        <FCard />
      )}
    </>
  );
}

async function fetchDashboardData(): Promise<Data> {
  const sql = `
    SELECT
      page_count * page_size, num_tables, num_views, num_users
    FROM
      pragma_page_count AS page_count,
      pragma_page_size AS page_size,
      (SELECT COUNT(*) AS num_tables FROM sqlite_master WHERE type = 'table'),
      (SELECT COUNT(*) AS num_views FROM sqlite_master WHERE type = 'view'),
      (SELECT COUNT(*) AS num_users FROM _user);`;

  const response = await executeSql(sql, null);
  const error = response.error;
  if (error) {
    throw Error(JSON.stringify(error));
  }

  const data = response.data;
  if (!data || data.rows.length < 1) {
    throw Error(`Missing data: ${data}`);
  }
  const row = data.rows[0];
  return {
    dbSize: castToInteger(row[0]),
    numTables: castToInteger(row[1]),
    numViews: castToInteger(row[2]),
    numUsers: castToInteger(row[3]),
  } as Data;
}

export function IndexPage() {
  const dashboardFetch = useQuery(() => ({
    queryKey: ["dashboard"],
    queryFn: fetchDashboardData,
  }));

  return (
    <div>
      <Header title="TrailBase" />

      <div class="prose dark:prose-invert flex grow flex-col gap-4 p-4">
        {dashboardFetch.isLoading ? (
          <div class="text-muted-foreground text-sm" role="status">
            Loading dashboard metrics…
          </div>
        ) : dashboardFetch.error ? (
          <div class="text-destructive text-sm" role="alert">
            Unable to load dashboard metrics.
          </div>
        ) : dashboardFetch.data ? (
          <div class="flex shrink gap-4">
            <FactCard
              title="Users"
              content={`${dashboardFetch.data!.numUsers}`}
              href={`${BASE}/auth`}
            />
            <FactCard
              title="Tables & Views"
              content={`${dashboardFetch.data!.numTables + dashboardFetch.data!.numViews}`}
              href={`${BASE}/table`}
            />
            <FactCard
              title="Size"
              content={formatBytes(Number(dashboardFetch?.data.dbSize ?? 0))}
            />
          </div>
        ) : (
          <div class="text-muted-foreground text-sm">
            No dashboard metrics available.
          </div>
        )}

        <Card>
          <CardContent>
            <CardTitle>Welcome to TrailBase 🚀</CardTitle>

            <p>
              Your open, sub-millisecond, single-executable FireBase alternative
              with type-safe APIs, notifications, builtin WebAssembly runtime,
              auth &amp; admin UI built on SQLite, Rust &amp; Wasmtime.
            </p>

            <p>
              TrailBase is still young and evolving rapidly. You'd really help
              us out by leaving some feedback on{" "}
              <a href="https://github.com/trailbaseio/trailbase">GitHub</a> or
              even a ⭐, if you like it.
            </p>

            <p>
              Documentation is available at{" "}
              <a href="https://trailbase.io/docs">trailbase.io</a>.
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardContent>
            <CardTitle>Quick Reference</CardTitle>

            <For each={elements}>
              {(item) => {
                const Icon = item.icon;
                return (
                  <a
                    class="ml-4 flex items-center gap-4 font-normal no-underline"
                    href={item.href}
                  >
                    <Icon size={20} /> {item.content}
                  </a>
                );
              }}
            </For>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function castToInteger(value: SqlValue): bigint {
  if (typeof value === "object" && "Integer" in value) {
    return value.Integer;
  }
  throw Error(`Expected integer, got: ${value}`);
}

function formatBytes(bytes: number, decimals: number = 0) {
  const k = 1024;

  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const value: string = (bytes / Math.pow(k, i)).toFixed(
    decimals < 0 ? 0 : decimals,
  );

  return `${value} ${suffixes[i]}`;
}

const suffixes = ["Bytes", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
