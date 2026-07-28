import { createMemo, For, JSXElement, Match, Show, Switch } from "solid-js";
import { template } from "solid-js/web";
import { useQuery } from "@tanstack/solid-query";
import { A } from "@solidjs/router";
import { TbOutlinePuzzle, TbOutlineSettings } from "solid-icons/tb";

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
  const component = () => props.component;
  const displayName = () => component().display_name ?? component().name;

  const WrapHyperlink = (props: { children: JSXElement }) => {
    return (
      <Switch>
        <Match when={component().admin_ui_path}>
          <A href={`/wasm/${component().name}`}>{props.children}</A>
        </Match>

        <Match when={true}>{props.children}</Match>
      </Switch>
    );
  };

  return (
    <Card>
      <WrapHyperlink>
        <CardContent class="flex bg-transparent p-4">
          <div class="text-muted-foreground size-10 shrink-0 content-center">
            <ComponentIcon icon={props.component.icon ?? undefined} />
          </div>

          <div class="flex w-full gap-2">
            <div class="flex grow flex-col justify-start">
              <div class="flex h-full items-center gap-2">
                <CardTitle>{displayName()}</CardTitle>

                <Show when={displayName() !== props.component.name}>
                  <span class="text-muted-foreground text-xs">
                    {props.component.name}
                  </span>
                </Show>
              </div>

              <Show when={props.component.description}>
                <CardDescription>{props.component.description}</CardDescription>
              </Show>
            </div>

            <Show when={props.component.admin_ui_path}>
              <div class="text-muted-foreground hover:bg-accent hover:text-accent-foreground content-center rounded-sm p-2">
                <TbOutlineSettings size={18} />
              </div>
            </Show>
          </div>
        </CardContent>
      </WrapHyperlink>
    </Card>
  );
}

export function WasmComponentsList() {
  const wasmComponents = useQuery(() => ({
    queryKey: ["wasm-components"],
    queryFn: listWasmComponents,
  }));

  const components = (): WasmComponent[] => {
    const components = [...(wasmComponents.data?.components ?? [])];
    if (import.meta.env.DEV) {
      components.push({
        name: "[DEV]injected_debug_default",
      });
    }
    return components;
  };

  return (
    <div>
      <Header title="WASM Components" />

      <div class="flex flex-col gap-3 p-4">
        <Switch>
          <Match when={wasmComponents.isLoading}>
            <div class="flex h-64 items-center justify-center">
              <Spinner size={32} class="text-muted-foreground" />
            </div>
          </Match>

          <Match when={wasmComponents.isError}>
            {`${wasmComponents.error}`}
          </Match>

          <Match when={wasmComponents.isSuccess}>
            <For each={components()}>
              {(c) => <ComponentCard component={c} />}
            </For>
          </Match>
        </Switch>
      </div>
    </div>
  );
}
