import { createSignal, Show } from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbOutlineClipboard } from "solid-icons/tb";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
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
import { TotpToggleButton } from "@/components/auth/Totp";

import { client, $user } from "@/lib/client";
import { showToast } from "../ui/toast";

export function AuthButton(props: { iconSize: number }) {
  const [open, setOpen] = createSignal(false);
  const user = useStore($user);

  const hasTotp = () => user()?.mfa ?? false;
  const base64Tokens = () => btoa(JSON.stringify(client.tokens()));

  return (
    <Dialog id="auth-dialog" open={open()} onOpenChange={setOpen}>
      <button class={navbarIconStyle} onClick={() => setOpen(true)}>
        <Avatar user={user()} size={props.iconSize} />
      </button>

      <DialogContent class="max-w-[95dvw]">
        <DialogHeader>
          <DialogTitle>Profile</DialogTitle>
        </DialogHeader>

        <Show when={user()}>
          <Profile user={user()!} twoFactorEnabled={hasTotp()} />

          {/*
          <Card>
            <CardContent class="pt-4">
              <div class="flex shrink items-center gap-4">
                <TbOutlineClipboard
                  class="hover:bg-accent hover:text-accent-foreground m-1 rounded"
                  size={40}
                  color="#0073aa"
                  onClick={() => {
                    navigator.clipboard.writeText(base64Tokens());
                    showToast({
                      title: `Copied to clipboard`,
                      variant: "success",
                    });
                  }}
                />

                <div class="h-20 overflow-y-scroll break-all">
                  {base64Tokens()}
                </div>
              </div>
            </CardContent>
          </Card>
          */}

          <Accordion multiple={false} collapsible>
            <AccordionItem class="border-none" value="foo">
              <AccordionTrigger>
                <div class="flex w-full items-center justify-between px-4">
                  <span>Tokens</span>

                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={(e) => {
                      e.stopPropagation();

                      navigator.clipboard.writeText(base64Tokens());
                      showToast({
                        title: `Copied to clipboard`,
                        variant: "success",
                      });
                    }}
                  >
                    <TbOutlineClipboard />
                  </Button>
                </div>
              </AccordionTrigger>

              <AccordionContent>
                <span class="h-20 overflow-y-scroll font-mono text-sm break-all">
                  {base64Tokens()}
                </span>
              </AccordionContent>
            </AccordionItem>
          </Accordion>
        </Show>

        <DialogFooter>
          <div class="flex w-full justify-between gap-4">
            <Show when={user()}>
              <TotpToggleButton client={client} user={user()!} />
            </Show>

            <Button
              type="button"
              variant="outline"
              onClick={() => client.logout()}
            >
              Logout
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
