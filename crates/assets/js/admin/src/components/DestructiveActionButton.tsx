import type { JSX } from "solid-js";
import { children, createSignal } from "solid-js";

import { Button } from "@/components/ui/button";
import type { DialogTriggerProps } from "@kobalte/core/dialog";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { showToast } from "@/components/ui/toast";

export function DestructiveActionButton(props: {
  children: JSX.Element;
  action: () => Promise<void>;
  msg?: string;
  size?: "default" | "lg" | "sm" | "icon";
}) {
  const [open, setOpen] = createSignal<boolean>(false);
  const resolved = children(() => props.children);

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <DialogContent>
        <DialogTitle>Confirmation</DialogTitle>

        {props.msg ?? "Are you sure?"}

        <DialogFooter class="gap-2">
          <Button variant="outline" onClick={() => setOpen(false)}>
            Back
          </Button>

          <Button
            variant="destructive"
            onClick={() => {
              // Start action.
              (async () => {
                try {
                  await props.action();
                } catch (err) {
                  showToast({
                    title: "Uncaught Error",
                    description: `${err}`,
                    variant: "error",
                  });
                }
              })();

              // And close dialog right away.
              setOpen(false);
            }}
            {...props}
          >
            Go ahead
          </Button>
        </DialogFooter>
      </DialogContent>

      <DialogTrigger
        as={(p: DialogTriggerProps) => (
          <Button size={props.size} variant="destructive" {...p}>
            {resolved()}
          </Button>
        )}
      />
    </Dialog>
  );
}
