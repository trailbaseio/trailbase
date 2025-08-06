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

export function DestructiveActionButton(props: {
  children: JSX.Element;
  action: () => Promise<void>;
  msg?: string;
}) {
  const [open, setOpen] = createSignal<boolean>(false);
  const resolved = children(() => props.children);

  return (
    <Dialog open={open()} onOpenChange={setOpen}>
      <DialogContent>
        <DialogTitle>Confirmation</DialogTitle>
        {props.msg ?? "Are you sure?"}
        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Back
          </Button>

          <Button
            variant="destructive"
            onClick={() => {
              props
                .action()
                .then(() => setOpen(false))
                .catch(console.error);
            }}
            {...props}
          >
            Go ahead
          </Button>
        </DialogFooter>
      </DialogContent>

      <DialogTrigger
        as={(props: DialogTriggerProps) => (
          <Button variant="destructive" {...props}>
            {resolved()}
          </Button>
        )}
      />
    </Dialog>
  );
}
