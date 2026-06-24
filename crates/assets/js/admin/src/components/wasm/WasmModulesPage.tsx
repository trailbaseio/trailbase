import { createMemo, For, Show } from "solid-js";
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
  const src = (): string | undefined => {
    const icon = props.icon;
    if (icon === undefined) {
      return undefined;
    }
    if (icon.trimStart().startsWith("<svg")) {
      return `data:image/svg+xml;utf8,${encodeURIComponent(icon)}`;
    }
    if (icon.startsWith("data:")) {
      return icon;
    }
    return undefined;
  };

  return (
    <Show
      when={src() !== undefined}
      fallback={<TbOutlinePuzzle size={24} />}
    >
      <img src={src()!} alt="" class="size-6" />
    </Show>
  );
}

function ModuleCard(props: { module: WasmModuleEntry }) {
  const hasManifest = createMemo(
    () =>
      props.module.display_name !== props.module.name ||
      props.module.icon !== null,
  );

  return (
    <div class="flex items-center gap-3 rounded-lg border border-border p-4">
      <div class="flex size-10 shrink-0 items-center justify-center text-muted-foreground">
        <ModuleIcon icon={props.module.icon ?? undefined} />
      </div>

      <div class="min-w-0 flex-1">
        <div class="flex items-baseline gap-2">
          <h3 class="truncate font-medium">{props.module.display_name}</h3>
          <Show when={hasManifest()}>
            <span class="shrink-0 text-xs text-muted-foreground">
              {props.module.name}
            </span>
          </Show>
        </div>
        <Show when={props.module.description}>
          <p class="mt-0.5 line-clamp-2 text-sm text-muted-foreground">
            {props.module.description}
          </p>
        </Show>
      </div>

      <Show when={props.module.config_path !== null}>
        <A
          href={`/wasm-modules/${props.module.name}`}
          class="flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
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
              <div class="flex h-64 flex-col items-center justify-center gap-2 text-muted-foreground">
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
