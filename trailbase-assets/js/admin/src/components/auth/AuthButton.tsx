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
