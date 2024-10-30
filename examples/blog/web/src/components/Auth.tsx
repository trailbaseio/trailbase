import { createResource, Match, Suspense, Switch } from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbUser } from "solid-icons/tb";
import type { User } from "trailbase";

import { $client, $user, removeTokens } from "@/lib/client";
import { $profile } from "@/lib/profile";

function UserBadge(props: { user: User | undefined }) {
  const client = useStore($client);
  const profile = useStore($profile);
  const [avatar] = createResource(client, async (c) => await c?.avatarUrl());

  const Fallback = () => (
    <TbUser class="p-1 size-6 bg-pacamara-secondary inline-block rounded-full dark:text-white" />
  );

  return (
    <Suspense fallback={<p>...</p>}>
      <div class="flex gap-2 items-center ">
        <Switch fallback={<Fallback />}>
          <Match when={avatar.error}>
            <Fallback />
          </Match>

          <Match when={avatar()}>
            <img
              class="size-6 inline-block rounded-full"
              src={avatar()!}
              alt="avatar"
            />
          </Match>
        </Switch>

        <span>{profile()?.profile?.username ?? props.user?.email}</span>
      </div>
    </Suspense>
  );
}

export function AuthButton() {
  const client = useStore($client);
  const user = useStore($user);

  return (
    <Switch>
      <Match when={!user()}>
        <a href={client()?.loginUri("/") ?? "/_/auth/login"}>Log in</a>
      </Match>

      <Match when={user()}>
        <button
          onClick={() => {
            removeTokens();
            const logout = client()?.logoutUri("/") ?? "/_/auth/logout";
            window.location.assign(logout);
          }}
        >
          <UserBadge user={user()} />
        </button>
      </Match>
    </Switch>
  );
}
