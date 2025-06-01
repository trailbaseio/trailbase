import { JSX } from "solid-js";

import { Button } from "@/components/ui/button";
import { TextField, TextFieldInput } from "@/components/ui/text-field";

export function FilterBar(props: {
  initial?: string;
  onSubmit: (filter: string) => void;
  example?: JSX.Element;
}) {
  let ref: HTMLInputElement | undefined;
  const onSubmit = (ev: SubmitEvent) => {
    ev.preventDefault();

    const value = ref?.value;
    console.debug("set filter: ", value);
    if (value !== undefined) {
      props.onSubmit(value);
    }
  };

  return (
    <div class="flex w-full flex-col">
      <form
        class="flex w-full items-center gap-2"
        method="dialog"
        onSubmit={onSubmit}
      >
        <TextField class="w-full">
          <TextFieldInput
            ref={ref}
            value={props.initial}
            type="text"
            placeholder="filter"
          />
        </TextField>

        <Button type="button">Filter</Button>
      </form>

      {props.example && <span class="ml-2 mt-1 text-sm">{props.example}</span>}
    </div>
  );
}
