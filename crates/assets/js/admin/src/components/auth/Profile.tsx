import { Switch, Match, Show, createEffect } from "solid-js";
import { TbFillUser } from "solid-icons/tb";
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
        <TbFillUser size={props.size} color="#0073aa" />
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
          <TbFillUser size={props.size} color="#0073aa" />
        </object>
      </Match>
    </Switch>
  );
}

export function Profile(props: {
  user: User;
  showId?: boolean;
  twoFactorEnabled?: boolean;
}) {
  if (import.meta.env.DEV) {
    createEffect(() => {
      console.debug("user:", props.user);
    });
  }

  return (
    <div class="flex w-full shrink flex-col">
      <div class="flex shrink items-center gap-4">
        <div class="m-1 flex items-center">
          <Avatar user={props.user} size={40} />
        </div>

        <div class="flex w-full flex-col gap-2 break-all">
          <div>Email: {props.user.email}</div>

          <Show when={props.showId ?? true}>
            <div>id: {urlSafeBase64ToUuid(props.user.id)}</div>
          </Show>

          <div>
            Two-factor:{" "}
            {(props.twoFactorEnabled ?? props.user.mfa ?? false)
              ? "Enabled"
              : "Disabled"}
          </div>
        </div>
      </div>
    </div>
  );
}
