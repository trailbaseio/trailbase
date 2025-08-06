import { For } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import type { IconTypes } from "solid-icons";
import {
  TbDatabase,
  TbEdit,
  TbChartDots3,
  TbUsers,
  TbTimeline,
  TbSettings,
} from "solid-icons/tb";

import { executeSql } from "@/lib/fetch";

import { Header } from "@/components/Header";
import { Card, CardContent, CardTitle } from "@/components/ui/card";

function ColorPalette() {
  return (
    <div class="my-4 grid w-[400px] grid-cols-2 text-sm">
      <div class="bg-background">Background</div>
      <div class="bg-foreground text-white">Foreground</div>

      <div class="bg-muted">Muted</div>
      <div class="bg-muted-foreground text-white">Muted FG</div>

      <div class="bg-border">Border</div>
      <div>N/A</div>

      <div class="bg-card">Card</div>
      <div class="bg-card-foreground text-white">Card FG</div>

      <div class="bg-primary text-white">Primary</div>
      <div class="bg-primary-foreground">Primary FG</div>

      <div class="bg-secondary">Secondary</div>
      <div class="bg-secondary-foreground text-white">Secondary FG</div>

      <div class="bg-accent">Accent</div>
      <div class="bg-accent-foreground text-white">Accent FG</div>

      <div class="bg-destructive">Destructive</div>
      <div class="bg-destructive-foreground">Destructive FG</div>

      <div class="bg-info">info</div>
      <div class="bg-info-foreground text-white">info FG</div>

      <div class="bg-success">success</div>
      <div class="bg-success-foreground text-white">success FG</div>

      <div class="bg-warning">warning</div>
      <div class="bg-warning-foreground text-white">warning FG</div>

      <div class="bg-error">error</div>
      <div class="bg-error-foreground text-white">error FG</div>

      <div class="bg-ring text-white">Ring</div>
    </div>
  );
}

type Element = {
  icon: IconTypes;
  content: string;
  href: string;
};

const BASE = import.meta.env.BASE_URL;
const elements = [
  {
    icon: TbDatabase,
    content: "Browse, create or alter your Tables, Indexes, and Views.",
    href: `${BASE}/table`,
  },
  {
    icon: TbEdit,
    content: "Untethered script access letting you execute arbitrary SQL.",
    href: `${BASE}/editor`,
  },
  {
    icon: TbChartDots3,
    content: "Visualize Database Schema as Entity-Relationship-Diagram",
    href: `${BASE}/erd`,
  },
  {
    icon: TbUsers,
    content: "Browse and manage your application's user registry.",
    href: `${BASE}/auth`,
  },
  {
    icon: TbTimeline,
    content: "Access logs for your application",
    href: `${BASE}/logs`,
  },
  {
    icon: TbSettings,
    content: "Server settings",
    href: `${BASE}/settings`,
  },
] as Element[];

type Data = {
  dbSize: number;
  numTables: number;
  numViews: number;
  numUsers: number;
};

function FactCard(props: { title: string; content: string; href?: string }) {
  const FCard = () => (
    <Card class="grow">
      <CardContent>
        <CardTitle>{props.title}</CardTitle>

        <div class="text-xl font-bold text-primary">{props.content}</div>
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

  const response = await executeSql(sql);
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
    dbSize: row[0] as number,
    numTables: row[1] as number,
    numViews: row[2] as number,
    numUsers: row[3] as number,
  } as Data;
}

export function IndexPage() {
  const dashboardFetch = useQuery(() => ({
    queryKey: ["dashboard"],
    queryFn: fetchDashboardData,
  }));

  return (
    <div class="h-dvh overflow-y-auto">
      <Header title="TrailBase" />

      <div class="prose m-4 flex grow flex-col gap-4">
        {dashboardFetch.data && (
          <div class="flex grow gap-4">
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
              content={`${(dashboardFetch.data!.dbSize / 1024 / 1024).toPrecision(2)} MB`}
            />
          </div>
        )}

        <Card>
          <CardContent>
            <CardTitle>Welcome to TrailBase 🚀</CardTitle>

            <p>
              Your open-source, sub-millisecond, single-executable FireBase
              alternative with type-safe APIs, notifications, builtin JS/TS
              runtime, auth &amp; admin UI built on SQLite, Rust &amp; V8.
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

        {import.meta.env.DEV && <ColorPalette />}
      </div>
    </div>
  );
}
