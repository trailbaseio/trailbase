/* eslint-disable @typescript-eslint/no-explicit-any */
import { createSignal, Match, Switch, Show } from "solid-js";
import type { JSX } from "solid-js";
import { type FieldApi, createForm } from "@tanstack/solid-form";
import { TbEye } from "solid-icons/tb";

import { cn, tryParseInt, tryParseFloat, tryParseBigInt } from "@/lib/utils";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
  TextFieldTextArea,
  type TextFieldType,
} from "@/components/ui/text-field";

export { type AnyFieldApi } from "@tanstack/solid-form";

// A typed form field where FieldT = TFormData[Key].
// prettier-ignore
export type FieldApiT<FieldT> = FieldApi<
  /*TFormData=*/any, /*Key=*/any, FieldT, any, any, any, any, any, any, any, any, any, any, any, any, any, any, any, any, any, any, any, any>;

// eslint-disable-next-line @typescript-eslint/no-unused-vars
function formApiTHelper<TFormData>() {
  return createForm(() => ({ defaultValues: {} as TFormData }));
}

export type FormApiT<TFormData> = ReturnType<typeof formApiTHelper<TFormData>>;

export type FormStateT<TFormData> = FormApiT<TFormData>["state"];

type TextFieldOptions = {
  disabled?: boolean;
  type?: TextFieldType;

  label: () => JSX.Element;
  info?: JSX.Element;
  autocomplete?: string;

  // Optional placeholder string for absent values, e.g. "NULL". Optional only option.
  placeholder?: string;
};

export function buildTextFormField(opts: TextFieldOptions) {
  const externDisable = opts.disabled ?? false;

  return function builder(field: () => FieldApiT<string>) {
    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            disabled={externDisable}
            type={opts.type ?? "text"}
            value={field().state.value ?? ""}
            placeholder={opts.placeholder}
            onBlur={field().handleBlur}
            autocomplete={opts.autocomplete}
            autocorrect={opts.type === "password" ? "off" : undefined}
            onInput={(e: Event) => {
              const value: string = (e.target as HTMLInputElement).value;
              field().handleChange(value as string);
            }}
            data-testid="input"
          />

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  };
}

/// Used for proto Settings. Empty field is the same as absent.
export function buildOptionalTextFormField(opts: TextFieldOptions) {
  return function builder(field: () => FieldApiT<string | undefined>) {
    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            disabled={opts.disabled ?? false}
            type={opts.type ?? "text"}
            value={field().state.value ?? ""}
            placeholder={opts.placeholder}
            onBlur={field().handleBlur}
            autocomplete={opts.autocomplete}
            autocorrect={opts.type === "password" ? "off" : undefined}
            onInput={(e: Event) => {
              const value = (e.target as HTMLInputElement).value;
              field().handleChange(value || undefined);
            }}
            data-testid="input"
          />

          <GridFieldInfo field={field()} />
          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  };
}

export function buildSecretFormField(
  opts: Omit<TextFieldOptions, "type" | "autocomplete">,
) {
  const [type, setType] = createSignal<TextFieldType>("password");

  return function builder(field: () => FieldApiT<string>) {
    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <div class="flex items-center gap-2">
            <TextFieldInput
              disabled={opts.disabled ?? false}
              type={type()}
              value={field().state.value}
              onBlur={field().handleBlur}
              autocomplete={"off"}
              autocorrect="off"
              onInput={(e: Event) => {
                field().handleChange((e.target as HTMLInputElement).value);
              }}
            />

            <Button
              disabled={opts.disabled}
              variant={type() === "text" ? "default" : "outline"}
              onClick={() => {
                setType(type() === "password" ? "text" : "password");
              }}
            >
              <TbEye size={18} />
            </Button>
          </div>

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  };
}

export function buildOptionalTextAreaFormField(
  opts: Omit<TextFieldOptions, "type">,
  // Height in number of lines of the text area.
  rows?: number,
) {
  return function builder(field: () => FieldApiT<string | undefined>) {
    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldTextArea
            rows={rows}
            placeholder={opts.placeholder}
            disabled={opts?.disabled ?? false}
            value={field().state.value}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const value = (e.target as HTMLInputElement).value;
              field().handleChange(value || undefined);
            }}
          />

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  };
}

type NumberFieldOptions = {
  disabled?: boolean;
  label: () => JSX.Element;

  info?: JSX.Element;
  placeholder?: string;
};

/// Used for proto Settings. Empty field is the same as absent.
///
/// Prefer `buildOptionalIntegerFormField` and `buildOptionalFloatFormField`. We
export function buildOptionalNumberFormField(
  opts: NumberFieldOptions & { integer?: boolean },
) {
  return function builder(field: () => FieldApiT<number | undefined>) {
    const isInt = opts.integer ?? false;

    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            disabled={opts.disabled ?? false}
            type={isInt ? "number" : "text"}
            step={isInt ? "1" : undefined}
            pattern={isInt ? intPattern : floatPattern}
            value={field().state.value?.toString() ?? ""}
            placeholder={opts.placeholder}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const value = (e.target as HTMLInputElement).value;
              const parsed = isInt ? tryParseInt(value) : tryParseFloat(value);
              field().handleChange(parsed);
            }}
            data-testid="input"
          />

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  };
}

/// Used for proto Settings. Empty field is the same as absent.
export function buildOptionalIntegerFormField(opts: NumberFieldOptions) {
  return function builder(field: () => FieldApiT<bigint | undefined>) {
    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            disabled={opts.disabled ?? false}
            type={"number"}
            step={"1"}
            value={field().state.value?.toString() ?? ""}
            placeholder={opts.placeholder}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const value = (e.target as HTMLInputElement).value;
              field().handleChange(tryParseBigInt(value));
            }}
            data-testid="input"
          />

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  };
}

/// Used for proto Settings. Empty field is the same as absent.
export function buildOptionalFloatFormField(opts: NumberFieldOptions) {
  return function builder(field: () => FieldApiT<number | undefined>) {
    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            disabled={opts.disabled ?? false}
            type={"text"}
            pattern={floatPattern}
            value={field().state.value?.toString() ?? ""}
            placeholder={opts.placeholder}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const value = (e.target as HTMLInputElement).value;
              field().handleChange(tryParseFloat(value));
            }}
            data-testid="input"
          />

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  };
}

export function buildBoolFormField(props: { label: () => JSX.Element }) {
  return function builder(field: () => FieldApiT<boolean>) {
    return (
      <div class="flex w-full justify-end gap-4">
        <Label class="text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
          {props.label()}
        </Label>

        <Checkbox
          checked={field().state.value}
          onBlur={field().handleBlur}
          onChange={field().handleChange}
        />
      </div>
    );
  };
}

export function buildOptionalBoolFormField(opts: {
  label: () => JSX.Element;
  info?: JSX.Element;
}) {
  return function builder(field: () => FieldApiT<boolean | undefined>) {
    return (
      <div
        class={`grid items-center ${gapStyle}`}
        style={{ "grid-template-columns": "auto 1fr" }}
      >
        <Label class="text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
          {opts.label()}
        </Label>

        <Checkbox
          checked={field().state.value}
          onBlur={field().handleBlur}
          onChange={field().handleChange}
        />

        <InfoColumn info={opts.info} />
      </div>
    );
  };
}

interface SelectFieldOpts {
  label: () => JSX.Element;
  disabled?: boolean;
}

export function buildSelectField(options: string[], opts: SelectFieldOpts) {
  return function builder(field: () => FieldApiT<string>) {
    return (
      <SelectField
        label={opts.label}
        disabled={opts.disabled}
        options={options}
        value={field().state.value}
        onChange={(v: string | null) => {
          if (v) {
            field().handleChange(v);
          }
        }}
        handleBlur={field().handleBlur}
      />
    );
  };
}

export function SelectField(
  props: {
    options: string[];
    value: string;
    onChange: (v: string | null) => void;
    handleBlur: () => void;
  } & SelectFieldOpts,
) {
  return (
    <div
      class={cn("grid w-full items-center", gapStyle)}
      style={{ "grid-template-columns": "auto 1fr" }}
    >
      <Label>{props.label()}</Label>

      <Select
        required={true}
        multiple={false}
        value={props.value}
        onBlur={props.handleBlur}
        onChange={props.onChange}
        options={props.options}
        itemComponent={(props) => (
          <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
        )}
        disabled={props?.disabled}
      >
        <SelectTrigger>
          <SelectValue<string>>{(state) => state.selectedOption()}</SelectValue>
        </SelectTrigger>

        <SelectContent />
      </Select>
    </div>
  );
}

export function FieldInfo<T>(props: { field: FieldApiT<T> }) {
  const meta = () => props.field.state.meta;
  return (
    <Switch>
      <Match when={meta().errors.length > 0}>
        <em class="text-sm text-red-700">{meta().errors}</em>
      </Match>

      <Match when={meta().isValidating}>Validating...</Match>
    </Switch>
  );
}

export function GridFieldInfo<T>(props: { field: FieldApiT<T> }) {
  const show = () => {
    const meta = props.field.state.meta;
    return meta.errors.length > 0 || meta.isValidating;
  };

  return (
    <Show when={show()}>
      <div class="text-muted-foreground col-start-2 ml-2 text-sm">
        <FieldInfo {...props} />
      </div>
    </Show>
  );
}

function InfoColumn(props: { info: JSX.Element | undefined }) {
  return (
    <Show when={props.info}>
      <div class="col-start-2 text-sm">{props.info}</div>
    </Show>
  );
}

export function notEmptyValidator() {
  return {
    onChange: ({ value }: { value: string | undefined }) => {
      if (!value) {
        if (import.meta.env.DEV) {
          return `Must not be empty. Undefined: ${value === undefined}`;
        }
        return "Must not be empty";
      }
    },
  };
}

export function unsetOrNotEmptyValidator() {
  return {
    onChange: ({ value }: { value: string | undefined }) => {
      if (value === undefined) return undefined;

      if (!value) {
        return "Must not be empty";
      }
    },
  };
}

export function unsetOrValidUrl() {
  return {
    onChange: ({ value }: { value: string | undefined }) => {
      if (value === undefined) return undefined;

      try {
        new URL(value);
      } catch (e) {
        if (e instanceof TypeError) {
          return `${e.message}: '${value}'`;
        }
        return `${e}: '${value}'`;
      }
    },
  };
}

export function largerThanZero() {
  return {
    onChange: ({ value }: { value: number | undefined }) => {
      if (!value || value <= 0) {
        return "Must be positive";
      }
    },
  };
}

export function unsetOrLargerThanZero() {
  return {
    onChange: ({ value }: { value: number | undefined }) => {
      if (value === undefined) return;

      if (value <= 0) {
        return "Must be positive";
      }
    },
  };
}

export const gapStyle = "gap-x-2 gap-y-1";
export const floatPattern = "[-+]?[0-9]*[.]?[0-9]+";
export const intPattern = "[-+]?[0-9]+";
export const uintPattern = "[+]?[0-9]+";
