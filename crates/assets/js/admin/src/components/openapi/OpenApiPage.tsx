import { Show, createEffect, createSignal } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { useStore } from "@nanostores/solid";
import { TbOutlineRefresh } from "solid-icons/tb";
import { adminFetch } from "@/lib/fetch";
import { createTheme } from "@/lib/theme";
import { $tokens, $user } from "@/lib/client";
import { Header } from "@/components/Header";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Callout, CalloutContent, CalloutTitle } from "@/components/ui/callout";
import {
  applyRequestTokens,
  parseImpersonationTokens,
  openApiMetadata,
  usableRequestTokens,
  type OpenApiDocument,
} from "./openapi";
import "rapidoc";

declare module "solid-js" {
  namespace JSX {
    interface IntrinsicElements {
      "rapi-doc": JSX.HTMLAttributes<HTMLElement> & { [key: string]: any };
    }
  }
}

type RapiDoc = HTMLElement & { loadSpec?: (spec: OpenApiDocument) => void };
type BeforeTryEvent = CustomEvent<{ request: Request }>;

async function fetchOpenApi(): Promise<OpenApiDocument> {
  const response = await adminFetch("/openapi.json");
  return (await response.json()) as OpenApiDocument;
}

export default function Page() {
  const theme = createTheme();
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
    if (import.meta.env.DEV) return "http://localhost:4000";
    const servers = query.data?.servers;
    if (
      Array.isArray(servers) &&
      typeof servers[0] === "object" &&
      servers[0] !== null &&
      typeof (servers[0] as { url?: unknown }).url === "string"
    )
      return (servers[0] as { url: string }).url;
    return window.location.origin;
  };
  const [rapidoc, setRapidoc] = createSignal<RapiDoc>();
  createEffect(() => {
    const element = rapidoc();
    const spec = query.data;
    if (element && spec) element.loadSpec?.(spec);
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
    <div class="flex h-full min-h-0 flex-col">
      <Header
        title="OpenAPI Explorer"
        description={() => `Explore and try ${metadata().title}.`}
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
        <Callout variant="error" class="m-4">
          <CalloutTitle>Unable to load the API specification</CalloutTitle>
          <CalloutContent>
            <Button size="sm" onClick={refresh}>
              Retry
            </Button>
          </CalloutContent>
        </Callout>
      </Show>
      <Show when={query.error && query.data}>
        <Callout variant="warning" class="mx-4 mb-2">
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
        <rapi-doc
          ref={(element) => {
            const r = element as RapiDoc;
            setRapidoc(r);
            r.setAttribute("server-url", server());
            r.setAttribute("default-api-server", server());
            r.addEventListener("before-try", (event) => {
              const tokens =
                overrideTokens() ?? usableRequestTokens($tokens.get());
              if (tokens)
                applyRequestTokens(
                  (event as BeforeTryEvent).detail.request,
                  tokens,
                );
            });
          }}
          load-fonts="false"
          sort-tags="true"
          theme={theme()}
          bg-color={theme() === "light" ? "#FFFFFF" : "#09090B"}
          primary-color="#0073a8"
          render-style="view"
          layout="row"
          show-header="false"
          allow-try="true"
          persist-auth="false"
          allow-authentication="false"
          allow-server-selection="false"
          class="min-h-0 flex-1"
        />
      </Show>
    </div>
  );
}
