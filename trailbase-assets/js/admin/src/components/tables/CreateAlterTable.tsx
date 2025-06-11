import { createMemo, createSignal, Index, Switch, Match } from "solid-js";
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
  primaryKeyPresets,
} from "@/components/tables/CreateAlterColumnForm";
import { invalidateConfig } from "@/lib/config";

import type { Column } from "@bindings/Column";
import type { Table } from "@bindings/Table";
import { QualifiedName } from "@bindings/QualifiedName";

function newDefaultColumn(index: number): Column {
  return {
    name: `new_${index}`,
    data_type: "Text",
    options: [{ Default: "''" }],
  };
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

  const original = createMemo(() =>
    props.schema ? JSON.parse(JSON.stringify(props.schema)) : undefined,
  );

  const onSubmit = async (value: Table, dryRun: boolean) => {
    /* eslint-disable solid/reactivity */
    console.debug("Table schema:", value);

    try {
      const o = original();
      if (o) {
        const response = await alterTable({
          source_schema: o,
          target_schema: value,
        });
        console.debug("AlterTableResponse:", response);
      } else {
        const response = await createTable({ schema: value, dry_run: dryRun });
        console.debug(`CreateTableResponse [dry: ${dryRun}]:`, response);

        if (dryRun) {
          setSql(response.sql);
        }
      }

      if (!dryRun) {
        invalidateConfig(queryClient);
        props.schemaRefetch().then(() => {
          props.setSelected(value.name);
        });
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
    defaultValues:
      props.schema ??
      ({
        name: {
          name: randomName(),
          database_schema: null,
        },
        strict: true,
        indexes: [],
        columns: [
          {
            name: "id",
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
    onSubmit: async ({ value }) => await onSubmit(value, false),
  }));

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  return (
    <SheetContainer>
      <SheetHeader>
        <SheetTitle>{original() ? "Alter Table" : "Add New Table"}</SheetTitle>
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

          <form.Field
            name="strict"
            children={buildBoolFormField({ label: () => "STRICT (type-safe)" })}
          />

          {/* columns */}
          <h2>Columns</h2>

          <form.Field name="columns">
            {(field) => (
              <div class="w-full">
                <div class="flex flex-col gap-2">
                  <Index each={field().state.value}>
                    {(c: Accessor<Column>, i: number) => (
                      <Switch>
                        <Match when={i === 0}>
                          <PrimaryKeyColumnSubForm
                            form={form}
                            colIndex={i}
                            column={c()}
                            allTables={props.allTables}
                            disabled={original() !== undefined}
                          />
                        </Match>

                        <Match when={i !== 0}>
                          <ColumnSubForm
                            form={form}
                            colIndex={i}
                            column={c()}
                            allTables={props.allTables}
                            disabled={false}
                          />
                        </Match>
                      </Switch>
                    )}
                  </Index>
                </div>

                <Button
                  class="m-2"
                  onClick={() => {
                    const length = field().state.value.length;
                    field().pushValue(newDefaultColumn(length));
                  }}
                  variant="default"
                >
                  Add column
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
                  {original() === undefined && (
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
                            onSubmit(form.state.values, true).catch(
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
                          <pre>{sql()}</pre>
                        </div>

                        <DialogFooter />
                      </DialogContent>
                    </Dialog>
                  )}

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
