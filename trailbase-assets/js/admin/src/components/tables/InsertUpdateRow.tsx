import { createMemo, For } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

import type { Column } from "@bindings/Column";
import type { Table } from "@bindings/Table";

import { buildDBCellField, isNumber } from "@/components/FormFields";
import {
  getDefaultValue,
  isNotNull,
  isOptional,
  isPrimaryKeyColumn,
} from "@/lib/schema";
import { SheetContainer } from "@/components/SafeSheet";
import { showToast } from "@/components/ui/toast";
import { copyRow, type FormRow } from "@/lib/convert";
import { updateRow, insertRow } from "@/lib/row";

function buildDefault(schema: Table): FormRow {
  const obj: FormRow = {};
  for (const col of schema.columns) {
    const optional = isOptional(col.options);
    if (optional) {
      // obj[col.name] = undefined;
      continue;
    }

    switch (col.data_type) {
      case "Blob":
        obj[col.name] = "";
        break;
      case "Text":
        obj[col.name] = "";
        break;
      case "Real":
        obj[col.name] = 0.0;
        break;
      case "Integer":
        obj[col.name] = 0;
        break;
      case "Null":
        break;
    }
  }
  return obj;
}

type FormRowForm = {
  row: FormRow;
};

export function InsertUpdateRowForm(props: {
  close: () => void;
  markDirty: () => void;
  rowsRefetch: () => void;
  schema: Table;
  row?: FormRow;
}) {
  const original = createMemo(() =>
    props.row ? copyRow(props.row) : undefined,
  );
  const isUpdate = original !== undefined;

  const form = createForm(() => ({
    defaultValues: {
      row: props.row ?? buildDefault(props.schema),
    } as FormRowForm,
    onSubmit: async ({ value }: { value: FormRowForm }) => {
      console.debug(`Submitting ${isUpdate ? "update" : "insert"}:`, value);
      try {
        const o = original();
        if (o) {
          await updateRow(props.schema, value.row);
        } else {
          await insertRow(props.schema, value.row);
        }

        props.rowsRefetch();
        props.close();
      } catch (err) {
        showToast({
          title: "Uncaught Error",
          description: `${err}`,
          variant: "error",
        });
      }
    },
  }));

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  return (
    <SheetContainer>
      <SheetHeader>
        <SheetTitle>{original() ? "Edit Row" : "Insert New Row"}</SheetTitle>
      </SheetHeader>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          e.stopPropagation();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col items-start gap-4 py-4">
          <For each={props.schema.columns}>
            {(col: Column) => {
              const pk = isPrimaryKeyColumn(col);
              const notNull = isNotNull(col.options);
              const defaultValue = getDefaultValue(col.options);

              return (
                <form.Field
                  name={`row[${col.name}]`}
                  validators={{
                    onChange: ({
                      value,
                    }: {
                      value: string | number | null | undefined;
                    }) => {
                      const required = notNull && defaultValue === undefined;
                      if (value === undefined) {
                        if (required) {
                          return `Missing value for ${col.name}`;
                        }
                        return undefined;
                      }

                      // TODO: Better input validation or better typed form fields.
                      // NOTE: this is currently rather pointless, since number
                      // field will resolve empty to null and required is handled
                      // via the input element.
                      if (value === "" && isNumber(col.data_type)) {
                        return `Invalid value for: ${col.data_type}`;
                      }
                      return undefined;
                    },
                  }}
                >
                  {buildDBCellField({
                    name: col.name,
                    type: col.data_type,
                    notNull,
                    disabled: isUpdate && pk,
                    placeholder: defaultValue ?? "",
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
                    : original()
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
