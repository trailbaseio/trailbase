import { createMemo, For } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

import type { Column } from "@bindings/Column";
import type { Table } from "@bindings/Table";

import { buildDBCellField } from "@/components/FormFields";
import {
  getDefaultValue,
  isInt,
  isNotNull,
  isPrimaryKeyColumn,
  isReal,
} from "@/lib/schema";
import { SheetContainer } from "@/components/SafeSheet";
import { showToast } from "@/components/ui/toast";
import {
  copyRow,
  preProcessInsertValue,
  preProcessUpdateValue,
  type FormRow,
} from "@/lib/convert";
import { updateRow, insertRow } from "@/lib/row";
import { isNullableColumn } from "@/lib/schema";

function buildDefault(schema: Table): FormRow {
  const obj: FormRow = {};
  for (const col of schema.columns) {
    const type = col.data_type;
    const isPk = isPrimaryKeyColumn(col);
    const notNull = isNotNull(col.options);
    const defaultValue = getDefaultValue(col.options);
    const nullable = isNullableColumn({
      type: col.data_type,
      notNull,
      isPk,
    });

    /// If there's no default and the column is nullable we default to null.
    if (defaultValue === undefined) {
      if (nullable) {
        obj[col.name] = null;
        continue;
      }
    } else {
      // If there is a default, we leave the form field empty and show the default as a textinput placeholder.
      obj[col.name] = "";
      continue;
    }

    // If there's neither a default nor is the column nullable, we fall back to generic defaults.
    // They may be invalid based on CHECK constraints.
    if (type === "Blob") {
      obj[col.name] = "";
      break;
    } else if (type === "Text") {
      obj[col.name] = "";
      break;
    } else if (isInt(type)) {
      obj[col.name] = 0;
      break;
    } else if (isReal(type)) {
      obj[col.name] = 0.0;
      break;
    } else if (type === "Null") {
      obj[col.name] = null;
      break;
    } else {
      console.debug(`No fallback for: ${col.name}`);
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
  const defaultValues = createMemo(() =>
    props.row ? copyRow(props.row) : buildDefault(props.schema),
  );
  const isUpdate = () => props.row !== undefined;

  const form = createForm(() => ({
    defaultValues: {
      row: defaultValues(),
    } as FormRowForm,
    onSubmit: async ({ value }: { value: FormRowForm }) => {
      console.debug(`Submitting ${isUpdate() ? "update" : "insert"}:`, value);
      try {
        if (isUpdate()) {
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
        <SheetTitle>{isUpdate() ? "Edit Row" : "Insert New Row"}</SheetTitle>
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
              const isPk = isPrimaryKeyColumn(col);
              const notNull = isNotNull(col.options);
              const defaultValue = getDefaultValue(col.options);

              return (
                <form.Field
                  name={`row[${col.name}]`}
                  validators={{
                    onChange: ({
                      value,
                    }: {
                      value: string | number | null;
                    }) => {
                      try {
                        if (isUpdate()) {
                          preProcessUpdateValue(col, value);
                        } else {
                          preProcessInsertValue(col, value);
                        }
                      } catch (e) {
                        return `Invalid value for ${col.name}: ${e}`;
                      }
                      return undefined;
                    },
                  }}
                >
                  {buildDBCellField({
                    name: col.name,
                    type: col.data_type,
                    notNull: notNull,
                    isPk,
                    isUpdate: isUpdate(),
                    defaultValue,
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
