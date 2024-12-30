import { createSignal } from "solid-js";
import { client } from "@/lib/fetch";

import { showToast } from "@/components/ui/toast";
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
    <div class="h-dvh flex flex-col items-center justify-center">
      <Card>
        <form
          class="flex flex-col gap-4 py-12 px-8"
          onSubmit={onSubmit}
          action="javascript:void(0);"
        >
          <h1>Login</h1>

          <TextField class="flex gap-2 items-center">
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

          <TextField class="flex gap-2 items-center">
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
        </form>
      </Card>
    </div>
  );
}
