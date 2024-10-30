import { createSignal } from "solid-js";

import { Button } from "@/components/ui/button";
import { TextField, TextFieldInput } from "@/components/ui/text-field";

export function FilterBar(props: {
  example?: string;
  placeholder?: string;
  initial?: string;
  onSubmit: (filter: string) => void;
}) {
  const [input, setInput] = createSignal(props.initial ?? "");

  const onSubmit = () => {
    const value = input();
    console.debug("set filter: ", value);
    props.onSubmit(value);
  };

  return (
    <div class="w-full flex flex-col">
      <form
        class="flex w-full items-center gap-2"
        onSubmit={onSubmit}
        action="javascript:void(0);"
      >
        <TextField class="w-full">
          <TextFieldInput
            type="text"
            placeholder={props.placeholder ?? "filter"}
            onKeyUp={(e: KeyboardEvent) => {
              const value = (e.currentTarget as HTMLInputElement).value;
              setInput(value);
            }}
          />
        </TextField>

        <Button type="button">Filter</Button>
      </form>

      {props.example && <span class="text-sm mt-1 ml-2">{props.example}</span>}
    </div>
  );
}
