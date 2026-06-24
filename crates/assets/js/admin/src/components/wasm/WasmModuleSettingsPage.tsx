import { createEffect, createSignal, Match, Show, Switch } from "solid-js";
import { A, useParams } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineArrowLeft } from "solid-icons/tb";

import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";

import { fetchWasmModules } from "@/lib/api/wasm-modules";

// Injects an HTML fragment string into a container element.
// Script tags in the fragment do not execute when set via innerHTML; this
// function clones each script element so the browser treats it as new and
// executes it. Non-script nodes are inserted as-is.
function injectFragment(container: HTMLDivElement, html: string): void {
  container.innerHTML = html;
  container.querySelectorAll("script").forEach((old) => {
    const next = document.createElement("script");
    Array.from(old.attributes).forEach((attr) => {
      next.setAttribute(attr.name, attr.value);
    });
    next.textContent = old.textContent;
    old.parentNode?.replaceChild(next, old);
  });
}

export function WasmModuleSettingsPage() {
  const params = useParams<{ name: string }>();

  const wasmModules = useQuery(() => ({
    queryKey: ["wasm-modules"],
    queryFn: fetchWasmModules,
    // The module list doesn't change on its own during an admin session;
    // refetching on window focus would re-inject the settings fragment and
    // reset any unsaved state the user may have entered.
    refetchOnWindowFocus: false,
  }));

  const module = () =>
    wasmModules.data?.modules.find((m) => m.name === params.name);

  const [fragmentState, setFragmentState] = createSignal<
    | { status: "idle" }
    | { status: "loading" }
    | { status: "error"; message: string }
    | { status: "ready" }
  >({ status: "idle" });

  let containerRef: HTMLDivElement | undefined;
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
        if (containerRef) {
          injectFragment(containerRef, html);
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
      class="flex items-center justify-center text-muted-foreground transition-colors hover:text-foreground"
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
            <div class="p-4 text-muted-foreground">
              No module named "{params.name}" is installed.
            </div>
          </Match>

          <Match when={!module()?.config_path}>
            <Header
              title={module()?.display_name ?? params.name}
              leading={backLink()}
            />
            <div class="p-4 text-muted-foreground">
              This module has no settings page.
            </div>
          </Match>

          <Match when={module()?.config_path}>
            <Header title={module()!.display_name} leading={backLink()} />

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

              <div ref={containerRef} />
            </div>
          </Match>
        </Switch>
      </Show>
    </div>
  );
}
