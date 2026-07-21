import { createEffect, createSignal, Match, Show, Switch } from "solid-js";
import { A, useParams } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineArrowLeft } from "solid-icons/tb";

import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";

import { fetchWasmModules } from "@/lib/api/wasm-modules";

// Side-effect import: registers the `tb-html-fragment` custom element via
// `customElements.define`. Kept separate from the type-only import below so
// bundlers never elide it as an unused import (it has no value usage here).
import "./TbHtmlFragment";
import type { TbHtmlFragment } from "./TbHtmlFragment";

// export function WasmModuleSettingsIFramePage() {
//   const params = useParams<{ name: string }>();
//
//   const wasmModules = useQuery(() => ({
//     queryKey: ["wasm-modules"],
//     queryFn: fetchWasmModules,
//   }));
//
//
//   const module = () =>
//     wasmModules.data?.modules.find((m) => m.name === params.name);
//
//   const mod = module();
//   if (!mod) return;
//   const configPath = mod.config_path;
//
//   return (
//     <Switch>
//       <Match when={configPath}>
//         {(cp) => (
//           <iframe src={cp()} />
//         )}
//       </Match>
//
//       <Match when={true}>
//         Fallback
//       </Match>
//     </Switch>
//   );
// }

export function WasmModuleSettingsPage() {
  const params = useParams<{ name: string }>();

  const wasmModules = useQuery(() => ({
    queryKey: ["wasm-modules"],
    queryFn: fetchWasmModules,
    // refetchOnWindowFocus: false,
  }));

  const module = () =>
    wasmModules.data?.modules.find((m) => m.name === params.name);

  const [fragmentState, setFragmentState] = createSignal<
    | { status: "idle" }
    | { status: "loading" }
    | { status: "error"; message: string }
    | { status: "ready" }
  >({ status: "idle" });

  let fragmentRef: TbHtmlFragment | undefined;
  // Tracks the config_path that was last successfully injected so that
  // spurious re-runs of the effect (e.g. from an unrelated query refetch that
  // produces a new object reference) don't wipe and re-inject the fragment.
  let loadedConfigPath: string | undefined;

  createEffect(() => {
    const mod = module();
    if (!mod) return;
    const configPath = mod.config_path;
    if (!configPath) return;
    if (configPath === loadedConfigPath) return;

    setFragmentState({ status: "loading" });

    fetch(configPath, { credentials: "include" })
      .then((res) => {
        if (!res.ok) {
          throw new Error(`HTTP ${res.status}`);
        }
        return res.text();
      })
      .then((html) => {
        if (fragmentRef) {
          fragmentRef.html = html;
          loadedConfigPath = configPath;
          setFragmentState({ status: "ready" });
        }
      })
      .catch((err: unknown) => {
        const message =
          err instanceof Error ? err.message : "Failed to load settings";
        setFragmentState({ status: "error", message });
      });
  });

  const backLink = () => (
    <A
      href="/wasm-modules"
      class="text-muted-foreground hover:text-foreground flex items-center justify-center transition-colors"
      title="Back to WASM Modules"
    >
      <TbOutlineArrowLeft size={20} />
    </A>
  );

  const path = () =>
    import.meta.env.DEV
      ? `http://${window.location.hostname}:4000${module()?.config_path}`
      : module()?.config_path;

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
              No module named "{params.name}" is installed.
            </div>
          </Match>

          <Match when={!module()?.config_path}>
            <Header
              title={module()?.display_name ?? params.name}
              leading={backLink()}
            />
            <div class="text-muted-foreground p-4">
              This module has no settings page.
            </div>
          </Match>

          <Match when={module()?.config_path}>
            <Header title={module()!.display_name} leading={backLink()} />

            <iframe class="h-full w-full" src={path() ?? ""} />

            {/*
            <div class="p-4">
              <Show when={fragmentState().status === "loading"}>
                <div class="flex h-64 items-center justify-center">
                  <Spinner size={32} class="text-muted-foreground" />
                </div>
              </Show>

              <Show when={fragmentState().status === "error"}>
                <div class="text-destructive">
                  Failed to load settings:{" "}
                  {(fragmentState() as { status: "error"; message: string }).message}
                </div>
              </Show>

              <tb-html-fragment ref={(el) => (fragmentRef = el)} />
            </div>
            */}
          </Match>
        </Switch>
      </Show>
    </div>
  );
}
