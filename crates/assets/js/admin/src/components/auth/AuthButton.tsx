import { createSignal, Show } from "solid-js";
import { useStore } from "@nanostores/solid";
import { Copy } from "lucide-solid";
import { client, $user } from "@/lib/client";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { navbarIconStyle } from "@/components/Navbar";
import { Avatar, Profile } from "@/components/auth/Profile";
import { QRCodeSVG } from "solid-qr-code";
import { showToast } from "../ui/toast";
import { TextField, TextFieldInput, TextFieldLabel } from "../ui/text-field";

export function AuthButton(props: { iconSize: number }) {
  const [open, setOpen] = createSignal(false);
  const [totpSecret, setTotpSecret] = createSignal<string | null>(null);
  const [totpUri, setTotpUri] = createSignal<string | null>(null);
  const user = useStore($user);
  let totpInput: HTMLInputElement | undefined;

  const enableTotp = async () => {
    try {
      const res = await client.generateTOTP();
      setTotpSecret(res.secret);
      setTotpUri(res.qr_code_uri);
    } catch (err) {
      showToast({
        title: "Error generating OTP",
        description: `${err}`,
        variant: "error",
      });
    }
  };

  const disableTotp = async () => {
    try {
      const totp = prompt("Enter current TOTP code to disable 2FA");
      if (!totp) return;
      await client.disableTOTP(totp);
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
  };

  const confirmTotp = async () => {
    try {
      const totp = totpInput?.value;
      if (!totpSecret() || !totp) return;

      await client.confirmTOTP(totpSecret()!, totp);
      showToast({
        title: "TOTP confirmed",
        description: "Two-factor authentication has been enabled.",
        variant: "success",
      });
      setTotpSecret(null);
      setTotpUri(null);
    } catch (err) {
      showToast({
        title: "Error confirming TOTP",
        description: `${err}`,
        variant: "error",
      });
    }
  };

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <button class={navbarIconStyle} onClick={() => setOpen(true)}>
        <Avatar user={user()} size={props.iconSize} />
      </button>

      <DialogContent class="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle>Logged in</DialogTitle>
        </DialogHeader>

        <Show when={user()}>
          <Profile user={user()!} />
        </Show>
        
        <Show when={totpSecret()}>
          <div class="flex flex-col items-center gap-4 p-4 border rounded-md bg-muted/50">
            <h3 class="font-semibold text-lg">Scan QR Code</h3>
              <div class="bg-white p-2 rounded">
                <QRCodeSVG value={totpUri()!} />
              </div>
            <div class="flex flex-col items-center gap-1 text-sm">
              <span class="text-muted-foreground">Or enter secret manually:</span>
              <div class="flex items-center gap-2 font-mono bg-background px-3 py-1 rounded border">
                {totpSecret()}
                <button 
                  class="hover:text-primary transition-colors"
                  onClick={() => navigator.clipboard.writeText(totpSecret()!)}
                  title="Copy secret"
                >
                  <Copy class="w-4 h-4" />
                </button>
              </div>
            </div>
            <p class="text-center text-xs text-muted-foreground w-full">
              Scan this code with your authenticator app (Google Authenticator, Authy, etc.) to enable 2FA.
            </p>
            <TextField class="flex items-center gap-2">
              <TextFieldLabel>TOTP</TextFieldLabel>
              <TextFieldInput
                type="text"
                autocomplete="one-time-code" 
                ref={totpInput}
              />
            </TextField>
            <Button type="button" onClick={confirmTotp}>
              Confirm TOTP
            </Button>
          </div>
        </Show>

        <DialogFooter>
          <Show when={!totpSecret()}>
            <Button type="button" onClick={enableTotp}>
              Generate TOTP
            </Button>
            <Button type="button" variant="outline" onClick={disableTotp}>
              Disable TOTP
            </Button>
          </Show>
          <Button type="button" variant="outline" onClick={() => client.logout()}>
            Logout
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
