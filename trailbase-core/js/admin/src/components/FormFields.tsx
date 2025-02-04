import { createSignal } from "solid-js";
import type { Accessor, Setter, JSXElement } from "solid-js";
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
export type FieldApiT<T> = FieldApi<T, any>;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type FormType<T> = ReturnType<typeof createForm<T, any>>;

type TextFieldOptions = {
  disabled?: boolean;
  type?: TextFieldType;
  onKeyUp?: Setter<string>;
  required?: boolean;

  label: () => JSXElement;
  info?: JSXElement;
  autocomplete?: string;

  // Optional placeholder string for absent values, e.g. "NULL". Optional only option.
  nullPlaceholder?: string;
};

export function buildTextFormField(opts: TextFieldOptions) {
  const keyUp = opts.onKeyUp;

  return (field: () => AnyFieldApi) => (
    <TextField class="w-full">
      <div
        class={`grid items-center ${gapStyle}`}
        style="grid-template-columns: auto 1fr"
      >
        <TextFieldLabel>{opts.label()}</TextFieldLabel>

        <TextFieldInput
          required={opts.required ?? true}
          disabled={opts.disabled ?? false}
          type={opts.type ?? "text"}
          value={field().state.value ?? ""}
          onBlur={field().handleBlur}
          autocomplete={opts.autocomplete}
          autocorrect={opts.type === "password" ? "off" : undefined}
          onKeyUp={(e: Event) => {
            const v = (e.target as HTMLInputElement).value;
            field().handleChange(v);
            if (keyUp) {
              keyUp(v);
            }
          }}
        />

        <div class="col-start-2 ml-2 text-sm text-muted-foreground">
          {field && <FieldInfo field={field()} />}
        </div>

        <div class="col-start-2 text-sm">{opts.info}</div>
      </div>
    </TextField>
  );
}

export function buildSecretFormField(opts: Omit<TextFieldOptions, "type">) {
  const keyUp = opts.onKeyUp;
  const [type, setType] = createSignal<TextFieldType>("password");

  return (field: () => AnyFieldApi) => (
    <TextField class="w-full">
      <div
        class={`grid items-center ${gapStyle}`}
        style="grid-template-columns: auto 1fr"
      >
        <TextFieldLabel>{opts.label()}</TextFieldLabel>

        <div class="flex gap-2 items-center">
          <TextFieldInput
            required={opts.required ?? true}
            disabled={opts.disabled ?? false}
            type={type()}
            value={field().state.value ?? ""}
            onBlur={field().handleBlur}
            autocomplete={opts.autocomplete ?? "off"}
            autocorrect="off"
            onKeyUp={(e: Event) => {
              const v = (e.target as HTMLInputElement).value;
              field().handleChange(v);
              if (keyUp) {
                keyUp(v);
              }
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

export function OptionalTextFormField(props: {
  label: () => JSXElement;
  info?: JSXElement;
  field?: () => AnyFieldApi;

  type?: TextFieldType;
  onKeyUp?: Setter<string>;

  initial?: string;
  initialEnabled?: boolean;
  disabled?: boolean;

  nullPlaceholder?: string;

  handleBlur?: () => void;
  handleChange?: (v: string | undefined) => void;
}) {
  const [text, setText] = createSignal(props.initial ?? "");
  const [enabled, setEnabled] = createSignal<boolean>(
    props.initialEnabled ?? props.initial !== undefined,
  );

  const externDisable = props.disabled ?? false;
  const effEnabled = () => enabled() && !externDisable;

  const keyUp = props.onKeyUp;

  return (
    <TextField class="w-full">
      <div
        class={`grid items-center ${gapStyle}`}
        style="grid-template-columns: auto 1fr"
      >
        <TextFieldLabel class={effEnabled() ? "" : "text-muted-foreground"}>
          {props.label()}
        </TextFieldLabel>

        <div class="w-full flex items-center">
          <TextFieldInput
            required={false}
            type={props.type ?? "text"}
            value={effEnabled() ? text() : (props.nullPlaceholder ?? "NULL")}
            disabled={!effEnabled()}
            onBlur={props.handleBlur}
            onKeyUp={(e: Event) => {
              const v = (e.target as HTMLInputElement).value;
              setText(v);
              props.handleChange?.(v);
              if (keyUp) {
                keyUp(v);
              }
            }}
            onChange={(e: Event) => {
              const v: string = (e.currentTarget as HTMLInputElement).value;
              props.handleChange?.(v);
            }}
          />

          <Checkbox
            disabled={externDisable}
            checked={enabled()}
            onChange={(value: boolean) => {
              setEnabled(value);
              // NOTE: null is critical here to actively unset a cell, undefined
              // would merely take it out of the patch set.
              props.handleChange?.(value ? text() : undefined);
            }}
          />
        </div>

        <div class="col-start-2 ml-2 text-sm text-muted-foreground">
          {props.field && <FieldInfo field={props.field()} />}
        </div>

        <div class="col-start-2 text-sm">{props.info}</div>
      </div>
    </TextField>
  );
}

export function buildOptionalTextFormField(opts: TextFieldOptions) {
  return (field: () => AnyFieldApi) => {
    return (
      <OptionalTextFormField
        initial={field().state.value}
        initialEnabled={field().state.value}
        disabled={opts.disabled}
        field={field}
        label={opts.label}
        info={opts.info}
        type={opts.type}
        onKeyUp={opts.onKeyUp}
        nullPlaceholder={opts.nullPlaceholder}
        handleBlur={field().handleBlur}
        handleChange={field().handleChange}
      />
    );
  };
}

export function buildTextAreaFormField(opts: TextFieldOptions, rows?: number) {
  const keyUp = opts?.onKeyUp;

  return (field: () => AnyFieldApi) => (
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
            const v = (e.target as HTMLInputElement).value;
            field().handleChange(v);
            if (keyUp) {
              keyUp(v);
            }
          }}
        />

        <div class="col-start-2 ml-2 text-sm text-muted-foreground">
          {field && <FieldInfo field={field()} />}
        </div>

        <div class="col-start-2 text-sm">{opts.info}</div>
      </div>
    </TextField>
  );
}

type NumberFieldOptions = {
  disabled?: boolean;
  label: () => JSXElement;

  info?: JSXElement;
  integer?: boolean;
};

export function buildNumberFormField(opts: NumberFieldOptions) {
  const isInt = opts.integer ?? false;

  return (field: () => AnyFieldApi) => {
    return (
      <TextField class="w-full">
        <div
          class={`grid items-center ${gapStyle}`}
          style="grid-template-columns: auto 1fr"
        >
          <TextFieldLabel>{opts.label()}</TextFieldLabel>

          <TextFieldInput
            required={true}
            type="number"
            step={isInt ? "1" : undefined}
            pattern={isInt ? "d+" : undefined}
            value={field().state.value}
            disabled={opts?.disabled}
            onBlur={field().handleBlur}
            onKeyUp={(e: Event) => {
              const v = (e.target as HTMLInputElement).value;
              const i = parseInt(v);
              field().handleChange(i);
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

export function buildBoolFormField(props: { label: () => JSXElement }) {
  return (field: () => AnyFieldApi) => (
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
  label: () => JSXElement;
  disabled?: Accessor<boolean>;
  multiple?: boolean;
}

export function buildSelectField<T>(options: T[], opts: SelectFieldOpts) {
  return (field: () => AnyFieldApi) => (
    <div
      class={`w-full grid items-center ${gapStyle}`}
      style="grid-template-columns: auto 1fr"
    >
      <Label>{opts.label()}</Label>

      <Select
        multiple={opts?.multiple ?? false}
        value={field().state.value}
        onBlur={field().handleBlur}
        onChange={field().handleChange}
        options={options}
        itemComponent={(props) => (
          <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
        )}
        disabled={opts?.disabled?.() ?? false}
      >
        <SelectTrigger>
          <SelectValue<string>>{(state) => state.selectedOption()}</SelectValue>
        </SelectTrigger>

        <SelectContent />
      </Select>
    </div>
  );
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

export function formFieldBuilder(
  type: ColumnDataType,
  labelText: string,
  optional: boolean,
  nullPlaceholder?: string,
) {
  const label = () => (
    <div class="w-[100px] flex gap-2 items-center">
      {type === "Blob" && (
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
      )}

      {labelText}
    </div>
  );

  if (type === "Text" || type === "Blob") {
    if (optional) {
      return buildOptionalTextFormField({ label, nullPlaceholder });
    } else {
      return buildTextFormField({ label });
    }
  }

  if (type === "Integer" && !optional) {
    return buildNumberFormField({ label });
  }

  console.debug(
    `Custom FormFields not yet implemented for (${type}, ${optional}). Falling back to textfields`,
  );
  if (optional) {
    return buildOptionalTextFormField({ label, nullPlaceholder });
  } else {
    return buildTextFormField({ label });
  }
}

export const gapStyle = "gap-x-2 gap-y-1";
