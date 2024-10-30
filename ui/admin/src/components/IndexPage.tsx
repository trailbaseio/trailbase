import { For } from "solid-js";
import type { IconTypes } from "solid-icons";
import {
  TbDatabase,
  TbEdit,
  TbUsers,
  TbTimeline,
  TbSettings,
} from "solid-icons/tb";

import { Separator } from "@/components/ui/separator";

function ColorPalette() {
  return (
    <div class="w-56 grid grid-cols-2 m-4">
      <div class="bg-background">Background</div>
      <div class="bg-foreground text-white">Foreground</div>

      <div class="bg-muted">Muted</div>
      <div class="bg-muted-foreground text-white">Muted FG</div>

      <div class="bg-border -center">Border</div>
      <div class="bg-border-input">Border Input</div>

      <div class="bg-card -center">Card</div>
      <div class="bg-card-foreground -input text-white">Card FG</div>

      <div class="bg-primary text-white">Primary</div>
      <div class="bg-primary-foreground">Primary FG</div>

      <div class="bg-secondary-content-center">Secondary</div>
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
    <>
      <h1 class="m-4">Welcome to TrailBase</h1>

      <Separator />

      <div class="flex flex-col md:flex-row ">
        <div class="grow m-4 prose flex flex-col gap-4">
          <span>Quick overview:</span>

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

          <span>
            TrailBase is at an early stage. Help us out and go to{" "}
            <a href="https://github.com/trailbaseio/trailbase">GitHub</a> to
            find open issues or report new ones. The full documentation is
            available on <a href="https://trailbase.io">trailbase.io</a>
          </span>
        </div>

        {import.meta.env.DEV && <ColorPalette />}
      </div>
    </>
  );
}
