import { Show, createEffect, createSignal, onCleanup } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { useStore } from "@nanostores/solid";
import { TbOutlineRefresh } from "solid-icons/tb";
import { adminFetch } from "@/lib/fetch";
import { createTheme } from "@/lib/theme";
import { $tokens, $user } from "@/lib/client";
import { Header } from "@/components/Header";
import { createIsMobile } from "@/lib/signals";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Callout, CalloutContent, CalloutTitle } from "@/components/ui/callout";
import {
  applyRequestTokens,
  parseImpersonationTokens,
  openApiMetadata,
  requestHasCredentialOrigin,
  resolveOpenApiServer,
  usableRequestTokens,
  withCollapsedOpenApiTags,
  type OpenApiDocument,
} from "./openapi";
import "rapidoc";

declare module "solid-js" {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace JSX {
    interface IntrinsicElements {
      "rapi-doc": JSX.HTMLAttributes<HTMLElement> & {
        loadSpec?: (spec: OpenApiDocument) => void;
        "server-url"?: string;
        "default-api-server"?: string;
        "render-style"?: "focused";
        layout?: "row";
        "schema-style"?: "table";
        "show-header"?: "false";
        "show-side-nav"?: "true";
        "allow-search"?: "true";
        "nav-item-spacing"?: "compact";
        "show-method-in-nav-bar"?: "as-colored-block";
        "use-path-in-nav-bar"?: "true";
        "allow-try"?: "true";
        "persist-auth"?: "false";
        "allow-authentication"?: "false";
        "allow-server-selection"?: "false";
        "load-fonts"?: "false";
        "sort-tags"?: "true";
        theme?: "light" | "dark";
        "bg-color"?: string;
        "text-color"?: string;
        "nav-bg-color"?: string;
        "nav-text-color"?: string;
        "nav-hover-bg-color"?: string;
        "nav-hover-text-color"?: string;
        "nav-accent-color"?: string;
        "nav-accent-text-color"?: string;
        "primary-color"?: string;
      };
    }
  }
}

type RapiDoc = HTMLElement & { loadSpec?: (spec: OpenApiDocument) => void };
const palettes = {
  light: {
    bg: "#ffffff",
    text: "#18181b",
    nav: "#f4f4f5",
    navText: "#3f3f46",
    hover: "#e4e4e7",
    hoverText: "#18181b",
    accent: "#0073a8",
    accentText: "#ffffff",
  },
  dark: {
    bg: "#09090b",
    text: "#fafafa",
    nav: "#18181b",
    navText: "#a1a1aa",
    hover: "#27272a",
    hoverText: "#fafafa",
    accent: "#38bdf8",
    accentText: "#082f49",
  },
} as const;
type BeforeTryEvent = CustomEvent<{ request: Request }>;

async function fetchOpenApi(): Promise<OpenApiDocument> {
  const response = await adminFetch("/openapi.json");
  return (await response.json()) as OpenApiDocument;
}

export default function Page() {
  const theme = createTheme();
  const isMobile = createIsMobile();
  const [navOpen, setNavOpen] = createSignal(false);
  const palette = () => palettes[theme()];
  const user = useStore($user);
  const query = useQuery(() => ({
    queryKey: ["openapi"],
    queryFn: fetchOpenApi,
  }));
  const [tokensInput, setTokensInput] = createSignal("");
  const [tokenError, setTokenError] = createSignal(false);
  const [overrideTokens, setOverrideTokens] =
    createSignal<ReturnType<typeof parseImpersonationTokens>>();
  const identity = () => user()?.email || user()?.username || "Admin session";
  const metadata = () => openApiMetadata(query.data);
  const server = () => {
    const servers = query.data?.servers;
    const candidate =
      Array.isArray(servers) &&
      typeof servers[0] === "object" &&
      servers[0] !== null
        ? (servers[0] as { url?: unknown }).url
        : undefined;
    return resolveOpenApiServer(candidate, import.meta.env.DEV);
  };
  const [rapidoc, setRapidoc] = createSignal<RapiDoc>();
  let loadedElement: RapiDoc | undefined;
  let loadedSpec: OpenApiDocument | undefined;
  createEffect(() => {
    const element = rapidoc();
    if (!element) return;
    const beforeTry = (event: Event) => {
      const detail = (event as BeforeTryEvent).detail;
      if (
        !requestHasCredentialOrigin(detail.request.url, import.meta.env.DEV) ||
        tokenError()
      )
        return;
      const tokens = overrideTokens() ?? usableRequestTokens($tokens.get());
      if (tokens) applyRequestTokens(detail.request, tokens);
    };
    const specLoaded = () => {
      const info = element.shadowRoot?.getElementById("api-info");
      if (info) info.style.marginLeft = "0";
    };
    element.addEventListener("before-try", beforeTry);
    element.addEventListener("spec-loaded", specLoaded);
    onCleanup(() => {
      element.removeEventListener("before-try", beforeTry);
      element.removeEventListener("spec-loaded", specLoaded);
    });
  });
  createEffect(() => {
    const element = rapidoc();
    const spec = query.data;
    if (element && spec) {
      element.setAttribute("server-url", server());
      element.setAttribute("default-api-server", server());
      if (element !== loadedElement || spec !== loadedSpec) {
        element.loadSpec?.(withCollapsedOpenApiTags(spec));
        loadedElement = element;
        loadedSpec = spec;
      }
    }
  });
  createEffect(() => {
    if (!isMobile()) setNavOpen(false);
  });
  const refresh = () => query.refetch();

  const handleTokens = (value: string) => {
    setTokensInput(value);
    try {
      setOverrideTokens(parseImpersonationTokens(value));
      setTokenError(false);
    } catch {
      setOverrideTokens(undefined);
      setTokenError(true);
    }
  };

  return (
    <div class="flex h-[calc(100dvh-3rem)] min-h-0 flex-col overflow-hidden md:h-dvh">
      <Header
        title="OpenAPI Explorer"
        description={<span>Explore and try {metadata().title}.</span>}
        right={
          <Button
            size="sm"
            variant="outline"
            onClick={refresh}
            disabled={query.isFetching}
          >
            <TbOutlineRefresh />
            {query.isFetching ? "Refreshing…" : "Refresh"}
          </Button>
        }
      />
      <div class="text-muted-foreground flex flex-wrap gap-2 px-4 py-2 text-xs">
        <Show when={metadata().version}>
          <Badge variant="outline">v{metadata().version}</Badge>
        </Show>
        <span>
          {metadata().operationCount}{" "}
          {metadata().operationCount === 1 ? "operation" : "operations"}
        </span>
        <Show when={query.data}>
          <span>Server: {server()}</span>
        </Show>
        <span>Identity: {identity()}</span>
      </div>
      <Show when={query.isLoading}>
        <div
          role="status"
          aria-label="Loading API specification"
          class="animate-pulse p-6"
        >
          Loading API specification…
        </div>
      </Show>
      <Show when={query.error && !query.data}>
        <Callout role="alert" variant="error" class="m-4">
          <CalloutTitle>Unable to load the API specification</CalloutTitle>
          <CalloutContent>
            <Button size="sm" onClick={refresh}>
              Retry
            </Button>
          </CalloutContent>
        </Callout>
      </Show>
      <Show when={query.error && query.data}>
        <Callout
          role="alert"
          aria-live="polite"
          variant="warning"
          class="mx-4 mb-2"
        >
          <CalloutContent>
            Unable to refresh the API specification.{" "}
            <Button size="sm" variant="outline" onClick={refresh}>
              Retry
            </Button>
          </CalloutContent>
        </Callout>
      </Show>
      <Show when={query.data}>
        <details class="mx-4 mb-2">
          <summary class="cursor-pointer text-sm font-medium">
            Advanced authentication
          </summary>
          <div class="mt-2 max-w-xl">
            <label for="openapi-tokens" class="text-sm font-medium">
              Login tokens
            </label>
            <p
              id="openapi-tokens-description"
              class="text-muted-foreground text-xs"
            >
              Paste copied Accounts login tokens. Stored locally only and never
              persisted.
            </p>
            <input
              id="openapi-tokens"
              aria-describedby="openapi-tokens-description openapi-tokens-error"
              aria-invalid={tokenError()}
              type="password"
              autocomplete="new-password"
              value={tokensInput()}
              onInput={(e) => handleTokens(e.currentTarget.value)}
              class="border-input mt-1 w-full rounded-md border px-3 py-2"
            />
            <Show when={tokenError()}>
              <p id="openapi-tokens-error" class="text-destructive text-xs">
                Invalid login tokens
              </p>
            </Show>
            <Show when={!tokenError()}>
              <p class="text-muted-foreground text-xs">
                {tokensInput().trim()
                  ? "Using impersonation tokens"
                  : "Using current admin session"}
              </p>
            </Show>
          </div>
        </details>
        <Show when={isMobile()}>
          <Button
            class="mx-4 mb-2"
            aria-expanded={navOpen()}
            onClick={() => setNavOpen(!navOpen())}
          >
            Browse endpoints
          </Button>
        </Show>
        <rapi-doc
          ref={(element) => setRapidoc(element as RapiDoc)}
          load-fonts="false"
          sort-tags="true"
          theme={theme()}
          bg-color={palette().bg}
          text-color={palette().text}
          nav-bg-color={palette().nav}
          nav-text-color={palette().navText}
          nav-hover-bg-color={palette().hover}
          nav-hover-text-color={palette().hoverText}
          nav-accent-color={palette().accent}
          nav-accent-text-color={palette().accentText}
          attr:primary-color={palette().accent}
          render-style="focused"
          layout="row"
          schema-style="table"
          show-header="false"
          show-side-nav="true"
          allow-search="true"
          nav-item-spacing="compact"
          show-method-in-nav-bar="as-colored-block"
          use-path-in-nav-bar="true"
          allow-try="true"
          persist-auth="false"
          allow-authentication="false"
          allow-server-selection="false"
          class={`openapi-explorer min-h-0 flex-1 ${navOpen() ? "openapi-nav-open" : ""}`}
        />
      </Show>
    </div>
  );
}
