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
          <h2 class="text-accent-600 m-0">
            <Show when={props.titleSelect} fallback={props.title}>
              {props.title}
              <span class="text-border mx-2 text-xs">‣</span>
              <span class="font-normal text-black">{props.titleSelect}</span>
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
