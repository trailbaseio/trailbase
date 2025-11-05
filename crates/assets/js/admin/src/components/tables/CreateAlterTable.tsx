import { createMemo, createSignal, Index, Match, Show, Switch } from "solid-js";
import type { Accessor } from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { useQueryClient } from "@tanstack/solid-query";

import { showToast } from "@/components/ui/toast";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";

import { createTable, alterTable } from "@/lib/table";
import { randomName } from "@/lib/name";
import {
  buildBoolFormField,
  buildTextFormField,
} from "@/components/FormFields";
import { SheetContainer } from "@/components/SafeSheet";
import {
  PrimaryKeyColumnSubForm,
  ColumnSubForm,
  newDefaultColumn,
  primaryKeyPresets,
} from "@/components/tables/CreateAlterColumnForm";
import { invalidateConfig } from "@/lib/config";

import type { Column } from "@bindings/Column";
import type { Table } from "@bindings/Table";
import type { AlterTableOperation } from "@bindings/AlterTableOperation";
import type { QualifiedName } from "@bindings/QualifiedName";

function columnsEqual(a: Column, b: Column): boolean {
  return (
    a.name === b.name &&
    a.data_type === b.data_type &&
    JSON.stringify(a.options) === JSON.stringify(b.options)
  );
}

export function CreateAlterTableForm(props: {
  close: () => void;
  markDirty: () => void;
  schemaRefetch: () => Promise<void>;
  allTables: Table[];
  setSelected: (tableName: QualifiedName) => void;
  schema?: Table;
}) {
  const queryClient = useQueryClient();
  const [sql, setSql] = createSignal<string | undefined>();

  const copyOriginal = (): Table | undefined =>
    props.schema ? JSON.parse(JSON.stringify(props.schema)) : undefined;

  const original = createMemo<Table | undefined>(() => copyOriginal());
  const isCreateTable = () => original() === undefined;

  // Columns are treated as append only. Instead of removing it and inducing animation junk and other stuff when
  // shifting offset, we simply don't render columns that were marked as deleted.
  const [deletedColumns, setDeletedColumn] = createSignal<number[]>([]);
  const isDeleted = (i: number): boolean =>
    deletedColumns().find((idx) => idx === i) !== undefined;

  const onSubmit = async (value: Table, dryRun: boolean) => {
    /* eslint-disable solid/reactivity */
    console.debug("Table schema:", value);

    // Assert that the type representations match up.
    for (const column of value.columns) {
      if (column.data_type.toUpperCase() != column.type_name) {
        throw new Error(
          `Got ${column.type_name}, expected, ${column.data_type}`,
        );
      }
    }

    try {
      const o = original();
      if (o !== undefined) {
        // Alter table

        // Build operations. Remember columns are append-only.
        const operations: AlterTableOperation[] = [];
        value.columns.forEach((column, i) => {
          if (i < o.columns.length) {
            // Pre-existing column.
            const originalName = o.columns[i].name;
            if (isDeleted(i)) {
              operations.push({ DropColumn: { name: originalName } });
              return;
            }

            if (!columnsEqual(o.columns[i], column)) {
              operations.push({
                AlterColumn: {
                  name: originalName,
                  column: column,
                },
              });
              return;
            }
          } else {
            // Newly added columns.
            if (!isDeleted(i)) {
              operations.push({ AddColumn: { column: column } });
            }
          }
        });

        const response = await alterTable({
          source_schema: o,
          operations,
          dry_run: dryRun,
        });
        console.debug(`AlterTableResponse [dry: ${dryRun}]:`, response);

        if (dryRun) {
          setSql(response.sql);
        }
      } else {
        // Create table
        value.columns = value.columns.filter((_, i) => !isDeleted(i));

        const response = await createTable({ schema: value, dry_run: dryRun });
        console.debug(`CreateTableResponse [dry: ${dryRun}]:`, response);

        if (dryRun) {
          setSql(response.sql);
        }
      }

      if (!dryRun) {
        // Trigger config reload
        invalidateConfig(queryClient);

        // Reload schemas.
        props.schemaRefetch().then(() => {
          props.setSelected(value.name);
        });

        // Close dialog/sheet.
        props.close();
      }
    } catch (err) {
      showToast({
        title: "Uncaught Error",
        description: `${err}`,
        variant: "error",
      });
    }
  };

  const form = createForm(() => ({
    onSubmit: async ({ value }) => await onSubmit(value, /*dryRun=*/ false),
    defaultValues:
      copyOriginal() ??
      ({
        name: {
          name: randomName(),
          database_schema: null,
        },
        strict: true,
        indexes: [],
        columns: [
          {
            ...primaryKeyPresets[0][1]("id"),
          },
          newDefaultColumn(1),
        ] satisfies Column[],
        // Table constraints: https://www.sqlite.org/syntax/table-constraint.html
        unique: [],
        foreign_keys: [],
        checks: [],
        virtual_table: false,
        temporary: false,
      } as Table),
  }));

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  return (
    <SheetContainer>
      <SheetHeader>
        <SheetTitle>
          {isCreateTable() ? "Add New Table" : "Alter Table"}
        </SheetTitle>
      </SheetHeader>

      <form
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();
          form.handleSubmit();
        }}
      >
        <div class="mt-4 flex flex-col items-start gap-4 pr-4">
          <form.Field
            name="name.name"
            validators={{
              onChange: ({ value }: { value: string | undefined }) => {
                return value ? undefined : "Table name missing";
              },
            }}
          >
            {buildTextFormField({
              label: () => <TextLabel text="Table name" />,
            })}
          </form.Field>

          <Show when={isCreateTable()}>
            <form.Field
              name="strict"
              children={buildBoolFormField({
                label: () => "STRICT (type-safe)",
              })}
            />
          </Show>

          {/* columns */}
          <h2>Columns</h2>

          <form.Field name="columns">
            {(field) => (
              <div class="w-full">
                <div class="flex flex-col gap-2">
                  <Index each={field().state.value}>
                    {(c: Accessor<Column>, i: number) => (
                      <Show when={!isDeleted(i)}>
                        <Switch>
                          <Match when={i === 0}>
                            <PrimaryKeyColumnSubForm
                              form={form}
                              colIndex={i}
                              column={c()}
                              allTables={props.allTables}
                              disabled={!isCreateTable()}
                            />
                          </Match>

                          <Match when={i !== 0}>
                            <ColumnSubForm
                              form={form}
                              colIndex={i}
                              column={c()}
                              allTables={props.allTables}
                              disabled={false}
                              onDelete={() =>
                                setDeletedColumn([i, ...deletedColumns()])
                              }
                            />
                          </Match>
                        </Switch>
                      </Show>
                    )}
                  </Index>
                </div>

                <Button
                  class="m-2"
                  onClick={() => {
                    const columns = field().state.value;
                    field().pushValue(
                      newDefaultColumn(
                        columns.length,
                        columns.map((c) => c.name),
                      ),
                    );
                  }}
                  variant="default"
                >
                  Add Column
                </Button>
              </div>
            )}
          </form.Field>
        </div>

        <SheetFooter>
          <form.Subscribe
            selector={(state) => ({
              canSubmit: state.canSubmit,
              isSubmitting: state.isSubmitting,
            })}
          >
            {(state) => {
              return (
                <div class="flex items-center gap-4">
                  <Dialog
                    open={sql() !== undefined}
                    onOpenChange={(open: boolean) => {
                      if (!open) {
                        setSql(undefined);
                      }
                    }}
                  >
                    <DialogTrigger>
                      <Button
                        class="w-[92px]"
                        disabled={!state().canSubmit}
                        variant="outline"
                        onClick={() => {
                          onSubmit(form.state.values, /*dryRun=*/ true).catch(
                            console.error,
                          );
                        }}
                        {...props}
                      >
                        {state().isSubmitting ? "..." : "Dry Run"}
                      </Button>
                    </DialogTrigger>

                    <DialogContent class="min-w-[80dvw]">
                      <DialogHeader>
                        <DialogTitle>SQL</DialogTitle>
                      </DialogHeader>

                      <div class="overflow-auto">
                        <pre>{sql() === "" ? "<EMPTY>" : sql()}</pre>
                      </div>

                      <DialogFooter />
                    </DialogContent>
                  </Dialog>

                  <div class="mr-4 flex w-full justify-end">
                    <Button
                      type="submit"
                      disabled={!state().canSubmit}
                      variant="default"
                    >
                      {state().isSubmitting ? "..." : "Submit"}
                    </Button>
                  </div>
                </div>
              );
            }}
          </form.Subscribe>
        </SheetFooter>
      </form>
    </SheetContainer>
  );
}

function TextLabel(props: { text: string }) {
  return <div class="w-[100px]">{props.text}</div>;
}
