import { createMemo, createSignal, For, Match, Show, Switch } from "solid-js";
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

import {
  buildBoolFormField,
  buildTextFormField,
  SelectOneOf,
} from "@/components/FormFields";
import { SheetContainer } from "@/components/SafeSheet";
import {
  PrimaryKeyColumnSubForm,
  ColumnSubForm,
  newDefaultColumn,
  primaryKeyPresets,
} from "@/components/tables/CreateAlterColumnForm";

import { createTable, alterTable } from "@/lib/api/table";
import { generateRandomName } from "@/lib/name";
import { createConfigQuery } from "@/lib/api/config";
import { invalidateConfig } from "@/lib/api/config";

import type { Column } from "@bindings/Column";
import type { Table } from "@bindings/Table";
import type { AlterTableOperation } from "@bindings/AlterTableOperation";
import type { QualifiedName } from "@bindings/QualifiedName";
import { equalQualifiedNames, prettyFormatQualifiedName } from "@/lib/schema";
import { createWritableMemo } from "@solid-primitives/memo";

export function CreateAlterTableForm(props: {
  close: () => void;
  markDirty: () => void;
  schemaRefetch: () => Promise<void>;
  allTables: Table[];
  setSelected: (tableName: QualifiedName) => void;
  schema?: Table;
}) {
  const queryClient = useQueryClient();
  const [dryRunDialog, setDryRunDialog] = createSignal<string | undefined>();

  const isCreateTable = () => props.schema === undefined;

  const config = createConfigQuery();
  const dbSchemas = (): string[] => [
    "main",
    ...(config.data?.config?.databases
      .map((db) => db.name)
      .filter((n) => n !== undefined) ?? []),
  ];

  const defaultValues = createMemo(() => {
    return props.schema !== undefined
      ? deepCopySchema(props.schema)
      : defaultSchema(props.allTables);
  });
  // Columns are treated as append only. Instead of removing actually removing a
  // column and inducing animation junk and other complications we simply don't
  // render columns that were marked as deleted.
  // const [deletedColumns, setDeletedColumns] = createSignal<Set<Column>>(new Set());
  const [order, setOrder] = createWritableMemo(() => {
    return defaultValues().columns.map((_, i) => i);
  });

  const onSubmit = async (value: Table, dryRun: boolean) => {
    // Assert that the type representations match up.
    for (const c of value.columns) {
      if (c.data_type.toUpperCase() != c.type_name.toUpperCase()) {
        throw new Error(`Got ${c.type_name}, expected, ${c.data_type}`);
      }
    }

    try {
      const original = props.schema;
      if (original !== undefined) {
        // Alter table
        const response = await alterTable({
          source_schema: original,
          operations: buildAlterTableOperations(original, value, order()),
          dry_run: dryRun,
        });
        console.debug(`AlterTableResponse [dry: ${dryRun}]:`, response);

        if (dryRun) {
          // Opens dialog.
          setDryRunDialog(response.sql);
          return;
        }
      } else {
        // Create table

        // Remove ephemeral/deleted columns, i.e. columns that were briefly added but then removed again.
        // value.columns = value.columns.filter((c) => !isDeleted(c));

        const response = await createTable({ schema: value, dry_run: dryRun });
        console.debug(`CreateTableResponse [dry: ${dryRun}]:`, response);

        if (dryRun) {
          // Opens dialog.
          setDryRunDialog(response.sql);
          return;
        }
      }

      console.assert(!dryRun, "unexpected dry run");

      // Trigger config reload
      invalidateConfig(queryClient);

      // Reload schemas and switch to new/altered table.
      // eslint-disable-next-line solid/reactivity
      props.schemaRefetch().then(() => {
        props.setSelected(value.name);
      });

      // Close dialog/sheet.
      props.close();
    } catch (err) {
      showToast({
        title: `${isCreateTable() ? "Creation" : "Alteration"} Error`,
        description: `${err}`,
        variant: "error",
      });
    }
  };

  const form = createForm(() => ({
    onSubmit: async ({ value }) => await onSubmit(value, /*dryRun=*/ false),
    defaultValues: defaultValues(),
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
          <Show when={isCreateTable() && dbSchemas().length > 1}>
            <form.Field name="name.database_schema">
              {(field) => (
                <SelectOneOf<string>
                  value={field().state.value ?? "main"}
                  label={() => <TextLabel text="Database" />}
                  options={dbSchemas()}
                  onChange={(schema: string) => {
                    if (schema === "main") {
                      field().handleChange(null);
                    } else {
                      field().handleChange(schema);
                    }
                  }}
                  handleBlur={() => field().handleBlur()}
                />
              )}
            </form.Field>
          </Show>

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
            {(field) => {
              const columns = createMemo(() =>
                filterAndOrderColumns(order(), field().state.value),
              );

              return (
                <div class="flex w-full flex-col gap-2 pb-2">
                  {/* Needs for be a "For" as opposed to an "Index" because order and length may change */}
                  <For each={columns()}>
                    {(el: [Column, number], i: () => number) => {
                      const [_column, origIndex] = el;

                      /* eslint-disable solid/reactivity */
                      const isFirst = () => i() <= 1;
                      const onMoveUp = isFirst()
                        ? undefined
                        : () => {
                            setOrder((old) => {
                              const index = i();
                              const next = [...old];
                              const tmp = next[index];
                              next[index] = next[index - 1];
                              next[index - 1] = tmp;
                              return next;
                            });
                            props.markDirty();
                          };

                      const isLast = () => i() >= columns().length - 1;
                      const onMoveDown = isLast()
                        ? undefined
                        : () => {
                            setOrder((old) => {
                              const index = i();
                              const next = [...old];
                              const tmp = next[index];
                              next[index] = next[index + 1];
                              next[index + 1] = tmp;
                              return next;
                            });
                            props.markDirty();
                          };

                      const onDelete = () => {
                        form.setFieldValue(
                          `columns[${origIndex}]`,
                          DELETED_COLUMN_MARKER,
                        );
                        setOrder((old) => old.toSpliced(i(), 1));
                      };

                      return (
                        <Switch>
                          <Match when={i() === 0}>
                            <PrimaryKeyColumnSubForm
                              form={form}
                              colIndex={origIndex}
                              allTables={props.allTables}
                              disabled={!isCreateTable()}
                            />
                          </Match>

                          <Match when={i() > 0}>
                            <ColumnSubForm
                              form={form}
                              colIndex={origIndex}
                              allTables={props.allTables}
                              disabled={false}
                              onDelete={onDelete}
                              onMoveUp={onMoveUp}
                              onMoveDown={onMoveDown}
                            />
                          </Match>
                        </Switch>
                      );
                    }}
                  </For>

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
                        setOrder((old) => [...old, old.length]);
                      }}
                      variant="default"
                    >
                      Add Column
                    </Button>
                  </div>
                </div>
              );
            }}
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
                    open={dryRunDialog() !== undefined}
                    onOpenChange={(open: boolean) => {
                      if (!open) {
                        setDryRunDialog(undefined);
                      }
                    }}
                  >
                    <DialogTrigger>
                      <Button
                        class="w-[92px]"
                        disabled={!state().canSubmit}
                        variant="outline"
                        onClick={() =>
                          onSubmit(form.state.values, /*dryRun=*/ true)
                        }
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
                        <pre>
                          {dryRunDialog() === "" ? "<EMPTY>" : dryRunDialog()}
                        </pre>
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
    ],
    // Table constraints: https://www.sqlite.org/syntax/table-constraint.html
    unique: [],
    foreign_keys: [],
    checks: [],
    virtual_table: false,
    temporary: false,
  };
}

function deepCopySchema(schema: Table): Table {
  return JSON.parse(JSON.stringify(schema));
}

function columnsEqual(a: Column, b: Column): boolean {
  return (
    a.name === b.name &&
    a.data_type === b.data_type &&
    JSON.stringify(a.options.toSorted()) ===
      JSON.stringify(b.options.toSorted())
  );
}

/// Builds alter table operations. Remember columns are append-only, i.e. there's a 1:1 mapping by index for pre-existing columns.
function buildAlterTableOperations(
  original: Table,
  target: Table,
  order: number[],
): AlterTableOperation[] {
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
      if (isDeleted(column)) {
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
      if (isDeleted(column)) {
        // New column has already been deleted, e.g. a user added and removed it.
        return;
      }

      operations.push({ AddColumn: { column: column } });
    }
  });

  const columsReordered = order.reduce((acc: boolean, curr, index): boolean => {
    if (index == 0) return false;
    const prev = order[index - 1];
    return acc || curr < prev;
  }, false);

  if (columsReordered) {
    const orderedColumnNames = filterAndOrderColumns(order, target.columns).map(
      ([col, _]) => col.name,
    );
    operations.push({ ChangeColumnOrder: { names: orderedColumnNames } });
  }

  return operations;
}

function TextLabel(props: { text: string }) {
  return <div class="w-[100px]">{props.text}</div>;
}

function isDeleted(c: Column) {
  return c === DELETED_COLUMN_MARKER;
}

const filterAndOrderColumns = (
  order: number[],
  columns: Column[],
): [Column, number][] => {
  return order.map((i) => [columns[i], i]);
};

const DELETED_COLUMN_MARKER: Column = {
  name: "<deleted>",
  type_name: "ANY",
  data_type: "Any",
  affinity_type: "Blob",
  options: [],
};
