import { createSignal, Show } from "solid-js";
import { useStore } from "@nanostores/solid";

import { client, $user } from "@/lib/client";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { navbarIconStyle } from "@/components/Navbar";
import { Avatar, Profile } from "@/components/auth/Profile";

export function AuthButton(props: { iconSize: number }) {
  const [open, setOpen] = createSignal(false);
  const user = useStore($user);

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <button class={navbarIconStyle} onClick={() => setOpen(true)}>
        <Avatar user={user()} size={props.iconSize} />
      </button>

      <DialogContent class="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle>Logged in</DialogTitle>
        </DialogHeader>

        <Show when={user()}>
          <Profile user={user()!} />
        </Show>

        <DialogFooter>
          <Button type="button" onClick={() => client.logout()}>
            Logout
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
