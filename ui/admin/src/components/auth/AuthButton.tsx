import { createEffect, createSignal, Show } from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbUser } from "solid-icons/tb";
import { type User } from "trailbase";

import { urlSafeBase64ToUuid } from "@/lib/utils";
import { client, $user } from "@/lib/fetch";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

const navBarIconSize = 22;
const style =
  "rounded-full hover:bg-accent-600 hover:text-white transition-all p-[10px]";

function Profile(props: { user: User }) {
  const profile = props.user;

  // TODO: Bring back avater.
  return (
    <div class="flex flex-col gap-2">
      <div>E-mail: {profile.email}</div>

      <div>id: {urlSafeBase64ToUuid(profile.id)}</div>

      {import.meta.env.DEV && <div>id b64: {profile.id}</div>}
    </div>
  );
}

export function AuthButton() {
  const [open, setOpen] = createSignal(false);
  const user = useStore($user);

  createEffect(() => {
    console.log("user", user());
  });

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <button class={style} onClick={() => setOpen(true)}>
        <TbUser size={navBarIconSize} />
      </button>

      <DialogContent class="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Current User</DialogTitle>
          <DialogDescription></DialogDescription>
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
