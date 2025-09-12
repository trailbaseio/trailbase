import { Match, Suspense, Switch } from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbUser } from "solid-icons/tb";
import type { User } from "trailbase";

import { $client, $user, removeTokens, HOST } from "@/lib/client";
import { $profile } from "@/lib/profile";

function UserBadge(props: { user: User | undefined }) {
  const client = useStore($client);
  const profile = useStore($profile);
  const avatar = () => {
    const url = client()?.avatarUrl();
    if (url !== undefined) {
      return `${HOST}${url}`;
    }
    return undefined;
  };

  const Fallback = () => (
    <TbUser class="inline-block size-6 rounded-full bg-pacamara-secondary p-1 dark:text-white" />
  );

  return (
    <Suspense fallback={<p>...</p>}>
      <div class="flex items-center gap-2">
        <Switch fallback={<Fallback />}>
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
  const redirect = import.meta.env.DEV ? `${window.location.origin}/` : "/";

  return (
    <Switch>
      <Match when={!user()}>
        <a href={`${HOST}/_/auth/login?redirect_uri=${redirect}`}>Log in</a>
      </Match>

      <Match when={user()}>
        <button
          onClick={() => {
            // Remove local tokens before redirecting.
            removeTokens();
            window.location.assign(`${HOST}/_/auth/logout`);
          }}
        >
          <UserBadge user={user()} />
        </button>
      </Match>
    </Switch>
  );
}
