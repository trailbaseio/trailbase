import { Match, Show, Switch } from "solid-js";
import type { Setter } from "solid-js";
import { useStore } from "@nanostores/solid";
import { FetchError } from "trailbase";
import { createWritableMemo } from "@solid-primitives/memo";

import { client, $user } from "@/lib/client";
import { MfaTokenResponse } from "@bindings/MfaTokenResponse";

import { Profile } from "@/components/auth/Profile";
import { showToast } from "@/components/ui/toast";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardFooter,
} from "@/components/ui/card";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";

export function LoginPage() {
  const user = useStore($user);

  return (
    <div class="flex h-dvh flex-col items-center justify-center">
      <Card>
        <Switch>
          <Match when={user() !== undefined}>
            <CardHeader>
              <CardTitle>NOT AN ADMIN</CardTitle>
            </CardHeader>

            <CardContent>
              <Profile user={user()!} showId={false} />
            </CardContent>

            <CardFooter class="flex w-full justify-end">
              <Button type="button" onClick={() => client.logout()}>
                Logout
              </Button>
            </CardFooter>
          </Match>

          <Match when={user() === undefined}>
            <LoginForm />
          </Match>
        </Switch>
      </Card>
    </div>
  );
}

function LoginForm() {
  const [mfaToken, setMfaToken] = createWritableMemo<MfaTokenResponse | null>(
    () => null,
  );

  return (
    <Switch>
      <Match when={mfaToken() === null}>
        <PasswordLoginForm setMfaToken={setMfaToken} />
      </Match>

      <Match when={mfaToken() !== null}>
        <MfaLoginForm mfaToken={mfaToken()!} />
      </Match>
    </Switch>
  );
}

function PasswordLoginForm(props: {
  setMfaToken: Setter<MfaTokenResponse | null>;
}) {
  let passwordInput: HTMLInputElement | undefined;
  let userInput: HTMLInputElement | undefined;

  const urlParams = new URLSearchParams(window.location.search);
  const message = urlParams.get("loginMessage");

  return (
    <form
      class="flex flex-col gap-4 px-8 py-12"
      method="dialog"
      onSubmit={async (ev: SubmitEvent) => {
        ev.preventDefault();

        const email = userInput?.value;
        const pw = passwordInput?.value;
        if (!email || !pw) return;

        try {
          await client.login(email, pw);
        } catch (err) {
          if (err instanceof FetchError && err.status === 401) {
            showToast({
              title: "Invalid credentials",
              variant: "warning",
              duration: 5 * 1000,
            });
          } else if (err instanceof FetchError && err.status === 403) {
            // MFA is needed. Flip to next form.
            props.setMfaToken(JSON.parse(err.message) as MfaTokenResponse);
          } else if (err instanceof FetchError && err.status === 429) {
            showToast({
              title: `Too many login attempts for ${email}`,
              description: "Try again later",
              variant: "warning",
              duration: 5 * 1000,
            });
          } else {
            showToast({
              title: "Uncaught Error",
              description: `${err}`,
              variant: "error",
            });
          }
        }
      }}
    >
      <h1>Login</h1>

      <TextField class="flex items-center gap-2">
        <TextFieldLabel class="w-[108px]">Email</TextFieldLabel>

        <TextFieldInput
          type="email"
          placeholder="Email"
          autocomplete="username"
          required={true}
          ref={userInput}
        />
      </TextField>

      <TextField class="flex items-center gap-2">
        <TextFieldLabel class="w-[108px]">Password</TextFieldLabel>

        <TextFieldInput
          type="password"
          placeholder="password"
          autocomplete="current-password"
          required={true}
          ref={passwordInput}
        />
      </TextField>

      <div class="flex justify-end">
        <Button type="submit">Log in</Button>
      </div>

      <Show when={message}>
        <div class="flex justify-center">
          <Badge variant="warning">{message}</Badge>
        </div>
      </Show>
    </form>
  );
}

function MfaLoginForm(props: { mfaToken: MfaTokenResponse }) {
  let totpInput: HTMLInputElement | undefined;

  const urlParams = new URLSearchParams(window.location.search);
  const message = urlParams.get("loginMessage");

  return (
    <form
      class="flex flex-col gap-4 px-8 py-12"
      method="dialog"
      onSubmit={async (ev: SubmitEvent) => {
        ev.preventDefault();

        const userTotp = totpInput?.value;
        if (!userTotp) return;

        try {
          await client.login({
            mfaToken: props.mfaToken.mfa_token,
            totp: userTotp,
          });
        } catch (err) {
          if (err instanceof FetchError && err.status === 401) {
            showToast({
              title: "Invalid TOTP",
              variant: "warning",
              duration: 5 * 1000,
            });
          } else {
            showToast({
              title: "Uncaught Error",
              description: `${err}`,
              variant: "error",
            });
          }
        }
      }}
    >
      <h1>Enter TOTP</h1>

      <TextField class="flex items-center gap-2">
        <TextFieldLabel class="w-[108px]">Code</TextFieldLabel>

        <TextFieldInput
          type="text"
          placeholder="Code"
          autocomplete="new-password"
          required={true}
          ref={totpInput}
        />
      </TextField>

      <div class="flex justify-end">
        <Button type="submit">Submit</Button>
      </div>

      <Show when={message}>
        <div class="flex justify-center">
          <Badge variant="warning">{message}</Badge>
        </div>
      </Show>
    </form>
  );
}
