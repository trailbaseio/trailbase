import { createResource, createSignal, Switch, Match } from "solid-js";
import { TbUser, TbLogout, TbTrash } from "solid-icons/tb";
import { Client, type User } from "trailbase";

import {
  HOST,
  RECORD_API,
  OUTLINE_BUTTON_STYLE,
  ICON_STYLE,
  DESTRUCTIVE_ICON_STYLE,
} from "@/lib/constants";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
// import {
//   TextField,
//   TextFieldLabel,
//   TextFieldInput,
// } from "@/components/ui/text-field";

function DeleteAccountButton(props: { client: Client }) {
  const [open, setOpen] = createSignal<boolean>(false);

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <DialogTrigger>
        <div class={DESTRUCTIVE_ICON_STYLE.join(" ")}>
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
            onClick={() =>
              (async () => {
                await props.client.deleteUser();
                setOpen(false);
                window.location.replace("/_/auth/login");
              })().catch(console.error)
            }
          >
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// function ChangeEmailButton(props: { oldEmail: string; client: Client }) {
//   const [open, setOpen] = createSignal<boolean>(false);
//   const [email, setEmail] = createSignal(props.oldEmail);
//
//   return (
//     <Dialog open={open()} onOpenChange={setOpen}>
//       <DialogTrigger>
//         <Button variant="outline">Change E-mail</Button>
//       </DialogTrigger>
//
//       <DialogContent>
//         <DialogHeader>
//           <DialogTitle>Change E-mail</DialogTitle>
//         </DialogHeader>
//
//         <TextField class="flex items-center gap-2">
//           <TextFieldLabel class="w-28">New E-mail</TextFieldLabel>
//
//           <TextFieldInput
//             required
//             type="email"
//             value={email()}
//             onKeyUp={(e: Event) => {
//               const v = (e.currentTarget as HTMLInputElement).value;
//               setEmail(v);
//             }}
//           />
//         </TextField>
//
//         <DialogFooter>
//           <Button variant="outline" onClick={() => setOpen(false)}>
//             Back
//           </Button>
//
//           <Button
//             variant="default"
//             disabled={email() === props.oldEmail}
//             onClick={async () => {
//               if (email() !== props.oldEmail) {
//                 await props.client.changeEmail(email());
//               }
//               setOpen(false);
//             }}
//           >
//             Send Verification E-mail
//           </Button>
//         </DialogFooter>
//       </DialogContent>
//     </Dialog>
//   );
// }

function Avatar(props: { avatarUrl?: () => string | undefined }) {
  const url = () => props.avatarUrl?.();

  const AvatarImage = () => {
    return (
      <Switch fallback={<TbUser size={60} />}>
        <Match when={url()}>
          <img class="size-[60px]" alt="user avatar" src={url()} />
        </Match>
      </Switch>
    );
  };

  const profilePageUrl = `${window.location.origin}/_/auth/profile`;
  const actionUrl = `${RECORD_API}/_user_avatar?redirect_to=${profilePageUrl}`;

  const openFileDialog = () => {
    try {
      const element = document.getElementById("file-input") as HTMLInputElement;
      element.click();
    } catch (err) {
      console.debug(err);
    }
  };

  return (
    <form
      id="avatar-form"
      method="post"
      action={actionUrl}
      enctype="multipart/form-data"
      target="_self"
      class="my-4 flex items-center justify-between"
    >
      {/* NOTE: user().id is a UUID rather than a b64 string.
        <input type="hidden" name="user" value={`${user().id}`} />
      */}
      <input
        hidden
        id="file-input"
        type="file"
        name="file"
        required
        accept="image/png, image/jpeg"
        onChange={(e: Event) => {
          const v = (e.currentTarget as HTMLInputElement).value;
          if (v) {
            const el = document.getElementById(
              "avatar-form",
            ) as HTMLFormElement;
            if (el) {
              el.submit();
            }
          }
        }}
      />

      <button class="bg-gray-200 p-2" onClick={openFileDialog}>
        <AvatarImage />
      </button>

      {/*
        <Button onClick={openFileDialog}>Set Avatar</Button>
      */}
    </form>
  );
}

function ProfileTable(props: {
  user: User;
  client: Client;
  avatarUrl?: () => string | undefined;
}) {
  const user = () => props.user;

  return (
    <Card class="w-[80dvw] max-w-[460px] p-8">
      <div class="flex items-center justify-between">
        <h1>User Profile</h1>

        <div class="flex items-center gap-2">
          <DeleteAccountButton client={props.client} />

          <a class={ICON_STYLE.join(" ")} href="/_/auth/logout">
            <TbLogout />
          </a>
        </div>
      </div>

      <div class="flex w-full items-center gap-4">
        <Avatar avatarUrl={props.avatarUrl} />

        <div class="flex flex-col gap-2">
          <strong>{user().email}</strong>

          <div>Id: {user().id}</div>
        </div>
      </div>

      <div class="my-4 flex items-end gap-2">
        <a class={OUTLINE_BUTTON_STYLE.join(" ")} href="/_/auth/change_email">
          Change E-Mail
        </a>

        <a
          class={OUTLINE_BUTTON_STYLE.join(" ")}
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
  // FIXME: This is ugly, that state management should be simpler. One option
  // might be to return synchronously from tryFromCookies and call onAuthChange
  // async later.
  const [user, setUser] = createSignal<User | undefined>();
  const [client] = createResource(async () => {
    return Client.tryFromCookies(HOST, {
      onAuthChange: (_client, user) => setUser(user),
    });
  });

  const [avatarUrl] = createResource(
    client,
    async (c: Client) => await c.avatarUrl(),
  );

  return (
    <ErrorBoundary>
      <Switch fallback={<div>Loading...</div>}>
        <Match when={client.error}>
          <span>{`${client.error}`}</span>

          <a href="/_/auth/login/">
            <Button>To Login</Button>
          </a>
        </Match>

        <Match when={client() && user()}>
          <ProfileTable
            user={user()!}
            client={client()!}
            avatarUrl={avatarUrl}
          />
        </Match>

        <Match when={client() && !user()}>Not logged in.</Match>
      </Switch>
    </ErrorBoundary>
  );
}
