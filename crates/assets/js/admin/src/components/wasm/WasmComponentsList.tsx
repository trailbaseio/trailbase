import { createMemo, For, Match, Show, Switch } from "solid-js";
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

import { listWasmComponents } from "@/lib/api/wasm-components";
import type { WasmComponent } from "@bindings/WasmComponent";

function ComponentIcon(props: { icon?: string }) {
  const icon = createMemo(() => props.icon?.trim());

  // Inline SVGs avoid <img> to keep 'em work with background colors.
  const buildSvg = (icon: string) => {
    return template(icon.trim())();
  };

  return (
    <Switch>
      <Match when={icon()?.startsWith("<svg") ? icon() : undefined}>
        {(icon) => <div class="size-6 [&>svg]:size-6">{buildSvg(icon())}</div>}
      </Match>

      <Match when={icon()?.startsWith("data:") ? icon() : undefined}>
        {(icon) => <img src={icon()} class="size-6" />}
      </Match>

      <Match when={true}>
        <TbOutlinePuzzle size={24} />
      </Match>
    </Switch>
  );
}

function ComponentCard(props: { component: WasmComponent }) {
  const displayName = () =>
    props.component.display_name ?? props.component.name;

  return (
    <Card>
      <CardContent class="flex p-4">
        <div class="text-muted-foreground flex size-10 shrink-0 items-center justify-center">
          <ComponentIcon icon={props.component.icon ?? undefined} />
        </div>

        <div class="flex w-full gap-2">
          <div class="flex grow flex-col justify-start">
            <div class="flex h-full items-center gap-2">
              <CardTitle>{displayName()}</CardTitle>

              <Show
                when={props.component.display_name !== props.component.name}
              >
                <span class="text-muted-foreground shrink-0 text-xs">
                  {props.component.name}
                </span>
              </Show>
            </div>

            <Show when={props.component.description}>
              <CardDescription>{props.component.description}</CardDescription>
            </Show>
          </div>

          <Show when={props.component.admin_ui_path !== null}>
            <A
              href={`/wasm/${props.component.name}`}
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
  const wasmComponents = useQuery(() => ({
    queryKey: ["wasm-components"],
    queryFn: listWasmComponents,
  }));

  const components = () => wasmComponents.data?.components ?? [];

  return (
    <div>
      <Header title="WASM Components" />

      <div class="flex flex-col gap-3 p-4">
        <Show
          when={!wasmComponents.isLoading}
          fallback={
            <div class="flex h-64 items-center justify-center">
              <Spinner size={32} class="text-muted-foreground" />
            </div>
          }
        >
          <Show
            when={components().length > 0}
            fallback={
              <div class="text-muted-foreground flex h-64 flex-col items-center justify-center gap-2">
                <TbOutlinePackage size={48} />
              </div>
            }
          >
            <For each={components()}>
              {(c) => <ComponentCard component={c} />}
            </For>
          </Show>
        </Show>
      </div>
    </div>
  );
}
