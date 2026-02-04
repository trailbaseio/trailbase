import { computed } from "nanostores";
import { persistentAtom } from "@nanostores/persistent";
import type { Client, Tokens, User } from "trailbase";
import { FetchError, initClient } from "trailbase";
import { showToast } from "@/components/ui/toast";

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

  // Check and/or update the tokens. In case of a 401 (UNAUTHORIZED), this will
  // internally trigger a logout, which will invoke `onAuthChange` above, which will
  // update the $token and $user state.
  try {
    client.refreshAuthToken();
  } catch (err) {
    if (err instanceof FetchError && err.status === 401) {
      console.info(
        "Token refresh failed (401). User should be redirected to login.",
      );

      showToast({
        title: "Logged out - redirecting",
        description:
          "Your tokens were either expired invalidated server-side. Please try signing in again.",
        variant: "default",
      });
    } else {
      throw err;
    }
  }

  return client;
}

export const client = buildClient();
