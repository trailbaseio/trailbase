{
  /* eslint-disable @typescript-eslint/no-explicit-any */
}
import { createSignal, Show, type JSX } from "solid-js";
import {
  type FieldApi,
  type FormState,
  type FormApi,
  type SolidFormApi,
} from "@tanstack/solid-form";
import { TbEye } from "solid-icons/tb";

import { cn } from "@/lib/utils";

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
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import type { ColumnDataType } from "@bindings/ColumnDataType";

export { type AnyFieldApi } from "@tanstack/solid-form";

// prettier-ignore
export type FieldApiT<T> = FieldApi<any, any, T, any, any, any, any, any, any, any, any, any, any, any, any, any, any, any, any>;

// prettier-ignore
export type FormStateT<T> = FormState<T, any, any, any, any, any, any, any, any>;

// prettier-ignore
export type FormApiT<T> = FormApi<T, any, any, any, any, any, any, any, any, any> &
  SolidFormApi<any, any, any, any, any, any, any, any, any, any>;

type TextFieldOptions = {
  disabled?: boolean;
  type?: TextFieldType;

  label: () => JSX.Element;
  info?: JSX.Element;
  autocomplete?: string;

  // Optional placeholder string for absent values, e.g. "NULL". Optional only option.
  placeholder?: string;
};

/// Note that we make not-required/optional explicit by having a checkbox, since there's
/// a difference between empty string and not set.
function buildTextFormFieldT<T extends string | null>(opts: TextFieldOptions) {
  const externDisable = opts.disabled ?? false;

  function builder(field: () => FieldApiT<T>) {
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
            onKeyUp={(e) => {
              const value: string = (e.target as HTMLInputElement).value;
              field().handleChange(value as T);
            }}
            data-testid="input"
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

export function buildTextFormField(opts: TextFieldOptions) {
  return buildTextFormFieldT<string>(opts);
}

function buildOptionalNullableTextFormField<
  T extends string | null | undefined,
>(opts: TextFieldOptions, unsetValue: T, handler: (e: Event) => T) {
  const externDisable = opts.disabled ?? false;

  function builder(field: () => FieldApiT<T>) {
    const initialValue = field().state.value;
    const [enabled, setEnabled] = createSignal<boolean>(
      !externDisable && initialValue !== undefined && initialValue !== null,
    );

    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
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
              onKeyUp={(e) => field().handleChange(handler(e))}
              data-testid="input"
            />

            <Checkbox
              disabled={externDisable}
              checked={enabled()}
              onChange={(enabled: boolean) => {
                setEnabled(enabled);
                // NOTE: null is critical here to actively unset a cell, undefined
                // would merely take it out of the patch set.
                const value = enabled
                  ? ((initialValue ?? "") as T)
                  : unsetValue;
                field().handleChange(value);
              }}
              data-testid="toggle"
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
  const handler = (e: Event) => (e.target as HTMLInputElement).value;
  return buildOptionalNullableTextFormField<string | undefined>(
    opts,
    undefined,
    handler,
  );
}

export function buildNullableTextFormField(opts: TextFieldOptions) {
  const handler = (e: Event) => (e.target as HTMLInputElement).value;
  return buildOptionalNullableTextFormField<string | null>(opts, null, handler);
}

export function buildSecretFormField(
  opts: Omit<TextFieldOptions, "type" | "autocomplete">,
) {
  const [type, setType] = createSignal<TextFieldType>("password");

  return (field: () => FieldApiT<string>) => (
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
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
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
  required?: boolean;
};

// NOTE: Optional/nullable numbers don't need a dedicated toggle switch, since
// the empty string can already be interpreted as an unset value, as opposed to
// strings.
function buildOptionalNullableNumberFormField<
  T extends string | number | null | undefined,
>(
  opts: NumberFieldOptions,
  defaultValue: number | string | undefined,
  handler: (e: Event) => T,
) {
  const isInt = opts.integer ?? false;

  return (field: () => FieldApiT<T>) => {
    return (
      <TextField class="w-full">
        <div
          class={`grid items-center ${gapStyle}`}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            type={isInt ? "number" : "text"}
            required={opts.required}
            step={isInt ? "1" : undefined}
            pattern={isInt ? "d+" : "[0-9]*[.,]?[0-9]*"}
            value={field().state.value ?? defaultValue}
            disabled={opts?.disabled}
            onBlur={field().handleBlur}
            onKeyUp={(e) => field().handleChange(handler(e))}
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

export function buildOptionalNumberFormField(opts: NumberFieldOptions) {
  function tryParseInt(e: Event): number | undefined {
    const n = parseInt((e.target as HTMLInputElement).value);
    return isNaN(n) ? undefined : n;
  }

  function tryParseFloat(e: Event): number | undefined {
    const n = parseFloat((e.target as HTMLInputElement).value);
    return isNaN(n) ? undefined : n;
  }

  return buildOptionalNullableNumberFormField<number | undefined>(
    opts,
    undefined,
    opts.integer ? tryParseInt : tryParseFloat,
  );
}

function buildNullableNumberFormField(opts: NumberFieldOptions) {
  function tryParseInt(e: Event): string | null {
    const n = parseInt((e.target as HTMLInputElement).value);
    return isNaN(n) ? null : n.toString();
  }

  function tryParseFloat(e: Event): string | null {
    const n = parseFloat((e.target as HTMLInputElement).value);
    return isNaN(n) ? null : n.toString();
  }

  return buildOptionalNullableNumberFormField<string | null>(
    opts,
    undefined,
    opts.integer ? tryParseInt : tryParseFloat,
  );
}

export function buildBoolFormField(props: { label: () => JSX.Element }) {
  return (field: () => FieldApiT<boolean>) => (
    <div class="flex w-full justify-end gap-4">
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
        class={cn("grid w-full items-center", gapStyle)}
        style={{ "grid-template-columns": "auto 1fr" }}
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

function isInt(type: ColumnDataType): boolean {
  switch (type) {
    case "Integer":
    case "Int":
    case "Int2":
    case "Int4":
    case "Int8":
    case "TinyInt":
    case "SmallInt":
    case "MediumInt":
    case "BigInt":
    case "UnignedBigInt":
      return true;
    default:
      return false;
  }
}

function isReal(type: ColumnDataType): boolean {
  switch (type) {
    case "Real":
    case "Float":
    case "Double":
    case "DoublePrecision":
    case "Decimal":
    case "Numeric":
      return true;
    default:
      return false;
  }
}

export function isNumber(type: ColumnDataType): boolean {
  return isInt(type) || isReal(type);
}

// NOTE: this is a not a component but a builder:
//   "(field: () => FieldApiT<T>) => Component"
//
// The unused extra arg only exists to make this clear to eslint.
export function buildDBCellField(
  props: {
    name: string;
    type: ColumnDataType;
    notNull: boolean;
    disabled: boolean;
    placeholder: string;
  },
  _unused?: unknown,
) {
  const typeLabel = () => `[${props.type}${props.notNull ? "" : "?"}]`;

  const label = () => (
    <div class="flex w-[100px] flex-wrap items-center gap-1 overflow-hidden">
      <span>{props.name} </span>

      <Show when={props.type === "Blob"} fallback={typeLabel()}>
        <Tooltip>
          <TooltipTrigger as="div">
            <span class="text-primary">{typeLabel()}</span>
          </TooltipTrigger>

          <TooltipContent>
            Binary blobs can be entered encoded as url-safe Base64.
          </TooltipContent>
        </Tooltip>
      </Show>
    </div>
  );

  const type = props.type;
  const optional = !props.notNull;
  const placeholder = props.placeholder;
  const disabled = props.disabled;

  if (type === "Text" || type === "Blob") {
    if (optional) {
      return buildNullableTextFormField({ label, placeholder, disabled });
    }
    return buildTextFormFieldT<string | null>({
      label,
      placeholder,
      disabled,
    });
  }

  if (isInt(type)) {
    return buildNullableNumberFormField({
      label,
      disabled,
      integer: true,
      required: !optional,
    });
  }

  if (isReal(type)) {
    return buildNullableNumberFormField({
      label,
      disabled,
      integer: false,
      required: !optional,
    });
  }

  console.debug(
    `Custom FormFields not implemented for '${type}'. Falling back to text field`,
  );

  if (optional) {
    return buildNullableTextFormField({ label, placeholder, disabled });
  }
  return buildTextFormFieldT<string | null>({ label, placeholder, disabled });
}

export const gapStyle = "gap-x-2 gap-y-1";
