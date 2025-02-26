import { createSignal, type JSX } from "solid-js";
import { createForm, type FieldApi } from "@tanstack/solid-form";
import { TbInfoCircle, TbEye } from "solid-icons/tb";

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
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card";

import type { ColumnDataType } from "@/lib/bindings";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyFieldApi = FieldApi<any, any, any, any>;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type FieldApiT<T> = FieldApi<any, any, any, any, T>;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type FormType<T> = ReturnType<typeof createForm<T, any>>;

type TextFieldOptions = {
  disabled?: boolean;
  type?: TextFieldType;

  label: () => JSX.Element;
  info?: JSX.Element;
  autocomplete?: string;

  // Optional placeholder string for absent values, e.g. "NULL". Optional only option.
  placeholder?: string;
};

export function buildTextFormFieldT<T extends string | undefined>(
  opts: TextFieldOptions,
) {
  const externDisable = opts.disabled ?? false;

  function builder(field: () => FieldApiT<T>) {
    return (
      <TextField class="w-full">
        <div
          class={`grid items-center ${gapStyle}`}
          style="grid-template-columns: auto 1fr"
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
            onKeyUp={(e) =>
              field().handleChange((e.target as HTMLInputElement).value as T)
            }
          />

          <div class="col-start-2 ml-2 text-sm text-muted-foreground">
            {field && <FieldInfo field={field()} />}
          </div>

          <div class="col-start-2 text-sm">{opts.info}</div>
        </div>
      </TextField>
    );
  }

  return builder;
}

/// Note that we make not-required/optional explicit by having a checkbox, since there's
/// a difference between empty string and not set.
export function buildTextFormField(opts: TextFieldOptions) {
  return buildTextFormFieldT<string>(opts);
}

export function buildOptionalTextFormFieldT<T extends string | undefined>(
  opts: TextFieldOptions,
) {
  const externDisable = opts.disabled ?? false;

  function builder(field: () => FieldApiT<T>) {
    const initialValue = field().state.value;
    const [enabled, setEnabled] = createSignal<boolean>(
      !externDisable && initialValue !== undefined && initialValue !== null,
    );

    return (
      <TextField class="w-full">
        <div
          class={`grid items-center ${gapStyle}`}
          style="grid-template-columns: auto 1fr"
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <div class="flex items-center">
            <TextFieldInput
              disabled={!enabled()}
              type={opts.type ?? "text"}
              value={field().state.value?.toString() ?? ""}
              placeholder={enabled() ? "" : "NULL"}
              onBlur={field().handleBlur}
              autocomplete={opts.autocomplete}
              autocorrect={opts.type === "password" ? "off" : undefined}
              onKeyUp={(e) =>
                field().handleChange((e.target as HTMLInputElement).value as T)
              }
            />

            <Checkbox
              disabled={externDisable}
              checked={enabled()}
              onChange={(enabled: boolean) => {
                setEnabled(enabled);
                // NOTE: null is critical here to actively unset a cell, undefined
                // would merely take it out of the patch set.
                const value = enabled ? (initialValue ?? "") : null;
                field().handleChange(value as T);
              }}
            />
          </div>

          <div class="col-start-2 ml-2 text-sm text-muted-foreground">
            {field && <FieldInfo field={field()} />}
          </div>

          <div class="col-start-2 text-sm">{opts.info}</div>
        </div>
      </TextField>
    );
  }

  return builder;
}

export function buildOptionalTextFormField(opts: TextFieldOptions) {
  return buildOptionalTextFormFieldT<string | undefined>(opts);
}

export function buildSecretFormField(opts: Omit<TextFieldOptions, "type">) {
  const [type, setType] = createSignal<TextFieldType>("password");

  return (field: () => FieldApiT<string>) => (
    <TextField class="w-full">
      <div
        class={`grid items-center ${gapStyle}`}
        style="grid-template-columns: auto 1fr"
      >
        <TextFieldLabel>{opts.label()}</TextFieldLabel>

        <div class="flex gap-2 items-center">
          <TextFieldInput
            disabled={opts.disabled ?? false}
            type={type()}
            value={field().state.value}
            onBlur={field().handleBlur}
            autocomplete={opts.autocomplete ?? "off"}
            autocorrect="off"
            onKeyUp={(e: Event) => {
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

        <div class="col-start-2 ml-2 text-sm text-muted-foreground">
          {field && <FieldInfo field={field()} />}
        </div>

        <div class="col-start-2 text-sm">{opts.info}</div>
      </div>
    </TextField>
  );
}

export function buildTextAreaFormField(
  opts: Omit<TextFieldOptions, "type">,
  rows?: number,
) {
  return (field: () => FieldApiT<string | undefined>) => {
    return (
      <TextField class="w-full">
        <div
          class={`grid items-center ${gapStyle}`}
          style="grid-template-columns: auto 1fr"
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldTextArea
            rows={rows}
            disabled={opts?.disabled ?? false}
            value={field().state.value}
            onBlur={field().handleBlur}
            onKeyUp={(e: Event) => {
              field().handleChange((e.target as HTMLInputElement).value);
            }}
          />

          <div class="col-start-2 ml-2 text-sm text-muted-foreground">
            {field && <FieldInfo field={field()} />}
          </div>

          <div class="col-start-2 text-sm">{opts.info}</div>
        </div>
      </TextField>
    );
  };
}

type NumberFieldOptions = {
  disabled?: boolean;
  label: () => JSX.Element;

  info?: JSX.Element;
  integer?: boolean;
};

export function buildNumberFormFieldT<T extends number | string | undefined>(
  opts: NumberFieldOptions,
) {
  const isInt = opts.integer ?? false;

  return (field: () => FieldApiT<T>) => {
    return (
      <TextField class="w-full">
        <div
          class={`grid items-center ${gapStyle}`}
          style="grid-template-columns: auto 1fr"
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            type="number"
            step={isInt ? "1" : undefined}
            pattern={isInt ? "d+" : undefined}
            value={field().state.value ?? undefined}
            disabled={opts?.disabled}
            onBlur={field().handleBlur}
            onKeyUp={(e: Event) => {
              const v = (e.target as HTMLInputElement).value;
              if (v) {
                const i = parseInt(v);
                field().handleChange(i as T);
              }
            }}
          />

          <div class="col-start-2 ml-2 text-sm text-muted-foreground">
            {field && <FieldInfo field={field()} />}
          </div>

          <div class="col-start-2 text-sm">{opts?.info}</div>
        </div>
      </TextField>
    );
  };
}

export function buildNumberFormField(opts: NumberFieldOptions) {
  return buildNumberFormFieldT<number>(opts);
}

export function buildBoolFormField(props: { label: () => JSX.Element }) {
  return (field: () => FieldApiT<boolean>) => (
    <div class="w-full flex gap-4 justify-end">
      <Label class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
        {props.label()}
      </Label>

      <Checkbox
        checked={field().state.value}
        onBlur={field().handleBlur}
        onChange={field().handleChange}
      />
    </div>
  );
}

interface SelectFieldOpts {
  label: () => JSX.Element;
  disabled?: boolean;
}

export function buildSelectField(options: string[], opts: SelectFieldOpts) {
  return (field: () => FieldApiT<string>) => {
    return (
      <div
        class={`w-full grid items-center ${gapStyle}`}
        style="grid-template-columns: auto 1fr"
      >
        <Label>{opts.label()}</Label>

        <Select
          required={true}
          multiple={false}
          value={field().state.value}
          onBlur={field().handleBlur}
          onChange={(v: string | null) => field().handleChange(v!)}
          options={options}
          itemComponent={(props) => (
            <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
          )}
          disabled={opts?.disabled}
        >
          <SelectTrigger>
            <SelectValue<string>>
              {(state) => state.selectedOption()}
            </SelectValue>
          </SelectTrigger>

          <SelectContent />
        </Select>
      </div>
    );
  };
}

function FieldInfo<T>(props: { field: FieldApiT<T> }) {
  return (
    <>
      {props.field.state.meta.errors ? (
        <em class="text-sm text-red-700">{props.field.state.meta.errors}</em>
      ) : null}
      {props.field.state.meta.isValidating ? "Validating..." : null}
    </>
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
        if (import.meta.env.DEV) {
          return `Must not be empty. Undefined: ${value === undefined}`;
        }
        return "Must not be empty";
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

function BinaryBlobHoverCard() {
  return (
    <HoverCard>
      <HoverCardTrigger
        class="size-[32px]"
        as={Button<"button">}
        variant="link"
      >
        <TbInfoCircle />
      </HoverCardTrigger>

      <HoverCardContent>
        Binary blobs can be entered encoded as url-safe Base64.
      </HoverCardContent>
    </HoverCard>
  );
}

export function buildDBCellField(props: {
  type: ColumnDataType;
  label: string;
  optional: boolean;
  placeholder?: string;
}) {
  const label = () => (
    <div class="w-[100px] flex gap-2 items-center overflow-hidden">
      {props.type === "Blob" && <BinaryBlobHoverCard />}
      {props.label}
    </div>
  );

  const type = props.type;
  const optional = props.optional;
  const placeholder = props.placeholder;
  if (type === "Text" || type === "Blob") {
    if (optional) {
      return buildOptionalTextFormField({ label, placeholder });
    }
    return buildTextFormFieldT<string | undefined>({ label, placeholder });
  }

  if (type === "Integer" && !optional) {
    return buildNumberFormFieldT<string | undefined>({ label });
  }

  console.debug(
    `Custom FormFields not implemented for '${type}'. Falling back to text field`,
  );

  if (optional) {
    return buildOptionalTextFormField({ label, placeholder });
  }
  return buildTextFormFieldT<string | undefined>({ label, placeholder });
}

export const gapStyle = "gap-x-2 gap-y-1";
