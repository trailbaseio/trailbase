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
  let [providersFetch] = createResource(listConfiguredOAuthProviders);

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
        <div class="flex flex-col w-full gap-4 items-start">
          {providers().length > 0 && <p>Or use an external provider:</p>}

          <For each={providers()}>
            {([name, displayName]) => {
              return (
                <a
                  class="w-full p-2 rounded-lg border border-gray-300/20 hover:bg-black/10 dark:hover:bg-black/20 flex flex-row items-center gap-4"
                  href={`${AUTH_API}/oauth/${name}/login`}
                >
                  <span>Login with {displayName}</span>
                </a>
              );
            }}
          </For>
        </div>
      </Suspense>
    </ErrorBoundary>
  );
}
