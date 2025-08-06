import { atom, computed, task } from "nanostores";
import type { Client, User } from "trailbase";
import { initClientFromCookies } from "trailbase";

import { HOST } from "@/lib/constants.ts";

export const $user = atom<User | undefined>();

function onAuthChange(c: Client) {
  $user.set(c.user());
}

export const $client = computed([], () =>
  task(async () => {
    const client = await initClientFromCookies(HOST, {
      tokens: undefined,
      onAuthChange,
    });

    onAuthChange(client);

    return client;
  }),
);
