import { createSignal } from "solid-js";
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
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");

  const urlParams = new URLSearchParams(window.location.search);
  const message = urlParams.get("loginMessage");

  const onSubmit = async () => {
    try {
      await client.login(username(), password());
    } catch (err) {
      showToast({
        title: "Uncaught Error",
        description: `${err}`,
        variant: "error",
      });
    }
    // Don't reload.
    return false;
  };

  return (
    <div class="flex h-dvh flex-col items-center justify-center">
      <Card>
        <form
          class="flex flex-col gap-4 px-8 py-12"
          method="dialog"
          onSubmit={onSubmit}
        >
          <h1>Login</h1>

          <TextField class="flex items-center gap-2">
            <TextFieldLabel class="w-[108px]">E-mail</TextFieldLabel>

            <TextFieldInput
              type="email"
              value={username()}
              placeholder="E-mail"
              onKeyUp={(e: KeyboardEvent) => {
                const target = e.currentTarget as HTMLInputElement;
                setUsername(target.value);
              }}
            />
          </TextField>

          <TextField class="flex items-center gap-2">
            <TextFieldLabel class="w-[108px]">Password</TextFieldLabel>

            <TextFieldInput
              type="password"
              value={password()}
              placeholder="password"
              onKeyUp={(e: KeyboardEvent) => {
                const target = e.currentTarget as HTMLInputElement;
                setPassword(target.value);
              }}
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
