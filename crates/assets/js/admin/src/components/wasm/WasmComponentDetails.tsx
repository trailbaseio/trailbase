import { createEffect, Match, Show, Switch } from "solid-js";
import { A } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineArrowLeft } from "solid-icons/tb";

import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";
import { client } from "@/lib/client";
import { createIsMobile } from "@/lib/signals";

import { fetchWasmModules } from "@/lib/api/wasm-modules";

export function WasmComponentDetails(props: { name: string }) {
  let iframe: HTMLIFrameElement | undefined;

  const isMobile = createIsMobile();
  const style = () => {
    if (isMobile()) {
      // Header (65px) + Navbar (48px) = 113px
      return "h-[calc(100dvh-113px)] w-[calc(100dvw)]";
    }
    return "h-[calc(100dvh-65px)] w-[calc(100dvw-58px)]";
  };

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

  const source = () => {
    const path = configPath();

    if (!path) {
      return;
    }
    return import.meta.env.DEV
      ? `http://${window.location.hostname}:4000${path}`
      : path;
  };

  const dashboardPage = useQuery(() => ({
    queryKey: ["wasm-dash", configPath()],
    queryFn: async ({ queryKey: _ }) => {
      const src = source();
      if (!src) {
        return;
      }

      const response = await fetch(src, { headers: client.headers() });
      return await response.text();
    },
  }));

  createEffect(() => {
    let body = dashboardPage.data;
    if (body !== undefined) {
      if (iframe === undefined) {
        console.error("iframe not bound");
        return;
      }

      // FIXME: Ultra hacky, requires the guest app to be set-up appropriately.
      // That said, should only be required in DEV mode :/
      if (import.meta.env.DEV) {
        const x = `<base href="http://${window.location.hostname}:4000/" />`;
        body = body.replace(`<base href="" />`, x);
      }

      iframe.onload = (_ev) => {
        console.log("loaded", iframe.contentDocument);
      }
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

            <div class={style()}>
              <iframe
                ref={iframe}
                style={{
                  width: "100%",
                  height: "100%",
                  display: "block",
                }}
              />
            </div>
          </Match>
        </Switch>
      </Show>
    </div>
  );
}
