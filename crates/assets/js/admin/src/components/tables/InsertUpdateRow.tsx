import { createMemo, For } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

import type { Column } from "@bindings/Column";
import type { Table } from "@bindings/Table";

import { buildDBCellField } from "@/components/FormFields";
import { getDefaultValue, isNotNull, isPrimaryKeyColumn } from "@/lib/schema";
import { SheetContainer } from "@/components/SafeSheet";
import { showToast } from "@/components/ui/toast";
import {
  copyRow,
  preProcessInsertValue,
  preProcessUpdateValue,
  buildDefaultRow,
  type FormRow,
} from "@/lib/convert";
import { updateRow, insertRow } from "@/lib/row";

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
    props.row ? copyRow(props.row) : buildDefaultRow(props.schema),
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
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col items-start gap-4 py-4">
          <For each={props.schema.columns}>
            {(col: Column) => {
              const isPk = isPrimaryKeyColumn(col);
              const notNull = isNotNull(col.options);
              const defaultValue = getDefaultValue(col.options);

              // TODO: For foreign keys we'd ideally render a auto-complete search bar.
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
