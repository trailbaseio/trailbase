import { createSignal, createMemo, Index } from "solid-js";
import type { Accessor } from "solid-js";
import { createForm } from "@tanstack/solid-form";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import { showToast } from "@/components/ui/toast";

import { alterIndex, createIndex } from "@/lib/table";
import {
  buildTextFormField,
  buildBoolFormField,
  buildSelectField,
} from "@/components/FormFields";
import { SheetContainer } from "@/components/SafeSheet";
import { prettyFormatQualifiedName } from "@/lib/schema";

import type { ColumnOrder } from "@bindings/ColumnOrder";
import type { Table } from "@bindings/Table";
import type { TableIndex } from "@bindings/TableIndex";

export function CreateAlterIndexForm(props: {
  close: () => void;
  markDirty: () => void;
  schemaRefetch: () => void;
  table: Table;
  schema?: TableIndex;
}) {
  const [sql, setSql] = createSignal<string | undefined>();

  const original = createMemo(() =>
    props.schema ? JSON.parse(JSON.stringify(props.schema)) : undefined,
  );
  const newDefaultColumn = (index: number): ColumnOrder => {
    return {
      column_name: props.table.columns[index].name,
      // Ascending is sqlite's default.
      ascending: false,
      // Sqlite doesn't support nulls_first, i.e. this parameter must be "null".
      nulls_first: null,
    };
  };

  const onSubmit = async (value: TableIndex, dryRun: boolean) => {
    console.debug("Index schema:", value);

    try {
      const o = original();
      if (o) {
        const response = await alterIndex({
          source_schema: o,
          target_schema: value,
        });
        console.debug("AlterIndexResponse:", response);
      } else {
        const response = await createIndex({ schema: value, dry_run: dryRun });
        console.debug(`CreateIndexResponse [dry: ${dryRun}]:`, response);

        if (dryRun) {
          setSql(response.sql);
        }
      }

      if (!dryRun) {
        props.schemaRefetch();
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

  const form = createForm(() => {
    const columns: ColumnOrder[] = [newDefaultColumn(0)];

    return {
      defaultValues:
        props.schema ??
        ({
          name: {
            name: `_${props.table.name.name}__${columns[0].column_name}_index`,
            database_schema: props.table.name.database_schema,
          },
          table_name: props.table.name.name,
          columns,
          unique: false,
          predicate: null,
        } as TableIndex),
      onSubmit: async ({ value }) => await onSubmit(value, false),
    };
  });

  form.useStore((state) => {
    if (state.isDirty && !state.isSubmitted) {
      props.markDirty();
    }
  });

  return (
    <SheetContainer>
      <SheetHeader>
        <SheetTitle>
          {original() ? "Alter Index" : "Add Index"} for{" "}
          <span class="font-mono">
            {prettyFormatQualifiedName(props.table.name)}
          </span>
        </SheetTitle>
      </SheetHeader>

      <form
        method="dialog"
        onSubmit={(e: SubmitEvent) => {
          e.preventDefault();
          form.handleSubmit();
        }}
      >
        <div class="flex flex-col items-start gap-4 pr-4">
          <form.Field
            name="name.name"
            validators={{
              onChange: ({ value }: { value: string | undefined }) => {
                return value ? undefined : "Table name missing";
              },
            }}
          >
            {buildTextFormField({ label: () => "Index Name" })}
          </form.Field>

          {/* columns */}
          <form.Field name="columns">
            {(field) => (
              <div class="w-full">
                <div class="flex flex-col gap-2">
                  <Index each={field().state.value}>
                    {(_c: Accessor<ColumnOrder>, i) => (
                      <Card>
                        {/* <CardHeader>Index Column {i}</CardHeader> */}

                        <CardContent>
                          <div class="mt-8 flex w-full flex-col gap-4">
                            <form.Field name={`columns[${i}].column_name`}>
                              {buildSelectField(
                                [...props.table.columns.map((c) => c.name)],
                                {
                                  label: () => (
                                    <div class={labelWidth}>Column Name</div>
                                  ),
                                },
                              )}
                            </form.Field>

                            <form.Field name={`columns[${i}].ascending`}>
                              {buildBoolFormField({
                                label: () => <div>Ascending</div>,
                              })}
                            </form.Field>

                            <form.Field name={`columns[${i}].nulls_first`}>
                              {buildBoolFormField({
                                label: () => <div>Nulls First</div>,
                              })}
                            </form.Field>
                          </div>
                        </CardContent>
                      </Card>
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
                        <div>
                          <Button
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
                        </div>
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

                  <div>
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

const labelWidth = "w-[112px]";
