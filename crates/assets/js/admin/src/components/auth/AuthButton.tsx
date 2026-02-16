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

export function AuthButton(props: { iconSize: number }) {
  const [open, setOpen] = createSignal(false);
  const [totpSecret, setTotpSecret] = createSignal<string | null>(null);
  const [totpUri, setTotpUri] = createSignal<string | null>(null);
  const user = useStore($user);

  const enableTotp = async () => {
    try {
      const res = await client.generateTOTP();
      setTotpSecret(res.secret);
      setTotpUri(res.qr_code_uri);
    } catch (e) {
      alert("Failed to generate TOTP secret");
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
          </div>
        </Show>

        <DialogFooter>
          <Show when={!totpSecret()}>
            <Button type="button" onClick={enableTotp}>
              Generate TOTP
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
