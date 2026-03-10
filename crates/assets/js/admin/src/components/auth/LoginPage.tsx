import { createEffect, createSignal, Match, Show, Switch } from "solid-js";
import type { Setter, Signal } from "solid-js";
import { useStore } from "@nanostores/solid";
import { FetchError, type MultiFactorAuthCallback } from "trailbase";
import { createWritableMemo } from "@solid-primitives/memo";

import { client, $user } from "@/lib/client";

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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";

export function LoginPage() {
  const user = useStore($user);

  createEffect(() => {
    console.debug(`current user: ${JSON.stringify(user())}`);
  });

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

const loginOptions = ["Password", "OTP"] as const;
type LoginOptions = (typeof loginOptions)[number];

function LoginForm() {
  const [mfaCallback, setMfaCallback] =
    createWritableMemo<MultiFactorAuthCallback | null>(() => null);
  const [loginType, setLoginType] = createSignal<LoginOptions>("Password");
  const [otpSent, setOtpSent] = createSignal<string | null>(null);

  const title = (): string => {
    if (mfaCallback() !== null) {
      return "Enter Authenticator Code";
    }
    return "Login";
  };

  return (
    <>
      <CardHeader>
        <div class="flex items-center justify-between gap-2">
          <CardTitle>{title()}</CardTitle>

          <Show when={mfaCallback() === null}>
            <Select
              multiple={false}
              options={[...loginOptions]}
              value={loginType()}
              itemComponent={(props) => (
                <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
              )}
              onChange={(option: LoginOptions | null) => {
                if (option !== null) {
                  setLoginType(option);
                }
              }}
            >
              <SelectTrigger>
                <SelectValue<string>>
                  {(state) => state.selectedOption()}
                </SelectValue>
              </SelectTrigger>

              <SelectContent />
            </Select>
          </Show>
        </div>
      </CardHeader>

      <CardContent>
        <Switch>
          <Match when={mfaCallback() !== null}>
            <MfaLoginForm mfaCallback={mfaCallback()!} />
          </Match>

          <Match when={loginType() === "OTP"}>
            <OtpLoginForm otpSent={[otpSent, setOtpSent]} />
          </Match>

          <Match when={true}>
            <PasswordLoginForm setMfaCallback={setMfaCallback} />
          </Match>
        </Switch>
      </CardContent>
    </>
  );
}

function PasswordLoginForm(props: {
  setMfaCallback: Setter<MultiFactorAuthCallback | null>;
}) {
  let passwordInput: HTMLInputElement | undefined;
  let userInput: HTMLInputElement | undefined;

  const urlParams = new URLSearchParams(window.location.search);
  const message = urlParams.get("loginMessage");

  return (
    <form
      class="flex flex-col gap-4"
      method="dialog"
      onSubmit={async (ev: SubmitEvent) => {
        ev.preventDefault();

        const email = userInput?.value;
        const pw = passwordInput?.value;
        if (!email || !pw) return;

        try {
          const mfaCallback = await client.login(email, pw);
          if (mfaCallback !== undefined) {
            props.setMfaCallback(() => mfaCallback.callback);
          }
        } catch (err) {
          if (err instanceof FetchError && err.status === 401) {
            showToast({
              title: "Invalid credentials",
              variant: "warning",
              duration: 5 * 1000,
            });
          } else if (err instanceof FetchError && err.status === 429) {
            showToast({
              title: `Too many login attempts for ${email}`,
              description: "Try again later",
              variant: "warning",
              duration: 5 * 1000,
            });
          } else {
            showToast({
              title: "Other Error",
              description: `${err}`,
              variant: "error",
            });
          }
        }
      }}
    >
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

function OtpLoginForm(props: { otpSent: Signal<string | null> }) {
  let userInput: HTMLInputElement | undefined;
  let otpInput: HTMLInputElement | undefined;

  // eslint-disable-next-line solid/reactivity
  const [otpSent, setOtpSent] = props.otpSent;

  const urlParams = new URLSearchParams(window.location.search);
  const message = urlParams.get("loginMessage");

  async function requestOtp(email: string) {
    await client.requestOtp(email, { redirectUri: "/_/admin" });
    setOtpSent(email);
  }

  async function login(email: string, otp: string) {
    await client.loginOtp(email, otp);
  }

  return (
    <form
      class="flex flex-col gap-4"
      method="dialog"
      onSubmit={async (ev: SubmitEvent) => {
        ev.preventDefault();

        const email = userInput?.value;
        if (!email) return;

        const otp = otpInput?.value;

        try {
          if (!otp) {
            await requestOtp(email);
          } else {
            await login(email, otp);
          }
        } catch (err) {
          if (err instanceof FetchError && err.status === 405) {
            showToast({
              title: "OTP Login Disabled",
              variant: "error",
              duration: 5 * 1000,
            });
          } else {
            showToast({
              title: "Other Error",
              description: `${err}`,
              variant: "error",
            });
          }
        }
      }}
    >
      <TextField class="flex items-center gap-2">
        <TextFieldLabel class="w-[108px]">Email</TextFieldLabel>

        <TextFieldInput
          type="email"
          placeholder="Email"
          autocomplete="username"
          required={true}
          ref={userInput}
          disabled={otpSent() !== null}
        />
      </TextField>

      <Show when={otpSent()}>
        <TextField class="flex items-center gap-2">
          <TextFieldLabel class="w-[108px]">Code</TextFieldLabel>

          <TextFieldInput
            type="text"
            placeholder="OTP"
            autocomplete="off"
            required={true}
            ref={otpInput}
          />
        </TextField>
      </Show>

      <div class="flex justify-end">
        <Button type="submit">{otpSent() ? "Log in" : "Request OTP"}</Button>
      </div>

      <Show when={message}>
        <div class="flex justify-center">
          <Badge variant="warning">{message}</Badge>
        </div>
      </Show>
    </form>
  );
}

function MfaLoginForm(props: { mfaCallback: MultiFactorAuthCallback }) {
  let totpInput: HTMLInputElement | undefined;

  const urlParams = new URLSearchParams(window.location.search);
  const message = urlParams.get("loginMessage");

  return (
    <form
      class="flex flex-col gap-4"
      method="dialog"
      onSubmit={async (ev: SubmitEvent) => {
        ev.preventDefault();

        const userTotp = totpInput?.value;
        if (!userTotp) return;

        try {
          props.mfaCallback(userTotp);
        } catch (err) {
          if (err instanceof FetchError && err.status === 401) {
            showToast({
              title: "Invalid TOTP",
              variant: "warning",
              duration: 5 * 1000,
            });
          } else {
            showToast({
              title: "Other Error",
              description: `${err}`,
              variant: "error",
            });
          }
        }
      }}
    >
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
