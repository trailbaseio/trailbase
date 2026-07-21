import { For, Match, Show, Switch } from "solid-js";
import { template } from "solid-js/web";
import { useQuery } from "@tanstack/solid-query";
import { A } from "@solidjs/router";
import {
  TbOutlinePackage,
  TbOutlinePuzzle,
  TbOutlineSettings,
} from "solid-icons/tb";

import {
  Card,
  CardContent,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";

// TODO: Rename to components.
import { fetchWasmModules } from "@/lib/api/wasm-modules";
import type { WasmModuleEntry } from "@bindings/WasmModuleEntry";

function ComponentIcon(props: { icon?: string }) {
  const icon = () => props.icon?.trim();

  // Inline SVGs are injected via innerHTML so `currentColor` inherits from
  // the parent; loading through <img> isolates the SVG and breaks theming.
  return (
    <Switch fallback={<TbOutlinePuzzle size={24} />}>
      <Match when={icon()?.startsWith("<svg") ? props.icon : undefined}>
        {/*
        {(icon) => <div innerHTML={icon()} class="[&>svg]:size-6" />}
        */}
        {(icon) => {
          return <div class="size-6 [&>svg]:size-6">{template(icon())()}</div>;
        }}
      </Match>

      <Match when={icon()?.startsWith("data:") ? props.icon : undefined}>
        {(icon) => <img src={icon()} class="size-6" />}
      </Match>
    </Switch>
  );
}

function ComponentCard(props: { module: WasmModuleEntry }) {
  const hasManifest = () =>
    props.module.display_name !== props.module.name ||
    props.module.icon !== null;

  return (
    <Card>
      <CardContent class="flex p-4">
        <div class="text-muted-foreground flex size-10 shrink-0 items-center justify-center">
          <ComponentIcon icon={props.module.icon ?? undefined} />
        </div>

        <div class="flex w-full gap-2">
          <div class="flex grow flex-col justify-start">
            <div class="flex h-full items-center gap-2">
              <CardTitle>{props.module.display_name}</CardTitle>

              <Show when={hasManifest()}>
                <span class="text-muted-foreground shrink-0 text-xs">
                  {props.module.name}
                </span>
              </Show>
            </div>

            <Show when={props.module.description}>
              <CardDescription>{props.module.description}</CardDescription>
            </Show>
          </div>

          <Show when={props.module.config_path !== null}>
            <A
              href={`/wasm/${props.module.name}`}
              class="text-muted-foreground hover:bg-accent hover:text-accent-foreground flex size-8 shrink-0 items-center justify-center rounded-md transition-colors"
            >
              <TbOutlineSettings size={18} />
            </A>
          </Show>
        </div>
      </CardContent>
    </Card>
  );
}

export function WasmComponentsList() {
  const wasmModules = useQuery(() => ({
    queryKey: ["wasm-components"],
    queryFn: fetchWasmModules,
  }));

  const modules = () => wasmModules.data?.modules ?? [];

  return (
    <div>
      <Header title="WASM Components" />

      <div class="flex flex-col gap-3 p-4">
        <Show
          when={!wasmModules.isLoading}
          fallback={
            <div class="flex h-64 items-center justify-center">
              <Spinner size={32} class="text-muted-foreground" />
            </div>
          }
        >
          <Show
            when={modules().length > 0}
            fallback={
              <div class="text-muted-foreground flex h-64 flex-col items-center justify-center gap-2">
                <TbOutlinePackage size={48} />
              </div>
            }
          >
            <For each={modules()}>
              {(module) => <ComponentCard module={module} />}
            </For>
          </Show>
        </Show>
      </div>
    </div>
  );
}
