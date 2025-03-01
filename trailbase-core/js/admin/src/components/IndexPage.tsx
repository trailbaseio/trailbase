import { For } from "solid-js";
import type { IconTypes } from "solid-icons";
import {
  TbDatabase,
  TbEdit,
  TbUsers,
  TbTimeline,
  TbSettings,
} from "solid-icons/tb";

import { Header } from "@/components/Header";

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
};

const elements = [
  {
    icon: TbDatabase,
    content: "Browse, create or alter your Tables, Indexes, and Views.",
  },
  {
    icon: TbEdit,
    content: "Untethered script access letting you execute arbitrary SQL.",
  },
  {
    icon: TbUsers,
    content: "Browse and manage your application's user registry.",
  },
  { icon: TbTimeline, content: "Access logs for your application" },
  { icon: TbSettings, content: "Server settings" },
] as Element[];

export function IndexPage() {
  return (
    <div class="h-dvh overflow-y-auto">
      <Header title="TrailBase" />

      <div class="prose m-4 grow">
        <p>
          Welcome to TrailBase 🚀: your open-source, sub-millisecond,
          single-executable FireBase alternative with type-safe APIs,
          notifications, builtin JS/TS runtime, auth &amp; admin UI built on
          SQLite, Rust &amp; V8.
        </p>

        <p>
          TrailBase is still young and evolving rapidly. You'd really help us
          out by leaving some feedback on{" "}
          <a href="https://github.com/trailbaseio/trailbase">GitHub</a> or even
          a ⭐, if you like it.
        </p>

        <p>
          Documentation is available at{" "}
          <a href="https://trailbase.io/getting-started/starting-up">
            trailbase.io
          </a>
          .
        </p>

        <p>
          Quick overview:
          <For each={elements}>
            {(item) => {
              const Icon = item.icon;
              return (
                <div class="ml-4 flex items-center gap-4">
                  <Icon size={20} /> {item.content}
                </div>
              );
            }}
          </For>
        </p>

        {import.meta.env.DEV && <ColorPalette />}
      </div>
    </div>
  );
}
