import { createEffect, Match, Show, Switch } from "solid-js";
import { A } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineArrowLeft } from "solid-icons/tb";

import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";
import { client } from "@/lib/client";

import { fetchWasmModules } from "@/lib/api/wasm-modules";

export function WasmComponentDetails(props: { name: string }) {
  const wasmModules = useQuery(() => ({
    queryKey: ["wasm-components"],
    queryFn: fetchWasmModules,
    // refetchOnWindowFocus: false,
  }));

  const module = () =>
    wasmModules.data?.modules.find((m) => m.name === props.name);

  const configPath = () => {
    const mod = module();
    if (!mod) return;
    return mod.config_path;
  };

  const dashboardPage = useQuery(() => ({
    queryKey: ["wasm-dash", configPath()],
    queryFn: async ({ queryKey: _ }) => {
      const path = configPath();

      if (!path) {
        return;
      }

      const p = import.meta.env.DEV
        ? `http://${window.location.hostname}:4000${path}`
        : path;

      const response = await fetch(p, { headers: client.headers() });
      return await response.text();
    },
  }));

  createEffect(() => {
    const body = dashboardPage.data;
    if (body !== undefined) {
      const iframe = document.getElementById("foobar")! as HTMLIFrameElement;
      iframe.srcdoc = body;
    }
  });

  const backLink = () => (
    <A
      href="/wasm"
      class="text-muted-foreground hover:text-foreground flex items-center justify-center transition-colors"
      title="Back to WASM Modules"
    >
      <TbOutlineArrowLeft size={20} />
    </A>
  );

  return (
    <div>
      <Show
        when={!wasmModules.isLoading}
        fallback={
          <div class="flex h-64 items-center justify-center">
            <Spinner size={32} class="text-muted-foreground" />
          </div>
        }
      >
        <Switch>
          <Match when={module() === undefined}>
            <Header title="Module not found" leading={backLink()} />
            <div class="text-muted-foreground p-4">
              No module named "{props.name}" is installed.
            </div>
          </Match>

          <Match when={!module()?.config_path}>
            <Header
              title={module()?.display_name ?? props.name}
              leading={backLink()}
            />
            <div class="text-muted-foreground p-4">
              This module has no settings page.
            </div>
          </Match>

          <Match when={module()?.config_path}>
            <Header title={module()!.display_name} leading={backLink()} />

            <iframe id="foobar" class="h-full w-full" />
          </Match>
        </Switch>
      </Show>
    </div>
  );
}
