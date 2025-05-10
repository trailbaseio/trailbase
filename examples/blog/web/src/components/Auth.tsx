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
    <TbUser class="inline-block size-6 rounded-full bg-pacamara-secondary p-1 dark:text-white" />
  );

  return (
    <Suspense fallback={<p>...</p>}>
      <div class="flex items-center gap-2 ">
        <Switch fallback={<Fallback />}>
          <Match when={avatar.error}>
            <Fallback />
          </Match>

          <Match when={avatar()}>
            <img
              class="inline-block size-6 rounded-full"
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
  const user = useStore($user);

  return (
    <Switch>
      <Match when={!user()}>
        <a
          href={
            import.meta.env.DEV
              ? "http://localhost:4000/_auth/login?redirect_to=/"
              : "/_/auth/login?redirect_to=/"
          }
        >
          Log in
        </a>
      </Match>

      <Match when={user()}>
        <button
          onClick={() => {
            // Remove local tokens before redirecting.
            removeTokens();

            const path = import.meta.env.DEV
              ? "http://localhost:4000/_/auth/logout"
              : "/_/auth/logout";
            window.location.assign(path);
          }}
        >
          <UserBadge user={user()} />
        </button>
      </Match>
    </Switch>
  );
}
