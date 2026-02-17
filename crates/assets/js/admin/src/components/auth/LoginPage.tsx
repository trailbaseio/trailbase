import { Match, Switch } from "solid-js";
import { useStore } from "@nanostores/solid";
import { FetchError } from "trailbase";

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
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";

import { createSignal } from "solid-js";

export function LoginPage() {
  const user = useStore($user);
  const [loginType, setLoginType] = createSignal("pw");
  const [email, setEmail] = createSignal("");
  const [pw, setPassword] = createSignal("");
  const [otp, setOtp] = createSignal("");

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

          <Match when={user() === undefined && loginType() === "pw"}>
            <LoginForm
              setLoginType={setLoginType}
              setEmail={setEmail}
              setPassword={setPassword}
            />
          </Match>
          <Match when={user() === undefined && loginType() === "otp"}>
            <OTPVerification
              setLoginType={setLoginType}
              email={email()}
              setOtp={setOtp}
            />
          </Match>
          <Match when={user() === undefined && loginType() === "totp"}>
            <TOTPVerification
              setLoginType={setLoginType}
              email={email()}
              pw={pw()}
              otp={otp()}
            />
          </Match>
        </Switch>
      </Card>
    </div>
  );
}

type LoginFormProps = {
  setEmail: (email: string) => void;
  setLoginType: (type: "pw" | "otp" | "totp") => void;
  setPassword: (password: string) => void;
};

function LoginForm(props: LoginFormProps) {
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
          if (pw) await client.login(email, pw);
        } catch (err: any) {
          if (err.message === "TOTP_REQUIRED") {
            props.setEmail(email);
            props.setPassword(pw);
            props.setLoginType("totp");
          } else if (err instanceof FetchError && err.status === 401) {
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
          ref={userInput}
        />
      </TextField>

      <TextField class="flex items-center gap-2">
        <TextFieldLabel class="w-[108px]">Password</TextFieldLabel>

        <TextFieldInput
          type="password"
          placeholder="password"
          autocomplete="current-password"
          ref={passwordInput}
        />
      </TextField>

      <div class="flex justify-end gap-2">
        <Button
          type="button"
          onClick={() => {
            const email = userInput?.value;
            if (!email) return;
            client.requestOTP(email).then(() => {
              showToast({
                title: "OTP Sent",
                description: `An OTP code has been sent to ${email} if an account with that email exists.`,
                variant: "success",
              });
              props.setEmail(email);
              props.setLoginType("otp");
            }).catch((err: any) => {
              showToast({
                title: "Error requesting OTP",
                description: `${err}`,
                variant: "error",
              });
            });
          }}
        >
          Request OTP
        </Button>
        <Button type="submit">Log in</Button>
      </div>

      {message && (
        <div class="flex justify-center">
          <Badge variant="warning">{message}</Badge>
        </div>
      )}
    </form>
  );
}

type OTPVerificationProps = {
  email: string;
  setOtp: (otp: string) => void;
  setLoginType: (type: "pw" | "otp" | "totp") => void;
};

function OTPVerification(props: OTPVerificationProps) {
  let otpInput: HTMLInputElement | undefined;

  return (
    <form
      class="flex flex-col gap-4 px-8 py-12"
      method="dialog"
      onSubmit={async (ev: SubmitEvent) => {
        ev.preventDefault();

        const otp = otpInput?.value;
        if (!props.email || !otp) return;

        try {
          await client.verifyOTP(props.email, otp);
        } catch (err: any) {
          if (err.message === "TOTP_REQUIRED") {
            props.setOtp(otp);
            props.setLoginType("totp");
          } else {
            showToast({
              title: "Error verifying OTP",
              description: `${err}`,
              variant: "error",
            });
          }
        }
      }}
    >
      <h1>OTP Login</h1>
      <p>Please enter the OTP code sent to your email <b>{props.email}</b>.</p>

      <TextField class="flex items-center gap-2">
        <TextFieldLabel>Code</TextFieldLabel>

        <TextFieldInput
          type="text"
          ref={otpInput}
        />
      </TextField>

      <div class="flex justify-end gap-2">
        <Button type="button" onClick={() => props.setLoginType("pw")}>Back</Button>
        <Button type="submit">Verify OTP</Button>
      </div>
    </form>
  );
}

type TOTPVerificationProps = {
  email: string;
  otp: string;
  pw: string;
  setLoginType: (type: "pw" | "otp" | "totp") => void;
};

function TOTPVerification(props: TOTPVerificationProps) {
  let totpInput: HTMLInputElement | undefined;

  return (
    <form
      class="flex flex-col gap-4 px-8 py-12"
      method="dialog"
      onSubmit={async (ev: SubmitEvent) => {
        ev.preventDefault();

        const totp = totpInput?.value;
        if (!props.email || !totp) return;

        try {
          await client.verifyTOTP(props.email, totp, props.pw, props.otp);
        } catch (err: any) {
          showToast({
            title: "Error verifying TOTP",
            description: `${err}`,
            variant: "error",
          });
        }
      }}
    >
      <h1>TOTP Verification</h1>
      <p>Please enter the TOTP code from your authenticator app.</p>

      <TextField class="flex items-center gap-2">
        <TextFieldLabel class="w-[108px]">Code</TextFieldLabel>

        <TextFieldInput
          type="text"
          ref={totpInput}
          autocomplete="one-time-code" 
        />
      </TextField>

      <div class="flex justify-end gap-2">
        <Button type="button" onClick={() => props.setLoginType("otp")}>Back</Button>
        <Button type="submit">Verify TOTP</Button>
      </div>
    </form>
  );
}