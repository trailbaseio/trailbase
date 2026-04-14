import { Match, Switch } from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbFillUser } from "solid-icons/tb";
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

  return (
    <div class="flex items-center gap-2">
      <object
        class="inline-block size-6 rounded-full hover:bg-gray-200"
        type="image/png"
        data={avatar()}
        aria-label="Avatar image"
      >
        {/* Fallback */}
        <div class="size-6 flex items-center justify-center">
          <TbFillUser size={18} color="#0073aa" />
        </div>
      </object>

      <span>{profile()?.profile?.username ?? props.user?.email}</span>
    </div>
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
