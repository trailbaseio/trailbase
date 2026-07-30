import {
  createEffect,
  createSignal,
  onCleanup,
  Match,
  Switch,
  Show,
} from "solid-js";
import { A } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineArrowLeft, TbOutlineSandbox } from "solid-icons/tb";

import { Header } from "@/components/Header";
import { Button } from "@/components/ui/button";

import { client, hostAddress } from "@/lib/client";
import { createIsMobile } from "@/lib/signals";
import { $tokens } from "@/lib/client";
import { type ResolvedTheme, currentTheme } from "@/lib/theme";

import type { WasmComponent } from "@bindings/WasmComponent";
import { Tokens } from "trailbase";

export function WasmComponentDetails(props: { component: WasmComponent }) {
  const isMobile = createIsMobile();
  const [enableSandbox, setEnableSandbox] = createSignal<boolean>(true);

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

      // QUESTION: Should we browser fingerprint and use a different sandbox or
      // simply offer a button to toggle sandbox?
      if (enableSandbox()) {
        iframe.sandbox = defaultSandbox;
      } else {
        iframe.sandbox = "";
      }

      if (import.meta.env.DEV) {
        // NOTE: Dev-server-only hack to allow guest dashboard to be mounted when
        // the admin UI runs in a separate dev-server. W/o guest dashboards
        // would try to fetch their assets from the dev-server rather than TB.
        // This requires guests to be appropriately set up, however isn't generally
        // necessary unless you're also developing on the admin UI itself.
        // NOTE: We cannot just pass the base URI via `postMessage`, since static
        // assets referenced by the root document could not be fetched.
        body = body.replace(
          `base href=""`,
          `base href="http://${window.location.hostname}:4000/"`,
        );
      }

      let cleanup: (() => void) | undefined;
      const onLoad = (_ev: HTMLElementEventMap["load"]) => {
        // Will be called after `srcdoc` was set (below), then parsed and built.
        console.debug("iframe loaded");

        // Focus the iframe so it can receive keyboard events.
        iframe.focus();

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
                theme: currentTheme(),
              },
            } satisfies Message,
            "*",
          );
        });

        // TODO: Subscribe to theme changes and send a dedicated "theme" message.
      };

      iframe.addEventListener("load", onLoad);
      onCleanup(() => cleanup?.());

      // Set the actual body.
      iframe.srcdoc = body;
    }
  });

  const BackButton = () => (
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
          leading={BackButton()}
        />
        <div class="text-muted-foreground p-4">
          This WASM component did not register a dashboard.
        </div>
      </Match>

      <Match when={true}>
        <Header
          title={props.component.display_name ?? props.component.name}
          leading={BackButton()}
          right={
            <Show when={import.meta.env.DEV}>
              <Button
                size="icon"
                variant="outline"
                onClick={() => {
                  setEnableSandbox((old) => {
                    const toggled = !old;
                    console.debug("sandbox enabled:", toggled);
                    return toggled;
                  });
                }}
              >
                <TbOutlineSandbox />
              </Button>
            </Show>
          }
        />

        <div class={style()}>
          {/*
             NOTE: The `csp` attribute is not yet supported by Firefox & Safari:
               https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/csp
          */}
          <iframe
            ref={iframe}
            style={{
              width: "100%",
              height: "100%",
              display: "block",
            }}
            sandbox={defaultSandbox}
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
    theme?: ResolvedTheme;
  };
};

type Message = SetupMessage;

const iframeCsp = import.meta.env.DEV
  ? ""
  : [
      "default-src 'self' 'unsafe-inline'",
      "style-src 'self' 'unsafe-inline'",
      // NOTE: the "*" is critical here because the sandboxed srcdoc iframe's
      // origin is "null", i.e. 'self' is null and we need to allow fetches from
      // the server. We also had '*' to the admin UI's CSP because Firefox/Safari
      // ignore this property.
      "connect-src * 'self' 'unsafe-inline'",
      // NOTE: For some reason `script-src` and `script-src-elem` seem to be ignored
      // even by Chrome and instead the parent CSP is maintained.
      // "script-src 'self' 'unsafe-inline'",
    ].join("; ");

// NOTE: An iframe which has both allow-scripts and allow-same-origin for its
// sandbox attribute can remove its sandboxing.
//
// Sandbox options:
//   https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe#sandbox
const defaultSandbox = "allow-scripts";
