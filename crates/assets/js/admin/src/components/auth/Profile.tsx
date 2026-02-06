import { Switch, Match, Show } from "solid-js";
import { TbUser } from "solid-icons/tb";
import type { User } from "trailbase";

import { urlSafeBase64ToUuid } from "@/lib/utils";
import { hostAddress } from "@/lib/client";

function avatarUrl(user: User): string {
  const address = hostAddress();
  return `${address ?? ""}/api/auth/v1/avatar/${user.id}`;
}

export function Avatar(props: { user: User | undefined; size: number }) {
  return (
    <Switch>
      <Match when={props.user === undefined}>
        <TbUser size={props.size} color="#0073aa" />
      </Match>

      <Match when={props.user !== undefined}>
        <object
          class="rounded-full bg-transparent"
          type="image/png"
          data={avatarUrl(props.user!)}
          width={props.size}
          height={props.size}
          aria-label="Avatar image"
        >
          {/* Fallback */}
          <TbUser size={props.size} color="#0073aa" />
        </object>
      </Match>
    </Switch>
  );
}

export function Profile(props: { user: User; showId?: boolean }) {
  return (
    <div class="flex w-full shrink flex-col">
      <div class="flex shrink items-center gap-4">
        <div class="flex items-center">
          <Avatar user={props.user} size={60} />
        </div>

        <div class="flex w-full flex-col gap-2 break-all">
          <div>Email: {props.user.email}</div>

          <Show when={props.showId ?? true}>
            <div>id: {urlSafeBase64ToUuid(props.user.id)}</div>
          </Show>

          <Show when={import.meta.env.DEV}>
            <span class="bg-red-200">{JSON.stringify(props.user)}</span>
          </Show>
        </div>
      </div>
    </div>
  );
}
