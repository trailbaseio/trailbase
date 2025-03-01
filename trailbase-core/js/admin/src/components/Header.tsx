import { type JSX, Show } from "solid-js";
import { Separator } from "@/components/ui/separator";

export function Header(props: {
  title: string;
  titleSelect?: string;
  left?: JSX.Element;
  right?: JSX.Element;
}) {
  return (
    <div>
      <header class="mx-4 my-3 flex flex-wrap items-center">
        {/* left */}
        <div class="flex h-[40px] flex-nowrap items-center gap-2">
          <h1 class="m-0 text-accent-600">
            <Show when={props.titleSelect} fallback={props.title}>
              {`${props.title} ‣ `}

              <span class="text-black">{props.titleSelect}</span>
            </Show>
          </h1>

          {props.left}
        </div>

        {/* right */}
        {props.right && (
          <div class="flex max-h-[40px] grow justify-end">{props.right}</div>
        )}
      </header>

      <Separator />
    </div>
  );
}
