import { createEffect, onCleanup, Match, Switch, Show } from "solid-js";
import { A } from "@solidjs/router";
import { useQuery } from "@tanstack/solid-query";
import { TbOutlineArrowLeft, TbOutlineSandbox } from "solid-icons/tb";
import { createWritableMemo } from "@solid-primitives/memo";
import { Tokens } from "trailbase";

import type { WasmComponent } from "@bindings/WasmComponent";

import { Header } from "@/components/Header";
import {
  Switch as ToggleSwitch,
  SwitchControl,
  SwitchThumb,
  SwitchLabel,
} from "@/components/ui/switch";

import { client, hostAddress } from "@/lib/client";
import { $tokens } from "@/lib/client";
import { type ResolvedTheme, currentTheme } from "@/lib/theme";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/Spinner";
import { cn } from "@/lib/utils";

function SandboxedIframe(props: { component: WasmComponent }) {
  const source = () => getAdminUiPath(props.component);
  const dashboardPage = useQuery(() => ({
    queryKey: ["wasm-dash", source()],
    queryFn: async ({ queryKey: _ }) => {
      const src = source();
      if (!src) {
        return;
      }

      const response = await fetch(src, { headers: client.headers() });
      if (!response.ok) {
        throw new Error("dashboard request failed");
      }
      const expectedOrigin = new URL(src, window.location.origin).origin;
      if (new URL(response.url, expectedOrigin).origin !== expectedOrigin) {
        throw new Error("dashboard origin rejected");
      }
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

      const src = source();
      if (!src) {
        return;
      }
      const dashboardOrigin = new URL(src, window.location.origin).origin;
      const metaCsp = iframeCsp(dashboardOrigin);
      body = injectCspMeta(body, metaCsp);

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
      let loaded = false;
      const onLoad = (_ev: HTMLElementEventMap["load"]) => {
        if (loaded) {
          cleanup?.();
          cleanup = undefined;
          return;
        }
        loaded = true;
        iframe.focus();
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
      };

      iframe.addEventListener("load", onLoad);
      onCleanup(() => {
        iframe?.removeEventListener("load", onLoad);
        cleanup?.();
      });

      // Set the actual body.
      //
      // NOTE: `srcdoc` with string is less efficient than using a `src="blob:..."`
      // with `createObjectURL`, however relative, path-based resources, e.g.
      // `<img src="/foo.png" />` will work because fetches won't be relative to
      // a `blob:` origin.
      iframe.srcdoc = body;
    }
  });

  return (
    <div class="relative size-full">
      {/*
         Sandbox options:
         https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe#sandbox

         WARN: An iframe which has both allow-scripts and allow-same-origin for its
         sandbox attribute can remove its sandboxing.
      */}
      <iframe
        ref={iframe}
        style={{
          width: "100%",
          height: "100%",
          display: "block",
        }}
        title="WASM component preview"
        sandbox="allow-scripts allow-modals"
        csp={src ? iframeCsp(new URL(src, window.location.origin).origin) : undefined}
      />

      <Show when={dashboardPage.isLoading || dashboardPage.isError}>
        <div
          class="bg-background/90 absolute inset-0 flex flex-col items-center justify-center gap-3 p-6 text-center"
          role={dashboardPage.isError ? "alert" : "status"}
          aria-live="polite"
        >
          <Show
            when={dashboardPage.isError}
            fallback={
              <>
                <Spinner size={28} />
                <p class="m-0">Loading component dashboard...</p>
              </>
            }
          >
            <p class="m-0">Unable to load the component dashboard.</p>
            <Button
              type="button"
              variant="outline"
              onClick={() => dashboardPage.refetch()}
            >
              Retry
            </Button>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function YoloIframe(props: { component: WasmComponent }) {
  const source = () => getAdminUiPath(props.component);
  const src = source();

  return (
    <iframe
      src={src}
      title="WASM component dashboard"
      style={{
        width: "100%",
        height: "100%",
        display: "block",
      }}
      sandbox={undefined}
      csp={undefined}
    />
  );
}

function BackButton() {
  return (
    <A
      href="/wasm"
      class="text-muted-foreground hover:text-foreground flex items-center justify-center transition-colors"
      title="Back to the list of WASM components"
      aria-label="Back to the list of WASM components"
    >
      <TbOutlineArrowLeft size={20} />
    </A>
  );
}

function SandboxButton(props: {
  sandboxed: boolean;
  setSandboxed: (v: boolean) => void;
}) {
  return (
    <ToggleSwitch
      class="flex items-center space-x-2"
      checked={props.sandboxed}
      onChange={(v) => {
        console.debug("sandbox enabled:", v);
        props.setSandboxed(v);
      }}
    >
      <SwitchControl class="bg-destructive ui-checked:bg-input">
        <SwitchThumb />
      </SwitchControl>

      <SwitchLabel>
        <div class={cn("flex gap-1", !props.sandboxed && "opacity-50")}>
          Sandboxed <TbOutlineSandbox />
        </div>
      </SwitchLabel>
    </ToggleSwitch>
  );
}

export function WasmComponentDetails(props: {
  component: WasmComponent;
  sandboxed: boolean;
}) {
  const [sandboxed, setSandboxed] = createWritableMemo<boolean>(
    () => props.sandboxed,
  );
  const dashboardPath = () => getAdminUiPath(props.component);

  return (
    <Switch>
      <Match when={!dashboardPath()}>
        <div class="flex size-full flex-col items-center justify-center gap-3 p-6 text-center">
          <h2 class="text-lg font-semibold">
            {props.component.admin_ui_path
              ? "Dashboard unavailable"
              : "No dashboard available"}
          </h2>
          <p class="text-muted-foreground m-0">
            {props.component.admin_ui_path
              ? "The dashboard path was rejected for safety."
              : `The '${props.component.name}' component has no dashboard.`}
          </p>
          <BackButton />
        </div>
      </Match>

      <Match when={true}>
        <Header
          title={props.component.display_name ?? props.component.name}
          description={`Internal name: ${props.component.name}`}
          leading={BackButton()}
          left={props.component.version && `@${props.component.version}`}
          right={
            <Show when={import.meta.env.DEV}>
              <SandboxButton
                sandboxed={sandboxed()}
                setSandboxed={setSandboxed}
              />
            </Show>
          }
        />

        <div class="size-full">
          <Switch>
            <Match when={!sandboxed()}>
              <YoloIframe component={props.component} />
            </Match>

            <Match when={true}>
              <SandboxedIframe component={props.component} />
            </Match>
          </Switch>
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

function getAdminUiPath(component: WasmComponent): string | undefined {
  const path = component.admin_ui_path;
  if (!path) {
    return;
  }

  // Ideally with a strict parent `connect-src` CSP we could allow URLs w/o
  // checking the dashboard's origin. However, Firefox required us to have a
  // loose '*' `connect-src` policy for now, forcing us to implement our own
  // here. The risk is that an untrusted component could register a URL, sent
  // admins off site and exfiltrate the postMessage tokens. Arguably that's
  // still true, i.e. a local path can forward credentials.
  //
  // Even with a stricter CSP, this defence in depth.
  let resolved: URL;
  try {
    resolved = new URL(path, window.location.origin);
  } catch {
    return;
  }
  if (!path.startsWith("/") || resolved.origin !== window.location.origin) {
    return;
  }

  // Fix up for separate dev server.
  return import.meta.env.DEV
    ? `http://${window.location.hostname}:4000${path}`
    : path;
}

export function injectCspMeta(body: string, csp: string): string {
  const meta = `<meta http-equiv="Content-Security-Policy" content="${csp}">`;
  if (/<head(?:\s[^>]*)?>/i.test(body)) {
    return body.replace(/(<head(?:\s[^>]*)?>)/i, `$1${meta}`);
  }
  if (/<html(?:\s[^>]*)?>/i.test(body)) {
    return body.replace(/(<html(?:\s[^>]*)?>)/i, `$1<head>${meta}</head>`);
  }
  return `<head>${meta}</head>${body}`;
}

// NOTE: The `csp` attribute is not yet supported by Firefox & Safari:
//   https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/csp
const iframeCsp = (origin: string) =>
  import.meta.env.DEV
    ? ""
    : `default-src 'self' ${origin}; style-src 'self' ${origin} 'unsafe-inline'; script-src 'self' ${origin} 'unsafe-inline'; img-src 'self' ${origin} data:; connect-src ${origin}`;
