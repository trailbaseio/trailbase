import { atom, computed, task } from "nanostores";
import { persistentAtom } from "@nanostores/persistent";
import { Client, type Tokens, type User } from "trailbase";

export const HOST = import.meta.env.DEV ? "http://localhost:4000" : "";

const $tokens = persistentAtom<Tokens | null>("auth_tokens", null, {
  encode: JSON.stringify,
  decode: JSON.parse,
});

export function removeTokens() {
  $tokens.set(null);
}

export const $user = atom<User | undefined>();

export const $client = computed([], () =>
  task(async () => {
    return Client.tryFromCookies(HOST, {
      tokens: $tokens.get() ?? undefined,
      onAuthChange: (c: Client, user?: User) => {
        $tokens.set(c.tokens() ?? null);
        $user.set(user);
      },
    });
  }),
);
