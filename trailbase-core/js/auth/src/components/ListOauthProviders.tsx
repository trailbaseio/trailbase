import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { createResource, For, Suspense, ErrorBoundary } from "solid-js";
import type { ConfiguredOAuthProvidersResponse } from "@bindings/ConfiguredOAuthProvidersResponse";

import { AUTH_API } from "@/lib/constants";

async function listConfiguredOAuthProviders(): Promise<ConfiguredOAuthProvidersResponse> {
  const response = await fetch(`${AUTH_API}/oauth/providers`, {
    method: "GET",
    headers: {
      "Content-Type": "application/json",
    },
  });

  if (!response.ok) {
    throw await response.text();
  }
  return await response.json();
}

export function ConfiguredOAuthProviders() {
  const [providersFetch] = createResource(listConfiguredOAuthProviders);

  const providers = () => {
    const providers = [...(providersFetch()?.providers ?? [])];
    if (import.meta.env.DEV) {
      providers.push(["name", "Display Name"]);
    }
    return providers;
  };

  return (
    <ErrorBoundary fallback={(err, _reset) => <h2>OAuth: {err.toString()}</h2>}>
      <Suspense fallback={<div>Loading...</div>}>
        <div class="flex w-full flex-col items-start gap-4">
          {providers().length > 0 && (
            <div class="flex w-full justify-center text-muted-foreground">
              <span>Or authenticate using:</span>
            </div>
          )}

          <For each={providers()}>
            {([name, displayName]) => {
              return (
                <a
                  class={cn("w-full", buttonVariants({ variant: "outline" }))}
                  href={`${AUTH_API}/oauth/${name}/login`}
                >
                  Login with {displayName}
                </a>
              );
            }}
          </For>
        </div>
      </Suspense>
    </ErrorBoundary>
  );
}
