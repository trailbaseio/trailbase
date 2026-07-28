import { createEffect, onCleanup, Match, Switch } from "solid-js";
import { A } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineArrowLeft } from "solid-icons/tb";

import { Header } from "@/components/Header";
import { client, hostAddress } from "@/lib/client";
import { createIsMobile } from "@/lib/signals";
import { $tokens } from "@/lib/client";

import type { WasmComponent } from "@bindings/WasmComponent";
import { Tokens } from "trailbase";

export function WasmComponentDetails(props: { component: WasmComponent }) {
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

  let iframe: HTMLIFrameElement | undefined;

  createEffect(() => {
    let body = dashboardPage.data;
    if (body !== undefined) {
      if (iframe === undefined) {
        console.error("iframe not bound");
        return;
      }

      // NOTE: Dev-server-only hack to allow guest dashboard to be mounted when
      // the admin UI runs in a separate dev-server. W/o guest dashboards
      // would try to fetch their assets from the dev-server rather than TB.
      // This requires guests to be appropriately set up, however isn't generally
      // necessary unless you're also developing on the admin UI itself.
      // NOTE: We cannot just pass the base URI via `postMessage`, since static
      // assets referenced by the root document could not be fetched.
      if (import.meta.env.DEV) {
        body = body.replace(
          `base href=""`,
          `base href="http://${window.location.hostname}:4000/"`,
        );
      }

      let cleanup: (() => void) | undefined;
      const onLoad = (_ev: HTMLElementEventMap["load"]) => {
        // Will be called after `srcdoc` was set (below), then parsed and built.
        console.debug("iframe loaded");

        // NOTE: with the iframe sandbox, we cannot access `iframe.contentDocument`
        // directly to interact with globals in the child. It would be rejected as
        // a cross-origin request. We thus need postMessage.
        // NOTE: the `*` target is critical for sandboxed (different-origin)
        // iframes to avoid messages being rejected.
        cleanup = $tokens.subscribe((tokens) => {
          iframe.contentWindow?.postMessage(
            {
              type: "setup",
              value: {
                tokens: tokens !== null ? { ...tokens } : undefined,
                url: hostAddress(),
              },
            } satisfies Message,
            "*",
          );
        });
      };

      iframe.addEventListener("load", onLoad);
      onCleanup(() => cleanup?.());

      // Set the actual body.
      iframe.srcdoc = body;
    }
  });

  const backLink = () => (
    <A
      href="/wasm"
      class="text-muted-foreground hover:text-foreground flex items-center justify-center transition-colors"
      title="Back to the list of WASM components"
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
          This WASM component did not register a dashboard.
        </div>
      </Match>

      <Match when={true}>
        <Header
          title={props.component.display_name ?? props.component.name}
          leading={backLink()}
        />

        <div class={style()}>
          {/*
             NOTE: The `csp` attribute is not yet supported by Firefox & Safari:
               https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/csp
            TODO: Can we use something stricter like:
              csp="default-src 'none'; script-src 'unsafe-inline'"
          */}
          <iframe
            ref={iframe}
            style={{
              width: "100%",
              height: "100%",
              display: "block",
            }}
            sandbox="allow-scripts"
            csp={iframeCsp}
          />
        </div>
      </Match>
    </Switch>
  );
}

type SetupMessage = {
  type: "setup";
  value: {
    tokens?: Tokens;
    url?: string;
  };
};

type Message = SetupMessage;

const iframeCsp = import.meta.env.DEV
  ? ""
  : [
      "default-src 'self' 'unsafe-inline'",
      "style-src 'self' 'unsafe-inline'",
      "connect-src * 'self' 'unsafe-inline'",
    ].join("; ");
