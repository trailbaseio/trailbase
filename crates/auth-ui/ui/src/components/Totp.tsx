import { createSignal, Match, Show, Switch } from "solid-js";
import { TbOutlineCopy } from "solid-icons/tb";
import type { Client, User } from "trailbase";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { showToast } from "@/components/ui/toast";
import {
  TextField,
  TextFieldInput,
  TextFieldLabel,
} from "@/components/ui/text-field";

interface Totp {
  url: string;
  png: string;
}

export function TotpToggleButton(props: { client: Client; user: User }) {
  const hasTotp = () => props.user.mfa ?? false;
  const [totp, setTotp] = createSignal<Totp | null>(null);

  async function registerTotp() {
    try {
      const res = await props.client.registerTOTP({ png: true });
      setTotp({
        url: res.totp_url,
        png: res.png ?? "",
      });
    } catch (err) {
      showToast({
        title: "Error generating OTP",
        description: `${err}`,
        variant: "error",
      });
    }
  }

  async function disableTotp() {
    try {
      // TODO: Better UI than browser prompt.
      const totp = prompt("Enter current TOTP code to disable 2FA");

      await props.client.unregisterTOTP(totp ?? "");
      showToast({
        title: "TOTP disabled",
        description: "Two-factor authentication has been disabled.",
        variant: "success",
      });
    } catch (err) {
      showToast({
        title: "Error disabling TOTP",
        description: `${err}`,
        variant: "error",
      });
    }
  }

  return (
    <>
      <Switch>
        <Match when={hasTotp()}>
          <Button type="button" variant="outline" onClick={disableTotp}>
            Disable TOTP
          </Button>
        </Match>

        <Match when={!hasTotp()}>
          <Button type="button" onClick={registerTotp}>
            Register TOTP
          </Button>
        </Match>
      </Switch>

      <Dialog
        id="totp-dialog"
        open={totp() !== null}
        onOpenChange={(open) => {
          if (!open) {
            setTotp(null);
          }
        }}
      >
        <DialogContent class="sm:max-w-[500px]">
          <Show when={totp() !== null}>
            <RegisterTotpDialog
              client={props.client}
              totp={totp()!}
              close={() => setTotp(null)}
            />
          </Show>
        </DialogContent>
      </Dialog>
    </>
  );
}

function RegisterTotpDialog(props: {
  client: Client;
  totp: Totp;
  close: () => void;
}) {
  let totpInput: HTMLInputElement | undefined;

  const secret = () => {
    return URL.parse(props.totp.url)?.searchParams.get("secret");
  };

  async function onSubmit(_ev: SubmitEvent) {
    try {
      const userTotp = totpInput?.value;
      if (!userTotp) {
        return;
      }

      await props.client.confirmTOTP(props.totp.url, userTotp);
      showToast({
        title: "TOTP confirmed",
        description: "Two-factor authentication has been enabled.",
        variant: "success",
      });
      props.close();
    } catch (err) {
      showToast({
        title: "Error confirming TOTP",
        description: `${err}`,
        variant: "error",
      });
    }
  }

  return (
    <form
      method="dialog"
      onSubmit={onSubmit}
      class="bg-muted/50 m-4 flex flex-col items-center gap-4 rounded-md border p-4"
    >
      <DialogHeader>
        <DialogTitle>Scan QR Code</DialogTitle>
      </DialogHeader>

      <div class="rounded-sm bg-white p-2">
        <img src={`data:image/png;base64,${props.totp.png ?? ""}`} />
      </div>

      <div class="flex flex-col items-center gap-1">
        <p class="text-muted-foreground">or enter secret manually:</p>

        <div class="bg-background flex items-center gap-2 rounded-sm border px-3 py-1 font-mono">
          {secret()}

          <Button
            variant="ghost"
            size="icon"
            onClick={() => navigator.clipboard.writeText(secret() ?? "")}
          >
            <TbOutlineCopy />
          </Button>
        </div>
      </div>

      <p class="text-muted-foreground text-center text-xs">
        Scan this code with your authenticator app (Google Authenticator, Authy,
        etc.) to enable 2FA.
      </p>

      <TextField class="flex items-center gap-2">
        <TextFieldLabel>Code</TextFieldLabel>

        <TextFieldInput
          class="bg-white"
          required={true}
          type="text"
          autocomplete="new-password"
          ref={totpInput}
        />
      </TextField>

      <DialogFooter>
        <Button type="submit">Confirm</Button>
      </DialogFooter>
    </form>
  );
}
