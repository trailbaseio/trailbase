import { atom, computed, task } from "nanostores";
import { Client, type User } from "trailbase";

import { HOST } from "@/lib/constants.ts";

export const $user = atom<User | undefined>();

function onAuthChange(c: Client) {
  $user.set(c.user());
}

export const $client = computed([], () =>
  task(async () => {
    const client = await Client.tryFromCookies(HOST, {
      tokens: undefined,
      onAuthChange,
    });

    onAuthChange(client);

    return client;
  }),
);
