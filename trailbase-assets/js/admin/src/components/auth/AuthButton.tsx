import { createSignal, Show } from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbUser } from "solid-icons/tb";
import { type User } from "trailbase";

import { urlSafeBase64ToUuid } from "@/lib/utils";
import { $user, client } from "@/lib/fetch";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { navBarIconSize, navBarIconStyle } from "@/components/NavBar";

function Profile(props: { user: User }) {
  // TODO: Bring back avater.
  return (
    <div class="flex flex-col gap-2">
      <div>E-mail: {props.user.email}</div>

      <div>id: {urlSafeBase64ToUuid(props.user.id)}</div>

      {import.meta.env.DEV && <div>id b64: {props.user.id}</div>}
    </div>
  );
}

export function AuthButton() {
  const [open, setOpen] = createSignal(false);
  const user = useStore($user);

  // For our dev server setup we assume that a TrailBase instance is running at ":4000", otherwise
  // we query APIs relative to the origin's root path.
  const redirect = import.meta.env.DEV
    ? "http://localhost:4000/_/auth/logout?redirect_to=http://localhost:3000/_/admin/"
    : "/_/auth/logout?redirect_to=/_/admin/";

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <button class={navBarIconStyle} onClick={() => setOpen(true)}>
        <TbUser size={navBarIconSize} />
      </button>

      <DialogContent class="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Current User</DialogTitle>
        </DialogHeader>

        <Show when={user()}>
          <Profile user={user()!} />
        </Show>

        <DialogFooter>
          <a
            type="button"
            href={redirect}
            class={buttonVariants({ variant: "default" })}
            onClick={() => client.logout()}
          >
            Logout
          </a>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
