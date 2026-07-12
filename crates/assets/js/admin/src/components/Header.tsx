import { type JSX, Show } from "solid-js";
import { Separator } from "@/components/ui/separator";

export function Header(props: {
  title: string;
  titleSelect?: JSX.Element;
  left?: JSX.Element;
  right?: JSX.Element;
  leading?: JSX.Element;
  class?: string;
}) {
  return (
    <div class={props.class}>
      <header
        class={`${props.leading ? "mr-4" : "mx-4"} my-3 flex flex-wrap items-center gap-2`}
      >
        <Show when={props.leading !== undefined}>
          <div class="bg-sidebar-accent flex h-10 w-9 items-center justify-center rounded-r-lg">
            {props.leading}
          </div>
        </Show>

        <div class="flex min-h-[40px] flex-nowrap items-center gap-2">
          <h2 class="m-0">
            <Show when={props.titleSelect} fallback={props.title}>
              <span class="text-accent-600">{props.title}</span>
              <span class="text-muted-foreground mx-2">‣</span>
              <span class="font-normal">{props.titleSelect}</span>
            </Show>
          </h2>

          {/* left */}
          <Show when={props.left !== undefined}>{props.left}</Show>
        </div>

        {/* right */}
        <Show when={props.right !== undefined}>
          <div class="flex max-h-[40px] grow justify-end">{props.right}</div>
        </Show>
      </header>

      <Separator />
    </div>
  );
}
