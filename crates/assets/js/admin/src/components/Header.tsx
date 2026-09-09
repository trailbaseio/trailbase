import { useContext, JSX, Show } from "solid-js";
import { cn } from "@/lib/utils";

import { Separator } from "@/components/ui/separator";
import { SidebarContext, SidebarTrigger } from "@/components/ui/sidebar";

export function Header(props: {
  title: string;
  titleSelect?: JSX.Element;
  description?: JSX.Element;
  left?: JSX.Element;
  right?: JSX.Element;
  leading?: JSX.Element;
  wrap?: boolean;
}) {
  const context = useContext(SidebarContext);
  const hasLeading = () => props.leading || context;

  return (
    <div class="flex-none">
      <header
        class={cn(
          hasLeading() ? "pr-4" : "px-4",
          "flex justify-between gap-2 py-3",
          (props.wrap ?? true) ? "flex-wrap" : "max-h-[64px]",
        )}
      >
        {/* Everything on the left side */}
        <div class="flex min-w-0 items-center gap-2">
          <Show when={props.leading}>
            <div class="hover:bg-accent hover:text-accent-foreground flex h-10 w-9 items-center justify-center rounded-r-lg">
              {props.leading}
            </div>
          </Show>

          <Show when={!props.leading && context}>
            <div
              class="hover:bg-accent flex h-10 w-9 items-center justify-center rounded-r-lg"
              onClick={() => {
                context?.toggleSidebar();
              }}
            >
              <SidebarTrigger />
            </div>
          </Show>

          {/* Title + description */}
          <div class="min-w-0">
            <h1 class="m-0 flex gap-2">
              <span class="text-primary grow text-nowrap">{props.title}</span>

              <Show when={props.titleSelect}>
                <span class="line-clamp-1 shrink font-normal text-nowrap text-ellipsis">
                  ‣ {props.titleSelect}
                </span>
              </Show>
            </h1>

            <Show when={props.description}>
              <p class="text-muted-foreground m-0 text-xs">
                {props.description}
              </p>
            </Show>
          </div>

          <Show when={props.left !== undefined}>
            <div>{props.left}</div>
          </Show>
        </div>

        {/* Everything on the right */}
        <Show when={props.right !== undefined}>
          <div class="flex max-h-[40px] grow justify-end">{props.right}</div>
        </Show>
      </header>

      <Separator />
    </div>
  );
}
