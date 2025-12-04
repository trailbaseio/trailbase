import { createSignal, Index, Match, Show, Switch } from "solid-js";
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

import { createTable, alterTable } from "@/lib/api/table";
import { generateRandomName } from "@/lib/name";
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
import { invalidateConfig } from "@/lib/api/config";

import type { Column } from "@bindings/Column";
import type { Table } from "@bindings/Table";
import type { AlterTableOperation } from "@bindings/AlterTableOperation";
import type { QualifiedName } from "@bindings/QualifiedName";
import { equalQualifiedNames, prettyFormatQualifiedName } from "@/lib/schema";

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

  const isCreateTable = () => props.schema === undefined;

  // Columns are treated as append only. Instead of removing it and inducing animation junk and other stuff when
  // shifting offset, we simply don't render columns that were marked as deleted.
  const [deletedColumns, setDeletedColumn] = createSignal<number[]>([]);
  const isDeleted = (i: number): boolean =>
    deletedColumns().findIndex((idx) => idx === i) !== -1;

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
      const original = props.schema;
      if (original !== undefined) {
        // Alter table

        const response = await alterTable({
          source_schema: original,
          operations: buildAlterTableOperations(
            original,
            value,
            deletedColumns(),
          ),
          dry_run: dryRun,
        });
        console.debug(`AlterTableResponse [dry: ${dryRun}]:`, response);

        if (dryRun) {
          setSql(response.sql);
        }
      } else {
        // Create table

        // Remove ephemeral/deleted columns, i.e. columns that were briefly added and then removed again.
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
    defaultValues: copySchema(props.schema) ?? defaultSchema(props.allTables),
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

      {/* NOTE: we set the tabindex to 0 to avoid mobile phones bringing up the on-screen keyboard on table name. */}
      <form
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();
          form.handleSubmit();
        }}
        tabindex={0}
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
          <form.Field name="columns">
            {(field) => (
              <div class="flex w-full flex-col gap-2 pb-2">
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

                <div>
                  <Button
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

                      <div class="max-h-[70vh] w-full overflow-auto">
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

function defaultSchema(allTables: Table[]): Table {
  return {
    name: {
      name: generateRandomName({
        taken: allTables.map((t) => t.name.name),
      }),
      // Use "main" db by default.
      database_schema: null,
    },
    strict: true,
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
  };
}

function copySchema(schema: Table | undefined): Table | undefined {
  return schema ? JSON.parse(JSON.stringify(schema)) : undefined;
}

function columnsEqual(a: Column, b: Column): boolean {
  return (
    a.name === b.name &&
    a.data_type === b.data_type &&
    JSON.stringify(a.options) === JSON.stringify(b.options)
  );
}

/// Builds alter table operations. Remember columns are append-only, i.e. there's a 1:1 mapping by index for pre-existing columns.
function buildAlterTableOperations(
  original: Table,
  target: Table,
  deletedColumns: number[],
): AlterTableOperation[] {
  const isDeleted = (i: number): boolean =>
    deletedColumns.findIndex((idx) => idx === i) !== -1;

  const operations: AlterTableOperation[] = [];
  if (!equalQualifiedNames(original.name, target.name)) {
    operations.push({
      RenameTableTo: {
        name: prettyFormatQualifiedName(target.name),
      },
    });
  }

  target.columns.forEach((column, i) => {
    if (i < original.columns.length) {
      // Pre-existing column.
      const originalName = original.columns[i].name;
      if (isDeleted(i)) {
        operations.push({ DropColumn: { name: originalName } });
        return;
      }

      if (!columnsEqual(original.columns[i], column)) {
        operations.push({
          AlterColumn: {
            name: originalName,
            column: column,
          },
        });
        return;
      }

      // Otherwise they're equal and there's nothing to do.
    } else {
      // Newly added columns.
      if (isDeleted(i)) {
        // New column has already been deleted, e.g. a user added and removed it.
        return;
      }

      operations.push({ AddColumn: { column: column } });
    }
  });

  return operations;
}

function TextLabel(props: { text: string }) {
  return <div class="w-[100px]">{props.text}</div>;
}
