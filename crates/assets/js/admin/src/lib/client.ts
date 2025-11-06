import { computed } from "nanostores";
import { persistentAtom } from "@nanostores/persistent";
import type { Client, Tokens, User } from "trailbase";
import { initClient } from "trailbase";

const $tokens = persistentAtom<Tokens | null>("auth_tokens", null, {
  encode: JSON.stringify,
  decode: JSON.parse,
});
export const $user = computed($tokens, (_tokens) => client.user());

export function hostAddress(): string | undefined {
  // For our dev server setup we assume that a TrailBase instance is running at ":4000",
  // otherwise we query APIs relative to the origin's root path.
  if (import.meta.env.DEV) {
    return `http://${window.location.hostname}:4000`;
  }
  return undefined;
}

function buildClient(): Client {
  const address = hostAddress();
  const client = initClient(address && new URL(address), {
    tokens: $tokens.get() ?? undefined,
    onAuthChange: (c: Client, _user: User | undefined) => {
      $tokens.set(c.tokens() ?? null);
    },
  });

  // This will also trigger a logout in case of 401.
  client.refreshAuthToken();

  return client;
}

export const client = buildClient();
