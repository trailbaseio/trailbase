import { createEffect, Match, Show, Switch } from "solid-js";
import { A } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineArrowLeft } from "solid-icons/tb";

import { Header } from "@/components/Header";
import { Spinner } from "@/components/Spinner";
import { client } from "@/lib/client";
import { createIsMobile } from "@/lib/signals";

import type { WasmComponent } from "@bindings/WasmComponent";

export function WasmComponentDetails(props: { component: WasmComponent }) {
  let iframe: HTMLIFrameElement | undefined;

  const isMobile = createIsMobile();
  const style = () => {
    if (isMobile()) {
      // Header (65px) + Navbar (48px) = 113px
      return "h-[calc(100dvh-113px)] w-[calc(100dvw)]";
    }
    return "h-[calc(100dvh-65px)] w-[calc(100dvw-58px)]";
  };

  const source = () => {
    const path = props.component.admin_ui_path;
    if (!path) {
      return;
    }

    // Fix up for separate dev server.
    return import.meta.env.DEV
      ? `http://${window.location.hostname}:4000${path}`
      : path;
  };

  const dashboardPage = useQuery(() => ({
    queryKey: ["wasm-dash", props.component.admin_ui_path],
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

      // NOTE: Ultra hacky, this requires the guest UI to be set-up appropriately,
      // however this is only relevant for components that should work in a
      // separate dev server for the admin UI.
      if (import.meta.env.DEV) {
        body = body.replace(
          `base href=""`,
          `base href="http://${window.location.hostname}:4000/"`,
        );
      }

      iframe.onload = (_ev) => {
        // Will be called after `srcdoc` was parsed and built.
        console.debug("loaded", iframe.contentDocument);
      };
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
    <Switch>
      <Match when={props.component.admin_ui_path === undefined}>
        <Header
          title={props.component.display_name ?? props.component.name}
          leading={backLink()}
        />
        <div class="text-muted-foreground p-4">
          This module has no settings page.
        </div>
      </Match>

      <Match when={true}>
        <Header
          title={props.component.display_name ?? props.component.name}
          leading={backLink()}
        />

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
  );
}
