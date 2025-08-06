import { client } from "@/lib/fetch";

import { showToast } from "@/components/ui/toast";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";

export function LoginPage() {
  let password: HTMLInputElement | undefined;
  let user: HTMLInputElement | undefined;

  const urlParams = new URLSearchParams(window.location.search);
  const message = urlParams.get("loginMessage");

  return (
    <div class="flex h-dvh flex-col items-center justify-center">
      <Card>
        <form
          class="flex flex-col gap-4 px-8 py-12"
          method="dialog"
          onSubmit={async (ev: SubmitEvent) => {
            ev.preventDefault();

            const email = user?.value;
            const pw = password?.value;
            if (!email || !pw) return;

            try {
              await client.login(email, pw);
            } catch (err) {
              showToast({
                title: "Uncaught Error",
                description: `${err}`,
                variant: "error",
              });
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
              ref={user}
            />
          </TextField>

          <TextField class="flex items-center gap-2">
            <TextFieldLabel class="w-[108px]">Password</TextFieldLabel>

            <TextFieldInput
              type="password"
              placeholder="password"
              autocomplete="current-password"
              ref={password}
            />
          </TextField>

          <div class="flex justify-end">
            <Button type="submit">Log in</Button>
          </div>

          {message && (
            <div class="flex justify-center">
              <Badge variant="warning">{message}</Badge>
            </div>
          )}
        </form>
      </Card>
    </div>
  );
}
