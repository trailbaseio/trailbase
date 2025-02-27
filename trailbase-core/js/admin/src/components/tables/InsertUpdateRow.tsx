import { For } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

import type { Column, Table } from "@/lib/bindings";
import type { InsertRowRequest } from "@bindings/InsertRowRequest";
import type { UpdateRowRequest } from "@bindings/UpdateRowRequest";

import { buildDBCellField } from "@/components/FormFields";
import {
  findPrimaryKeyColumnIndex,
  getDefaultValue,
  isNotNull,
  isOptional,
  isPrimaryKeyColumn,
} from "@/lib/schema";
import { adminFetch } from "@/lib/fetch";
import { SheetContainer } from "@/components/SafeSheet";
import { showToast } from "@/components/ui/toast";
import { copyRow, type FormRow } from "@/lib/convert";

async function insertRow(tableName: string, row: FormRow) {
  const request: InsertRowRequest = {
    row: copyRow(row),
  };

  const response = await adminFetch(`/table/${tableName}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(request),
  });

  return await response.text();
}

async function updateRow(table: Table, row: FormRow) {
  const tableName = table.name;
  const primaryKeyColumIndex = findPrimaryKeyColumnIndex(table.columns);
  if (primaryKeyColumIndex < 0) {
    throw Error("No primary key column found.");
  }
  const pkColName = table.columns[primaryKeyColumIndex].name;

  const pkValue = row[pkColName];
  if (pkValue === undefined) {
    throw Error("Row is missing primary key.");
  }

  // Update cannot change the PK value.
  const copy = {
    ...row,
  };
  delete copy[pkColName];

  const request: UpdateRowRequest = {
    primary_key_column: pkColName,
    // eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
    primary_key_value: pkValue as Object,
    row: copyRow(copy),
  };

  const response = await adminFetch(`/table/${tableName}`, {
    method: "PATCH",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(request),
  });

  return await response.text();
}

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
  const original = props.row ? copyRow(props.row) : undefined;
  const isUpdate = original !== undefined;

  const form = createForm<FormRowForm>(() => ({
    defaultValues: {
      row: props.row ?? buildDefault(props.schema),
    },
    onSubmit: async ({ value }: { value: FormRowForm }) => {
      console.debug(`Submitting ${isUpdate ? "update" : "insert"}:`, value);
      try {
        if (original) {
          await updateRow(props.schema, value.row);
        } else {
          await insertRow(props.schema.name, value.row);
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
        <SheetTitle>{original ? "Edit Row" : "Insert New Row"}</SheetTitle>
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
                      if (value === "" && col.data_type !== "Text") {
                        return `Invalid value for: ${col.data_type}`;
                      }
                      return undefined;
                    },
                  }}
                  children={buildDBCellField({
                    name: col.name,
                    type: col.data_type,
                    notNull,
                    disabled: isUpdate && pk,
                    placeholder: defaultValue ?? "",
                  })}
                />
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
                    : original
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
