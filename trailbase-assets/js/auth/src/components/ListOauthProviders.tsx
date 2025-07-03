import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { createResource, For, Suspense, ErrorBoundary } from "solid-js";
import type { ConfiguredOAuthProvidersResponse } from "@bindings/ConfiguredOAuthProvidersResponse";

import { AUTH_API } from "@/lib/constants";

// OAuth2 provider assets.
import openIdConnect from "@shared/assets/oauth2/oidc.svg";
import discord from "@shared/assets/oauth2/discord.svg";
import facebook from "@shared/assets/oauth2/facebook.svg";
import gitlab from "@shared/assets/oauth2/gitlab.svg";
import google from "@shared/assets/oauth2/google.svg";
import microsoft from "@shared/assets/oauth2/microsoft.svg";

const assets = new Map<string, string>([
  ["discord", discord.src],
  ["facebook", facebook.src],
  ["gitlab", gitlab.src],
  ["google", google.src],
  ["microsoft", microsoft.src],
  ["oidc0", openIdConnect.src],

  ["fake", openIdConnect.src],
]);

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
      providers.push(["fake", "Fake Provider"]);
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
              const image = assets.get(name);

              return (
                <a
                  class={cn("w-full", buttonVariants({ variant: "outline" }))}
                  href={`${AUTH_API}/oauth/${name}/login${window.location.search}`}
                >
                  <div class="flex items-center gap-2">
                    {image && (
                      <img class="size-[28px]" src={image} alt={displayName} />
                    )}
                    <span class="font-bold">{displayName}</span>
                  </div>
                </a>
              );
            }}
          </For>
        </div>
      </Suspense>
    </ErrorBoundary>
  );
}
