import { atom, computed, task } from "nanostores";
import { persistentAtom } from "@nanostores/persistent";
import { initClientFromCookies } from "trailbase";
import type { Client, Tokens, User } from "trailbase";

export const HOST = import.meta.env.DEV ? "http://localhost:4000" : "";

const $tokens = persistentAtom<Tokens | null>("auth_tokens", null, {
  encode: JSON.stringify,
  decode: JSON.parse,
});

export function removeTokens() {
  $tokens.set(null);
}

export const $user = atom<User | undefined>();

function onAuthChange(c: Client) {
  $tokens.set(c.tokens() ?? null);
  $user.set(c.user());
}

export const $client = computed([], () =>
  task(async () => {
    const client = await initClientFromCookies(HOST, {
      tokens: $tokens.get() ?? undefined,
      onAuthChange,
    });

    onAuthChange(client);

    return client;
  }),
);
