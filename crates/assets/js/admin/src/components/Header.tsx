import { useContext, JSX, Show } from "solid-js";

import { Separator } from "@/components/ui/separator";
import { SidebarContext, SidebarTrigger } from "@/components/ui/sidebar";

export function Header(props: {
  title: string;
  titleSelect?: JSX.Element;
  left?: JSX.Element;
  right?: JSX.Element;
  leading?: JSX.Element;
  class?: string;
}) {
  const context = useContext(SidebarContext);
  const hasLeading = () => props.leading || context;

  return (
    <div class={props.class}>
      <header
        class={`${hasLeading() ? "mr-4" : "mx-4"} my-3 flex flex-wrap items-center gap-2`}
      >
        <Show when={props.leading}>
          <div class="hover:bg-accent hover:text-accent-foreground flex h-10 w-9 items-center justify-center rounded-r-lg">
            {props.leading}
          </div>
        </Show>

        <Show when={!props.leading && context}>
          <div
            class="hover:bg-accent hover:text-accent-foreground flex h-10 w-9 items-center justify-center rounded-r-lg"
            onClick={() => {
              context?.toggleSidebar();
            }}
          >
            <SidebarTrigger />
          </div>
        </Show>

        <div class="flex min-h-[40px] flex-nowrap items-center gap-2">
          <h1 class="m-0">
            <span class="text-accent-600">{props.title}</span>

            <Show when={props.titleSelect}>
              <span class="text-muted-foreground mx-2">‣</span>
              <span class="font-normal">{props.titleSelect}</span>
            </Show>
          </h1>

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
