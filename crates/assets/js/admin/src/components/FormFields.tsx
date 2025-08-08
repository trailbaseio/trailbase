/* eslint-disable @typescript-eslint/no-explicit-any */
import { createSignal, Match, Switch, Show, type JSX } from "solid-js";
import { type FieldApi, createForm } from "@tanstack/solid-form";
import { TbEye } from "solid-icons/tb";
import { urlSafeBase64Decode } from "trailbase";

import { cn, tryParseInt, tryParseFloat } from "@/lib/utils";
import { isNullableColumn, isReal, isInt } from "@/lib/schema";
import { literalDefault } from "@/lib/convert";

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
            onInput={(e: Event) => {
              const value: string = (e.target as HTMLInputElement).value;
              field().handleChange(value as T);
            }}
            data-testid="input"
          />

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
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

    const value = () => {
      return enabled() ? field().state.value?.toString() : "";
    };
    const placeholder = () => {
      return enabled()
        ? (field().state.value ?? opts.placeholder ?? "")
        : "NULL";
    };

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
              value={value()}
              placeholder={placeholder()}
              onBlur={field().handleBlur}
              autocomplete={opts.autocomplete}
              autocorrect={opts.type === "password" ? "off" : undefined}
              onInput={(e: Event) => field().handleChange(handler(e))}
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

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
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

// Simple optional textarea that treats empty string as undefined
export function buildSimpleOptionalTextArea(
  opts: TextFieldOptions & { rows?: number }
) {
  function builder(field: () => FieldApiT<string | undefined>) {
    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <textarea
            disabled={opts.disabled ?? false}
            value={field().state.value ?? ""}
            placeholder={opts.placeholder}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const target = e.target as HTMLTextAreaElement;
              const value = target.value;
              field().handleChange(value || undefined);
            }}
            rows={opts.rows ?? 5}
            class="flex min-h-[80px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
          />

          <GridFieldInfo field={field()} />
          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  }
  return builder;
}

// Simple optional field that treats empty string as undefined (for SMTP settings)
export function buildSimpleOptionalTextField(opts: TextFieldOptions) {
  function builder(field: () => FieldApiT<string | undefined>) {
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
              field().handleChange(value === "" ? undefined : value);
            }}
            data-testid="input"
          />

          <GridFieldInfo field={field()} />
          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  }
  return builder;
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
            onInput={(e: Event) => {
              field().handleChange((e.target as HTMLInputElement).value);
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
  integer?: boolean;
  required?: boolean;
  placeholder?: string;
};

// NOTE: Optional/nullable numbers still need a dedicated toggle switch, since
// the empty string could either mean use the default or set NULL.
function buildOptionalNullableNumberFormField<
  T extends string | number | null | undefined,
>(opts: NumberFieldOptions, unsetValue: T, handler: (e: Event) => T) {
  const externDisable = opts.disabled ?? false;

  function builder(field: () => FieldApiT<T>) {
    const initialValue = field().state.value;
    const [enabled, setEnabled] = createSignal<boolean>(
      !externDisable && initialValue !== undefined && initialValue !== null,
    );
    const isInt = opts.integer ?? false;

    const value = () => {
      return enabled() ? field().state.value?.toString() : "";
    };
    const placeholder = () => {
      if (enabled()) {
        const value = field().state.value?.toString();
        if (value === undefined || value === "") {
          return opts.placeholder ?? "";
        }
        return value;
      }
      return "NULL";
    };

    return (
      <TextField class="w-full">
        <div
          class={`grid items-center ${gapStyle}`}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <div class="flex items-center">
            <TextFieldInput
              disabled={!enabled()}
              type={isInt && enabled() ? "number" : "text"}
              required={opts.required}
              step={isInt ? "1" : undefined}
              pattern={isInt ? "d+" : "[0-9]*[.,]?[0-9]*"}
              value={value()}
              placeholder={placeholder()}
              onBlur={field().handleBlur}
              onInput={(e: Event) => {
                // NOTE: OnKeyUp doesn't work here, since numbers can be scrolled etc.
                field().handleChange(handler(e));
              }}
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

          <GridFieldInfo field={field()} />

          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  }

  return builder;
}

// Simple optional number field that treats empty as undefined (for SMTP settings)
export function buildSimpleOptionalNumberField(opts: NumberFieldOptions) {
  function builder(field: () => FieldApiT<number | undefined>) {
    return (
      <TextField class="w-full">
        <div
          class={cn("grid items-center", gapStyle)}
          style={{ "grid-template-columns": "auto 1fr" }}
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            disabled={opts.disabled ?? false}
            type="number"
            value={field().state.value?.toString() ?? ""}
            placeholder={opts.placeholder}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const value = (e.target as HTMLInputElement).value;
              const parsed = opts.integer ? tryParseInt(value) : tryParseFloat(value);
              field().handleChange(parsed === undefined ? undefined : parsed);
            }}
            data-testid="input"
          />

          <GridFieldInfo field={field()} />
          <InfoColumn info={opts.info} />
        </div>
      </TextField>
    );
  }
  return builder;
}

export function buildOptionalNumberFormField(opts: NumberFieldOptions) {
  return buildOptionalNullableNumberFormField<number | undefined>(
    opts,
    undefined,
    opts.integer
      ? (e: Event) => tryParseInt((e.target as HTMLInputElement).value)
      : (e: Event) => tryParseFloat((e.target as HTMLInputElement).value),
  );
}

function buildNullableNumberFormField(opts: NumberFieldOptions) {
  return buildOptionalNullableNumberFormField<string | null>(
    opts,
    null,
    opts.integer
      ? (e: Event) =>
          tryParseInt((e.target as HTMLInputElement).value)?.toString() ?? ""
      : (e: Event) =>
          tryParseFloat((e.target as HTMLInputElement).value)?.toString() ?? "",
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

export function buildOptionalBoolFormField(opts: {
  label: () => JSX.Element;
  info?: JSX.Element;
}) {
  return (field: () => FieldApiT<boolean | undefined>) => (
    <div
      class={`grid items-center ${gapStyle}`}
      style={{ "grid-template-columns": "auto 1fr" }}
    >
      <Label class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
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
}

interface SelectFieldOpts {
  label: () => JSX.Element;
  disabled?: boolean;
}

export function buildSelectField(options: string[], opts: SelectFieldOpts) {
  return (field: () => FieldApiT<string>) => {
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

function GridFieldInfo<T>(props: { field: FieldApiT<T> }) {
  const show = () => {
    const meta = props.field.state.meta;
    return meta.errors.length > 0 || meta.isValidating;
  };

  return (
    <Show when={show()}>
      <div class="col-start-2 ml-2 text-sm text-muted-foreground">
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

// NOTE: this is not a component but a builder:
//   "(field: () => FieldApiT<T>) => Component"
//
// The unused extra arg only exists to make this clear to eslint.
export function buildDBCellField(opts: {
  name: string;
  type: ColumnDataType;
  notNull: boolean;
  isPk: boolean;
  isUpdate: boolean;
  defaultValue: string | undefined;
}) {
  const typeLabel = () => `[${opts.type}${opts.notNull ? "" : "?"}]`;

  const label = () => (
    <div class="flex w-[100px] flex-wrap items-center gap-1 overflow-hidden">
      <span>{opts.name} </span>

      <Show when={opts.type === "Blob"} fallback={typeLabel()}>
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

  const type = opts.type;
  const nullable = isNullableColumn({
    type,
    notNull: opts.notNull,
    isPk: opts.isPk,
  });
  const placeholder: string | undefined = (() => {
    // Placeholders indicate default values. However, default values only apply
    // on first insert.
    if (opts.isUpdate) {
      return undefined;
    }
    const value = opts.defaultValue;
    if (value === undefined) {
      return undefined;
    }

    if (value.startsWith("(")) {
      return value;
    } else {
      const literal = literalDefault(type, value);
      if (literal === undefined || literal === null) {
        return undefined;
      }

      if (type === "Blob" && typeof literal === "string") {
        return `${literal} (decoded: ${urlSafeBase64Decode(literal)})`;
      }
      return literal.toString();
    }
  })();
  const disabled = opts.isUpdate && opts.isPk;

  if (type === "Text" || type === "Blob") {
    if (nullable) {
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
      // required: (!nullable && opts.defaultValue === undefined),
      placeholder,
    });
  }

  if (isReal(type)) {
    return buildNullableNumberFormField({
      label,
      disabled,
      integer: false,
      // required: (!nullable && opts.defaultValue === undefined),
      placeholder,
    });
  }

  console.debug(
    `Custom FormFields not implemented for '${type}'. Falling back to text field`,
  );

  if (nullable) {
    return buildNullableTextFormField({ label, placeholder, disabled });
  }
  return buildTextFormFieldT<string | null>({ label, placeholder, disabled });
}

export const gapStyle = "gap-x-2 gap-y-1";
