import { createSignal, Switch, Show, Match } from "solid-js";
import type { Signal } from "solid-js";
import {
  TbFillUser,
  TbOutlineMenu2,
  TbOutlineLogout,
  TbOutlineTrash,
  TbOutlineEdit,
} from "solid-icons/tb";
import { useStore } from "@nanostores/solid";
import type { Client, User } from "trailbase";

import { HOST, AVATAR_API } from "@/lib/constants";
import { $client } from "@/lib/client";
import { cn } from "@/lib/utils";

import { Button, buttonVariants } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { showToast } from "@/components/ui/toast";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { TotpToggleButton } from "@/components/Totp";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

function avatarUrl(user: User): string {
  return `${AVATAR_API}/${user.id}`;
}

function DeleteAccountDialog(props: { client: Client; open: Signal<boolean> }) {
  const [open, setOpen] = props.open;

  return (
    <Dialog id="delete-account" open={open()} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete Account</DialogTitle>
        </DialogHeader>
        Are you sure you want to proceed? The deletion is destructive and cannot
        be reverted.
        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Back
          </Button>

          <Button
            variant="destructive"
            onClick={() => {
              (async () => {
                try {
                  await props.client.deleteUser();
                  window.location.replace("/_/auth/login");
                } catch (err) {
                  showToast({
                    title: "User deletion",
                    description: `${err}`,
                    variant: "error",
                  });
                } finally {
                  setOpen(false);
                }
              })();
            }}
          >
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Avatar(props: { client: Client; user: User }) {
  const [failed, setFailed] = createSignal(false);

  let fileRef: HTMLInputElement | undefined;
  let formRef: HTMLFormElement | undefined;

  return (
    <form
      ref={formRef}
      method="dialog"
      enctype="multipart/form-data"
      class="my-4 flex items-center justify-between"
      onSubmit={async (ev: SubmitEvent) => {
        ev.preventDefault();

        const form = ev.currentTarget;
        if (form) {
          const formData = new FormData(form as HTMLFormElement);
          const response = await props.client.fetch(AVATAR_API, {
            method: "POST",
            body: formData,
          });

          if (response.ok) {
            window.location.reload();
          }
        }
      }}
    >
      <input
        hidden
        ref={fileRef}
        type="file"
        name="file"
        required
        accept="image/png, image/jpeg"
        onChange={(_e: Event) => {
          formRef!.requestSubmit();
        }}
      />

      <div class="relative">
        <button
          class="rounded-sm bg-gray-100 p-2 hover:bg-gray-200"
          onClick={() => fileRef!.click()}
        >
          <object
            class="rounded-sm"
            type="image/jpeg"
            data={avatarUrl(props.user)}
            width={60}
            height={60}
            aria-label="Avatar image"
            onError={() => {
              setFailed(true);
            }}
          >
            {/* Fallback */}
            <TbFillUser size={60} color="#0073aa" />
          </object>

          <div class="absolute right-1 bottom-1">
            <TbOutlineEdit />
          </div>
        </button>

        <Show when={!failed()}>
          <div class="absolute top-1 right-1">
            <button
              class={cn(DESTRUCTIVE_ICON_STYLE, "rounded-full bg-white/75")}
              onClick={async () => {
                const response = await props.client.fetch(AVATAR_API, {
                  method: "DELETE",
                });
                if (response.ok) {
                  window.location.reload();
                }
              }}
            >
              <TbOutlineTrash />
            </button>
          </div>
        </Show>
      </div>
    </form>
  );
}

function ProfileTable(props: { client: Client; user: User }) {
  const [deleteAccountOpen, setDeleteAccountOpen] = createSignal(false);

  return (
    <Card class="w-[80dvw] max-w-[540px] p-8">
      <div class="flex items-center justify-between">
        <h1>User Profile</h1>

        <div class="flex items-center gap-2">
          <DropdownMenu>
            <DropdownMenuTrigger class={cn(ICON_STYLE, "size-[32px]")}>
              <TbOutlineMenu2 />
            </DropdownMenuTrigger>

            <DropdownMenuContent>
              <a href="/_/auth/change_email">
                <DropdownMenuItem>Change Email</DropdownMenuItem>
              </a>

              <a href="/_/auth/change_password">
                <DropdownMenuItem>Change Password</DropdownMenuItem>
              </a>

              <DropdownMenuItem>Register 2nd Factor</DropdownMenuItem>
              <DropdownMenuSeparator />

              <DropdownMenuItem
                class="data-[highlighted]:bg-destructive"
                onClick={() => setDeleteAccountOpen((old) => !old)}
              >
                <TbOutlineTrash /> Delete Account
              </DropdownMenuItem>

              <Show when={import.meta.env.DEV}>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={() => {
                    throw Error("Exception");
                  }}
                >
                  Throw (DEV)
                </DropdownMenuItem>
              </Show>
            </DropdownMenuContent>
          </DropdownMenu>

          <DeleteAccountDialog
            client={props.client}
            open={[deleteAccountOpen, setDeleteAccountOpen]}
          />

          <a class={cn(ICON_STYLE)} href={`${HOST}/_/auth/logout`}>
            <TbOutlineLogout />
          </a>
        </div>
      </div>

      <div class="flex w-full items-center gap-4">
        <Avatar client={props.client} user={props.user} />

        <div class="flex flex-col gap-2">
          <strong>{props.user.email}</strong>

          <div>Id: {props.user.id}</div>
        </div>
      </div>

      <div class="my-4 flex w-full flex-col items-end gap-2">
        <TotpToggleButton {...props} />
      </div>
    </Card>
  );
}

export function Profile() {
  const client = useStore($client);

  return (
    <ErrorBoundary>
      <Switch fallback={<div>Something went wrong</div>}>
        <Match when={client() === undefined}>
          <div>Loading...</div>
        </Match>

        <Match when={client()?.user() === undefined}>
          <a
            class={buttonVariants({ variant: "default" })}
            href="/_/auth/login"
          >
            Login
          </a>
        </Match>

        <Match when={client()?.user()}>
          <ProfileTable client={client()!} user={client()!.user()!} />
        </Match>
      </Switch>
    </ErrorBoundary>
  );
}

const ICON_STYLE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "rounded-md",
  "p-2",
  "hover:text-primary-foreground",
  "hover:bg-primary/90",
  "data-[expanded]:text-primary-foreground",
  "data-[expanded]:bg-primary/90",
];

const DESTRUCTIVE_ICON_STYLE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "rounded-md",
  "p-2",
  "hover:text-primary-foreground",
  "hover:bg-destructive/90",
];
