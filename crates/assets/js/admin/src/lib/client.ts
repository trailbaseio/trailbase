import { computed } from "nanostores";
import { persistentAtom } from "@nanostores/persistent";
import type { Client, Tokens, User } from "trailbase";
import { initClient } from "trailbase";

const $tokens = persistentAtom<Tokens | null>("auth_tokens", null, {
  encode: JSON.stringify,
  decode: JSON.parse,
});
export const $user = computed($tokens, (_tokens) => client.user());

function buildClient(): Client {
  // For our dev server setup we assume that a TrailBase instance is running at ":4000", otherwise
  // we query APIs relative to the origin's root path.
  const HOST = import.meta.env.DEV
    ? new URL("http://localhost:4000")
    : undefined;
  const client = initClient(HOST, {
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
