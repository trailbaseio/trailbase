import { createSignal, Show } from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbUser } from "solid-icons/tb";
import { type User } from "trailbase";

import { urlSafeBase64ToUuid } from "@/lib/utils";
import { client, $user } from "@/lib/fetch";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { navBarIconSize, navBarIconStyle } from "@/components/NavBar";

function avatarUrl(user: User): string {
  return import.meta.env.DEV
    ? `http://localhost:4000/api/auth/v1/avatar/${user.id}`
    : `/api/auth/v1/avatar/${user.id}`;
}

function Avatar(props: { user: User | undefined; size: number }) {
  if (props.user === undefined) {
    return <TbUser size={props.size} color="#0073aa" />;
  }

  return (
    <object
      class="rounded-full bg-transparent"
      type="image/png"
      data={avatarUrl(props.user)}
      width={props.size}
      height={props.size}
      aria-label="Avatar image"
    >
      {/* Fallback */}
      <TbUser size={props.size} color="#0073aa" />
    </object>
  );
}

function Profile(props: { user: User }) {
  return (
    <div class="flex gap-4">
      <div class="flex items-center">
        <Avatar user={props.user} size={60} />
      </div>

      <div class="flex flex-col gap-2">
        <div>E-mail: {props.user.email}</div>

        <div>id: {urlSafeBase64ToUuid(props.user.id)}</div>

        {import.meta.env.DEV && <div>id b64: {props.user.id}</div>}
      </div>
    </div>
  );
}

export function AuthButton() {
  const [open, setOpen] = createSignal(false);
  const user = useStore($user);

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <button class={navBarIconStyle} onClick={() => setOpen(true)}>
        <Avatar user={user()} size={navBarIconSize} />
      </button>

      <DialogContent class="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Current User</DialogTitle>
        </DialogHeader>

        <Show when={user()}>
          <Profile user={user()!} />
        </Show>

        <DialogFooter>
          <Button
            type="Logout"
            onClick={() => {
              client.logout();
            }}
          >
            Logout
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
