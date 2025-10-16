import { children, createSignal, For, Show, JSX } from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { urlSafeBase64Decode } from "trailbase";

import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

import type { Column } from "@bindings/Column";
import type { Table } from "@bindings/Table";
import type { ColumnAffinityType } from "@bindings/ColumnAffinityType";
import type { ColumnDataType } from "@bindings/ColumnDataType";

import { Checkbox } from "@/components/ui/checkbox";
import { gapStyle, GridFieldInfo } from "@/components/FormFields";
import type { FieldApiT } from "@/components/FormFields";
import { SheetContainer } from "@/components/SafeSheet";
import { showToast } from "@/components/ui/toast";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { getDefaultValue, isNotNull, isPrimaryKeyColumn } from "@/lib/schema";
import {
  buildDefaultRow,
  literalDefault,
  shallowCopySqlValue,
} from "@/lib/convert";
import type { Record } from "@/lib/convert";
import { updateRow, insertRow } from "@/lib/row";
import {
  sqlValueToString,
  getInteger,
  getReal,
  getText,
  getBlob,
} from "@/lib/value";
import type {
  SqlBlobValue,
  SqlIntegerValue,
  SqlNotNullValue,
  SqlNullValue,
  SqlRealValue,
  SqlTextValue,
  SqlValue,
} from "@/lib/value";
import { tryParseFloat, tryParseBigInt } from "@/lib/utils";
import { isNullableColumn } from "@/lib/schema";

export function InsertUpdateRowForm(props: {
  close: () => void;
  markDirty: () => void;
  rowsRefetch: () => void;
  schema: Table;
  row?: Record;
}) {
  const isUpdate = () => props.row !== undefined;

  const form = createForm(() => {
    const defaultValues: Record = props.row
      ? { ...props.row }
      : buildDefaultRow(props.schema);

    return {
      defaultValues,
      onSubmit: async ({ value }: { value: Record }) => {
        console.debug(`Submitting ${isUpdate() ? "update" : "insert"}:`, value);
        try {
          if (isUpdate()) {
            // NOTE: updateRow mutates the value - it deletes the pk, thus shallow copy.
            // NOTE: value['key'] === undefined won't be serialized to JSON and thus sent.
            await updateRow(props.schema, { ...value });
          } else {
            await insertRow(props.schema, { ...value });
          }

          props.rowsRefetch();
          props.close();
        } catch (err) {
          showToast({
            title: "Submit Failed",
            description: `${err}`,
            variant: "error",
          });
        }
      },
    };
  });

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  form.createField(() => ({
    name: "row.test",
  }));

  return (
    <SheetContainer>
      <SheetHeader>
        <SheetTitle>{isUpdate() ? "Edit Row" : "Insert New Row"}</SheetTitle>
      </SheetHeader>

      <form
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col items-start gap-4 py-4">
          <For each={props.schema.columns}>
            {(column: Column) => {
              return (
                <form.Field
                  name={column.name}
                  validators={{
                    onChange: ({ value }: { value: SqlValue | undefined }) => {
                      return isUpdate()
                        ? validateUpdateSqlValueFormField({
                            column,
                            value,
                          })
                        : validateInsertSqlValueFormField({
                            column,
                            value,
                          });
                    },
                  }}
                >
                  {buildSqlValueFormField({
                    column,
                    isUpdate: isUpdate(),
                  })}
                </form.Field>
              );
            }}
          </For>
        </div>

        <SheetFooter>
          <form.Subscribe
            selector={(state) => ({
              canSubmit: state.canSubmit,
              isSubmitting: state.isSubmitting,
            })}
            children={(state) => {
              return (
                <Button
                  type="submit"
                  disabled={!state().canSubmit}
                  variant="default"
                >
                  {state().isSubmitting
                    ? "..."
                    : isUpdate()
                      ? "Update"
                      : "Insert"}
                </Button>
              );
            }}
          />
        </SheetFooter>
      </form>
    </SheetContainer>
  );
}

function FormRow<
  T extends
    | SqlRealValue
    | SqlIntegerValue
    | SqlTextValue
    | SqlBlobValue
    | SqlNullValue
    | undefined,
>(props: { children: JSX.Element; field: () => FieldApiT<T> }) {
  const c = children(() => props.children);

  return (
    <div
      class={`grid items-center ${gapStyle}`}
      style={{ "grid-template-columns": "auto 1fr 16px" }}
    >
      {c()}

      <div class="col-start-1">
        <GridFieldInfo field={props.field()} />
      </div>
    </div>
  );
}

type SqlFormFieldOptions = {
  label: JSX.Element;
  disabled: boolean;
  nullable: boolean;
  hasDefault: boolean;
  placeholder: (initial: SqlValue | undefined, disabled: boolean) => string;
};

function initialState(
  field: FieldApiT<SqlValue | undefined>,
  opts: {
    disabled: boolean;
    nullable: boolean;
    hasDefault: boolean;
  },
): {
  initialValue: SqlValue | undefined;
  initiallyDisabled: boolean;
} {
  const initialValue = shallowCopySqlValue(field.state.value);

  function initiallyDisabled(): boolean {
    if (opts.disabled) return true;
    if (!opts.nullable) return false;

    // Insert-case only. Updates don't have a default but an initial value.
    if (opts.hasDefault) return false;

    return initialValue === undefined || initialValue === "Null";
  }

  return {
    initialValue,
    initiallyDisabled: initiallyDisabled(),
  };
}

function buildSqlIntegerFormField(opts: SqlFormFieldOptions) {
  return function builder(field: () => FieldApiT<SqlValue | undefined>) {
    const { initialValue, initiallyDisabled } = initialState(field(), opts);

    const [disabled, setDisabled] = createSignal<boolean>(initiallyDisabled);

    return (
      <TextField class="w-full">
        <FormRow field={field}>
          <TextFieldLabel>{opts.label}</TextFieldLabel>

          <TextFieldInput
            disabled={disabled()}
            type={disabled() ? "number" : "text"}
            step={1}
            pattern={"[ ]*[0-9]+[ ]*"}
            value={getInteger(field().state.value) ?? ""}
            placeholder={opts.placeholder(initialValue, disabled())}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const parsed = tryParseBigInt(
                (e.target as HTMLInputElement).value,
              );
              field().handleChange(
                parsed !== undefined ? { Integer: parsed } : undefined,
              );
            }}
          />

          {opts.nullable && (
            <Checkbox
              disabled={opts.disabled}
              defaultChecked={!initiallyDisabled}
              onChange={(enabled: boolean) => {
                setDisabled(!enabled);

                const value = enabled ? (initialValue ?? "Null") : "Null";
                field().handleChange(value);
              }}
              data-testid="toggle"
            />
          )}
        </FormRow>
      </TextField>
    );
  };
}

function buildSqlRealFormField(opts: SqlFormFieldOptions) {
  return function builder(field: () => FieldApiT<SqlValue | undefined>) {
    const { initialValue, initiallyDisabled } = initialState(field(), opts);

    const [disabled, setDisabled] = createSignal<boolean>(initiallyDisabled);

    return (
      <TextField class="w-full">
        <FormRow field={field}>
          <TextFieldLabel>{opts.label}</TextFieldLabel>

          <TextFieldInput
            disabled={disabled()}
            type={"text"}
            pattern="[ ]*[0-9]+[.,]?[0-9]*[ ]*"
            value={getReal(field().state.value) ?? ""}
            placeholder={opts.placeholder(initialValue, disabled())}
            autocomplete={false}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const parsed = tryParseFloat(
                (e.target as HTMLInputElement).value,
              );
              field().handleChange(
                parsed !== undefined ? { Real: parsed } : undefined,
              );
            }}
          />

          {opts.nullable && (
            <Checkbox
              disabled={opts.disabled}
              defaultChecked={!initiallyDisabled}
              onChange={(enabled: boolean) => {
                setDisabled(!enabled);

                const value = enabled ? (initialValue ?? "Null") : "Null";
                field().handleChange(value);
              }}
              data-testid="toggle"
            />
          )}
        </FormRow>
      </TextField>
    );
  };
}

// TODO: Right now we don't have a good way of distinguishing between
// empty string and a non-empty default value. We'd need some other UI
// element. An empty-input field is ambiguous.
function buildSqlTextFormField(opts: SqlFormFieldOptions) {
  return function builder(field: () => FieldApiT<SqlValue | undefined>) {
    const { initialValue, initiallyDisabled } = initialState(field(), opts);

    const [disabled, setDisabled] = createSignal<boolean>(initiallyDisabled);

    return (
      <TextField class="w-full">
        <FormRow field={field}>
          <TextFieldLabel>{opts.label}</TextFieldLabel>

          <TextFieldInput
            disabled={disabled()}
            type={"text"}
            value={getText(field().state.value) ?? ""}
            placeholder={opts.placeholder(initialValue, disabled())}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const value: string = (e.target as HTMLInputElement).value;
              // NOTE: There's no way to get back to `value === undefined` to
              // apply the column default.
              field().handleChange({ Text: value });
            }}
            data-testid="input"
          />

          {opts.nullable && (
            <Checkbox
              disabled={opts.disabled}
              defaultChecked={!initiallyDisabled}
              onChange={(enabled: boolean) => {
                setDisabled(!enabled);

                const value = enabled ? (initialValue ?? "Null") : "Null";
                field().handleChange(value);
              }}
              data-testid="toggle"
            />
          )}
        </FormRow>
      </TextField>
    );
  };
}

function buildSqlBlobFormField(opts: SqlFormFieldOptions) {
  return function builder(field: () => FieldApiT<SqlValue | undefined>) {
    const { initialValue, initiallyDisabled } = initialState(field(), opts);

    const [disabled, setDisabled] = createSignal<boolean>(initiallyDisabled);

    return (
      <TextField class="w-full">
        <FormRow field={field}>
          <TextFieldLabel>{opts.label}</TextFieldLabel>

          <TextFieldInput
            disabled={disabled()}
            type={"text"}
            value={getBlob(field().state.value) ?? ""}
            placeholder={opts.placeholder(initialValue, disabled())}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              const value: string = (e.target as HTMLInputElement).value;

              // FIXME: Missing input validation.
              field().handleChange({ Blob: { Base64UrlSafe: value } });
            }}
            data-testid="input"
          />

          {opts.nullable && (
            <Checkbox
              disabled={opts.disabled}
              defaultChecked={!initiallyDisabled}
              onChange={(enabled: boolean) => {
                setDisabled(!enabled);

                const value = enabled ? (initialValue ?? "Null") : "Null";
                field().handleChange(value);
              }}
              data-testid="toggle"
            />
          )}
        </FormRow>
      </TextField>
    );
  };
}

function buildSqlAnyFormField(
  opts: SqlFormFieldOptions & { affinity_type: ColumnAffinityType },
) {
  return function builder(field: () => FieldApiT<SqlValue | undefined>) {
    const { initialValue, initiallyDisabled } = initialState(field(), opts);

    const [disabled, setDisabled] = createSignal<boolean>(initiallyDisabled);

    const value = (): string | undefined => {
      const v = field().state.value;
      return v !== undefined ? sqlValueToString(v) : undefined;
    };

    return (
      <TextField class="w-full">
        <FormRow field={field}>
          <TextFieldLabel>{opts.label}</TextFieldLabel>

          <TextFieldInput
            disabled={disabled()}
            type={"text"}
            value={value() ?? ""}
            placeholder={opts.placeholder(initialValue, disabled())}
            onBlur={field().handleBlur}
            onInput={(e: Event) => {
              field().handleChange(
                parseUsingAffinity(
                  opts.affinity_type,
                  (e.target as HTMLInputElement).value,
                ),
              );
            }}
            data-testid="input"
          />

          {opts.nullable && (
            <Checkbox
              disabled={opts.disabled}
              defaultChecked={!initiallyDisabled}
              onChange={(enabled: boolean) => {
                setDisabled(!enabled);

                const value = enabled ? (initialValue ?? "Null") : "Null";
                field().handleChange(value);
              }}
              data-testid="toggle"
            />
          )}
        </FormRow>
      </TextField>
    );
  };
}

function parseUsingAffinity(
  affinity_type: ColumnAffinityType,
  value: string,
): SqlValue {
  if (value === "NULL") {
    return "Null";
  }

  switch (affinity_type) {
    case "Text":
      return { Text: value };
    case "Integer": {
      const i = tryParseBigInt(value);
      if (i !== undefined) {
        return { Integer: i };
      }
      return { Text: value };
    }
    case "Real": {
      const f = tryParseFloat(value);
      if (f !== undefined) {
        return { Real: f };
      }
      return { Text: value };
    }
    case "Numeric": {
      const i = tryParseBigInt(value);
      if (i !== undefined) {
        return { Integer: i };
      }
      const f = tryParseFloat(value);
      if (f !== undefined) {
        return { Real: f };
      }
      return { Text: value };
    }
    case "Blob": {
      if (value.startsWith("x'") || value.startsWith("X'")) {
        return {
          Blob: {
            Hex: value,
          },
        };
      }

      return {
        Blob: {
          Base64UrlSafe: value,
        },
      };
    }
  }
}

function initialValuePlaceholder(initial: SqlValue | undefined) {
  if (initial === undefined) return "";
  if (initial === "Null") return "(current: NULL)";

  if ("Text" in initial) {
    return `(current: '${initial.Text}')`;
  }
  if ("Blob" in initial) {
    const blob = initial.Blob;
    if ("Base64UrlSafe" in blob) {
      return `(current: ${blob.Base64UrlSafe} (${urlSafeBase64Decode(blob.Base64UrlSafe)})`;
    } else if ("Hex" in blob) {
      return `(current: ${blob.Hex})`;
    } else {
      return `(current: ${blob.Array})`;
    }
  }
  return `(current: ${sqlValueToString(initial)})`;
}

function defaultValuePlaceholder(
  type: ColumnDataType,
  defaultValue: string | undefined,
): string {
  // Placeholders indicate default values. However, default values only apply
  // on first insert.
  if (defaultValue === undefined) {
    return "";
  }

  if (defaultValue.startsWith("(")) {
    return `(default: ${defaultValue})`;
  } else {
    const literal = literalDefault(type, defaultValue);
    if (literal === undefined || literal === null) {
      return "";
    }

    if (type === "Blob" && typeof literal === "string") {
      return `(default: ${literal} (${urlSafeBase64Decode(literal)}))`;
    }

    if (type === "Text") {
      return `(default: '${literal}')`;
    }
    return `(default: ${literal})`;
  }
}

function Label(props: {
  name: string;
  type: ColumnDataType;
  notNull: boolean;
}) {
  const typeLabel = () => `[${props.type}${props.notNull ? "" : "?"}]`;

  return (
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
}

// NOTE: this is not a component but a builder:
//   "(field: () => FieldApiT<T>) => Component"
//
// The unused extra arg only exists to make this clear to eslint.
// TODO: For foreign keys we'd ideally render a auto-complete search bar.
function buildSqlValueFormField(opts: {
  column: Column;
  isUpdate: boolean;
}): (field: () => FieldApiT<SqlValue | undefined>) => JSX.Element {
  const name = opts.column.name;
  const type: ColumnDataType = opts.column.data_type;

  const isPk = isPrimaryKeyColumn(opts.column);
  const notNull = isNotNull(opts.column.options);
  const nullable = isNullableColumn({
    type,
    notNull: notNull,
    isPk: isPk,
  });

  // NOTE: default values only apply on insert and not during updates.
  const defaultValue = opts.isUpdate
    ? undefined
    : getDefaultValue(opts.column.options);

  function placeholder(
    initial: SqlValue | undefined,
    disabled: boolean,
  ): string {
    if (disabled) return "NULL";
    return opts.isUpdate
      ? initialValuePlaceholder(initial)
      : defaultValuePlaceholder(type, defaultValue);
  }

  switch (type) {
    case "Integer":
      return buildSqlIntegerFormField({
        label: <Label name={name} type={type} notNull={notNull} />,
        nullable,
        hasDefault: defaultValue !== undefined,
        placeholder,
        disabled: opts.isUpdate && isPk,
      });
    case "Real":
      return buildSqlRealFormField({
        label: <Label name={name} type={type} notNull={notNull} />,
        nullable,
        hasDefault: defaultValue !== undefined,
        placeholder,
        disabled: opts.isUpdate && isPk,
      });
    case "Text":
      return buildSqlTextFormField({
        label: <Label name={name} type={type} notNull={notNull} />,
        nullable,
        hasDefault: defaultValue !== undefined,
        placeholder,
        disabled: opts.isUpdate && isPk,
      });
    case "Blob":
      return buildSqlBlobFormField({
        label: <Label name={name} type={type} notNull={notNull} />,
        nullable,
        hasDefault: defaultValue !== undefined,
        placeholder,
        disabled: opts.isUpdate && isPk,
      });
    case "Any":
      return buildSqlAnyFormField({
        label: <Label name={name} type={type} notNull={notNull} />,
        nullable,
        hasDefault: defaultValue !== undefined,
        placeholder,
        affinity_type: opts.column.affinity_type,
        disabled: opts.isUpdate && isPk,
      });
  }
}

function assertColumnType(expected: ColumnDataType, value: SqlNotNullValue) {
  function assert(expected: ColumnDataType, got: ColumnDataType) {
    if (expected !== "Any" && got !== expected) {
      throw Error(`Expected ${expected}, got: ${got}`);
    }
  }

  if ("Integer" in value) {
    assert(expected, "Integer");
  }
  if ("Real" in value) {
    assert(expected, "Real");
  }
  if ("Text" in value) {
    assert(expected, "Text");
  }
  if ("Blob" in value) {
    assert(expected, "Blob");
  }
}

/// Returns undefined when input considered good.
function validateUpdateSqlValueFormField({
  column,
  value,
}: {
  column: Column;
  value: SqlValue | undefined;
}): string | undefined {
  const type: ColumnDataType = column.data_type;
  const isPk: boolean = isPrimaryKeyColumn(column);
  const notNull: boolean = isNotNull(column.options);
  const nullable: boolean = isNullableColumn({
    type,
    notNull: notNull,
    isPk: isPk,
  });

  // During update, undefined simply means: don't send and preserve currently
  // stored value. No validation needed.
  if (value === undefined) {
    return undefined;
  }

  if (value === "Null") {
    if (nullable) return undefined;
    throw Error(
      `Null for not-nullable. Form should have disallowed: ${column}`,
    );
  }

  assertColumnType(type, value);

  // Input validated by input element itself.
  // if ("Integer" in value) { }
  // if ("Real" in value) { }

  if ("Blob" in value) {
    const blob = value.Blob;
    if ("Base64UrlSafe" in blob) {
      try {
        // TODO: We seem to be a lot more lax then what the server expects in terms of padding.
        // Either lax the server or be more strict here for predictable input validation.
        urlSafeBase64Decode(blob.Base64UrlSafe);
      } catch {
        return "Not valid url-safe b64";
      }
      return undefined;
    }
    throw Error("Expected Base64UrlSafe");
  }

  if ("Text" in value) {
    /// TODO: Validation could be more comprehensive, e.g. JSON inputs.
  }

  // Pass validation.
  return undefined;
}

function validateInsertSqlValueFormField({
  column,
  value,
}: {
  column: Column;
  value: SqlValue | undefined;
}): string | undefined {
  const type: ColumnDataType = column.data_type;
  const isPk: boolean = isPrimaryKeyColumn(column);
  const notNull: boolean = isNotNull(column.options);
  const nullable: boolean = isNullableColumn({
    type,
    notNull: notNull,
    isPk: isPk,
  });

  // NOTE: default values only apply on insert and not during updates.
  const defaultValue: string | undefined = getDefaultValue(column.options);

  // During insert undefined means the column must either be nullable or have a default;
  // stored value. No validation needed.
  if (value === undefined) {
    if (defaultValue !== undefined || nullable) {
      return undefined;
    }
    return `Missing value for: ${column}`;
  }

  if (value === "Null") {
    if (nullable) return undefined;
    throw Error(
      `Null for not-nullable. Form should have disallowed: ${column}`,
    );
  }

  assertColumnType(type, value);

  // Input validated by input element itself.
  // if ("Integer" in value) { }
  // if ("Real" in value) { }

  if ("Blob" in value) {
    const blob = value.Blob;
    if ("Base64UrlSafe" in blob) {
      try {
        // TODO: We seem to be a lot more lax then what the server expects in terms of padding.
        // Either lax the server or be more strict here for predictable input validation.
        urlSafeBase64Decode(blob.Base64UrlSafe);
      } catch {
        return "Not valid url-safe b64";
      }
      return undefined;
    }
    throw Error("Expected Base64UrlSafe");
  }

  if ("Text" in value) {
    /// TODO: Validation could be more comprehensive, e.g. JSON inputs.
  }

  // Pass validation.
  return undefined;
}
