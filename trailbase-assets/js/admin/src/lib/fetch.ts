import { computed } from "nanostores";
import { persistentAtom } from "@nanostores/persistent";
import { Client, type Tokens, type User } from "trailbase";

import { showToast } from "@/components/ui/toast";

const $tokens = persistentAtom<Tokens | null>("auth_tokens", null, {
  encode: JSON.stringify,
  decode: JSON.parse,
});
export const $user = computed($tokens, (_tokens) => client.user());

function initClient(): Client {
  const HOST = import.meta.env.DEV ? "http://localhost:4000" : "";
  const client = Client.init(HOST, {
    tokens: $tokens.get() ?? undefined,
    onAuthChange: (c: Client, _user: User | undefined) => {
      $tokens.set(c.tokens() ?? null);
    },
  });

  // This will also trigger a logout in case of 401.
  client.refreshAuthToken();

  return client;
}
export const client = initClient();

type FetchOptions = RequestInit & {
  throwOnError?: boolean;
};

export async function adminFetch(
  input: string,
  init?: FetchOptions,
): Promise<Response> {
  if (!input.startsWith("/")) {
    throw Error("Should start with '/'");
  }

  try {
    return await client.fetch(`api/_admin${input}`, init);
  } catch (err) {
    showToast({
      title: "Fetch Error",
      description: `${err}`,
      variant: "error",
    });

    throw err;
  }
}
