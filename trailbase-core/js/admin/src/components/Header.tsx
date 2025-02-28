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
      <header class="flex flex-wrap items-center my-3 mx-4">
        {/* left */}
        <div class="no-flex h-[40px] flex items-center gap-2">
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
          <div class="max-h-[40px] grow flex justify-end">{props.right}</div>
        )}
      </header>

      <Separator />
    </div>
  );
}
