import { createSignal, Switch, Match } from "solid-js";
import { TbUser, TbLogout, TbTrash } from "solid-icons/tb";
import { useStore } from "@nanostores/solid";
import { Client, type User } from "trailbase";

import { HOST, AVATAR_API } from "@/lib/constants";
import { $client } from "@/lib/client";
import { cn } from "@/lib/utils";

import { Button, buttonVariants } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { showToast } from "@/components/ui/toast";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";

function avatarUrl(user: User): string {
  return `${AVATAR_API}/${user.id}`;
}

function DeleteAccountButton(props: { client: Client }) {
  const [open, setOpen] = createSignal<boolean>(false);

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <DialogTrigger>
        <div class={cn(DESTRUCTIVE_ICON_STYLE)}>
          <TbTrash />
        </div>
      </DialogTrigger>

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
            onClick={async () => {
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
        <button class="rounded bg-white p-2" onClick={() => fileRef!.click()}>
          <object
            class="rounded"
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
            <TbUser size={60} color="#0073aa" />
          </object>
        </button>

        {!failed() && (
          <div class="absolute right-0 top-0">
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
              <TbTrash />
            </button>
          </div>
        )}
      </div>
    </form>
  );
}

function ProfileTable(props: { client: Client; user: User }) {
  return (
    <Card class="w-[80dvw] max-w-[460px] p-8">
      <div class="flex items-center justify-between">
        <h1>User Profile</h1>

        <div class="flex items-center gap-2">
          <DeleteAccountButton client={props.client} />

          <a class={cn(ICON_STYLE)} href={`${HOST}/_/auth/logout`}>
            <TbLogout />
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

      <div class="my-4 flex items-end gap-2">
        <a
          class={buttonVariants({ variant: "outline" })}
          href="/_/auth/change_email"
        >
          Change E-Mail
        </a>

        <a
          class={buttonVariants({ variant: "outline" })}
          href="/_/auth/change_password"
        >
          Change Password
        </a>
      </div>

      {import.meta.env.DEV && (
        <div class="flex justify-center">
          <Button
            variant="destructive"
            onClick={() => {
              throw Error("Exception");
            }}
          >
            Throw (DEV)
          </Button>
        </div>
      )}
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
