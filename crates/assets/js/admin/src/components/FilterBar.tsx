import {
  Show,
  createEffect,
  createSignal,
  onCleanup,
  onMount,
  type JSX,
} from "solid-js";
import { TbOutlineSearch, TbOutlineX } from "solid-icons/tb";

import { Button } from "@/components/ui/button";
import { TextField, TextFieldInput } from "@/components/ui/text-field";

export function FilterBar(props: {
  initial?: string;
  onSubmit: (filter: string) => void;
  example?: JSX.Element;
  placeholder?: string;
}) {
  let input: HTMLInputElement | undefined;
  const [value, setValue] = createSignal(props.initial ?? "");

  createEffect(() => setValue(props.initial ?? ""));

  onMount(() => {
    const focusFilter = (event: KeyboardEvent) => {
      const target = event.target;
      if (
        event.key !== "/" ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return;
      }

      event.preventDefault();
      input?.focus();
    };

    document.addEventListener("keydown", focusFilter);
    onCleanup(() => document.removeEventListener("keydown", focusFilter));
  });

  const onSubmit = (event: SubmitEvent) => {
    event.preventDefault();
    props.onSubmit(value());
  };

  const clear = () => {
    setValue("");
    props.onSubmit("");
    input?.focus();
  };

  return (
    <div class="flex w-full min-w-0 flex-col">
      <form
        class="flex w-full min-w-0 items-center gap-1.5"
        method="dialog"
        onSubmit={onSubmit}
      >
        <TextField class="min-w-0 flex-1">
          <TextFieldInput
            ref={(element) => (input = element)}
            value={value()}
            onInput={(event) => setValue(event.currentTarget.value)}
            type="text"
            aria-label="Filter rows"
            placeholder={props.placeholder ?? "Filter rows…"}
          />
        </TextField>

        <Show when={value()}>
          <Button
            size="icon"
            variant="ghost"
            type="button"
            aria-label="Clear filter"
            onClick={clear}
          >
            <TbOutlineX />
          </Button>
        </Show>

        <Button variant="outline" type="submit" aria-label="Apply filter">
          <TbOutlineSearch />
          <span class="hidden sm:inline">Apply</span>
        </Button>
      </form>

      {props.example && (
        <span class="text-muted-foreground mt-1 ml-2 text-sm">
          {props.example}
        </span>
      )}
    </div>
  );
}
