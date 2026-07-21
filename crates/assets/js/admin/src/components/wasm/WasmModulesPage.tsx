import { createMemo, For, Match, Show, Switch } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { A } from "@solidjs/router";
import {
  TbOutlinePackage,
  TbOutlinePuzzle,
  TbOutlineSettings,
} from "solid-icons/tb";

import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";

import { fetchWasmModules } from "@/lib/api/wasm-modules";

import type { WasmModuleEntry } from "@bindings/WasmModuleEntry";

function ModuleIcon(props: { icon?: string }) {
  const icon = () => props.icon?.trim();

  // Inline SVGs are injected via innerHTML so `currentColor` inherits from
  // the parent; loading through <img> isolates the SVG and breaks theming.
  return (
    <Switch fallback={<TbOutlinePuzzle size={24} />}>
      <Match when={icon()?.startsWith("<svg") ? props.icon : undefined}>
        {(icon) => <div innerHTML={icon()} class="[&>svg]:size-6" />}
      </Match>

      <Match when={icon()?.startsWith("data:") ? props.icon : undefined}>
        {(icon) => <img src={icon()} alt="" class="size-6" />}
      </Match>
    </Switch>
  );
}

function ModuleCard(props: { module: WasmModuleEntry }) {
  const hasManifest = createMemo(
    () =>
      props.module.display_name !== props.module.name ||
      props.module.icon !== null,
  );

  // FIXME: should use Card component.
  return (
    <div class="border-border flex items-center gap-3 rounded-lg border p-4">
      <div class="text-muted-foreground flex size-10 shrink-0 items-center justify-center">
        <ModuleIcon icon={props.module.icon ?? undefined} />
      </div>

      <div class="min-w-0 flex-1">
        <div class="flex items-baseline gap-2">
          <h3 class="truncate font-medium">{props.module.display_name}</h3>
          <Show when={hasManifest()}>
            <span class="text-muted-foreground shrink-0 text-xs">
              {props.module.name}
            </span>
          </Show>
        </div>
        <Show when={props.module.description}>
          <p class="text-muted-foreground mt-0.5 line-clamp-2 text-sm">
            {props.module.description}
          </p>
        </Show>
      </div>

      <Show when={props.module.config_path !== null}>
        <A
          href={`/wasm-modules/${props.module.name}`}
          class="text-muted-foreground hover:bg-accent hover:text-accent-foreground flex size-8 shrink-0 items-center justify-center rounded-md transition-colors"
        >
          <TbOutlineSettings size={18} />
        </A>
      </Show>
    </div>
  );
}

export function WasmModulesPage() {
  const wasmModules = useQuery(() => ({
    queryKey: ["wasm-modules"],
    queryFn: fetchWasmModules,
  }));

  const modules = createMemo(() => wasmModules.data?.modules ?? []);

  return (
    <div>
      <Header title="WASM Modules" />

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
              {(module) => <ModuleCard module={module} />}
            </For>
          </Show>
        </Show>
      </div>
    </div>
  );
}
