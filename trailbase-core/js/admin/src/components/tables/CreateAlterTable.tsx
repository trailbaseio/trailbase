import { createEffect, createMemo, createSignal, Index, For } from "solid-js";
import type { Accessor, JSX, JSXElement, Setter } from "solid-js";
import { createForm } from "@tanstack/solid-form";
import { Collapsible } from "@kobalte/core/collapsible";
import { TbChevronDown, TbTrash, TbInfoCircle } from "solid-icons/tb";

import { showToast } from "@/components/ui/toast";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { SheetHeader, SheetTitle, SheetFooter } from "@/components/ui/sheet";
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";

import {
  isNotNull,
  setNotNull,
  getCheckValue,
  setCheckValue,
  getDefaultValue,
  setDefaultValue,
  getUnique,
  setUnique,
  getForeignKey,
  setForeignKey,
} from "@/lib/schema";
import { createTable, alterTable } from "@/lib/table";
import { cn } from "@/lib/utils";
import { randomName } from "@/lib/name";
import type {
  Column,
  ColumnDataType,
  ColumnOption,
  Table,
} from "@/lib/bindings";
import {
  buildBoolFormField,
  gapStyle,
  buildSelectField,
  buildTextFormField,
} from "@/components/FormFields";
import type { FormType, AnyFieldApi } from "@/components/FormFields";
import { SheetContainer } from "@/components/SafeSheet";
import { invalidateConfig } from "@/lib/config";

export function CreateAlterTableForm(props: {
  close: () => void;
  markDirty: () => void;
  schemaRefetch: () => Promise<void>;
  allTables: Table[];
  setSelected: (tableName: string) => void;
  schema?: Table;
}) {
  const [sql, setSql] = createSignal<string | undefined>();

  const original = createMemo(() =>
    props.schema ? JSON.parse(JSON.stringify(props.schema)) : undefined,
  );
  const newDefaultColumn = (index: number): Column => {
    return {
      name: `new_${index}`,
      data_type: "Text",
      options: [{ Default: "''" }],
    };
  };

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
        invalidateConfig();
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

  const form = createForm<Table>(() => ({
    defaultValues: props.schema ?? {
      name: randomName(),
      strict: true,
      indexes: [],
      columns: [
        {
          name: "id",
          data_type: "Blob",
          // Column constraints: https://www.sqlite.org/syntax/column-constraint.html
          options: [
            { Unique: { is_primary: true } },
            { Check: "is_uuid_v7(id)" },
            { Default: "(uuid_v7())" },
            "NotNull",
          ],
        },
        newDefaultColumn(1),
      ] satisfies Column[],
      // Table constraints: https://www.sqlite.org/syntax/table-constraint.html
      unique: [],
      foreign_keys: [],
      virtual_table: false,
      temporary: false,
    },
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
        onSubmit={(e) => {
          e.preventDefault();
          e.stopPropagation();
          form.handleSubmit();
        }}
      >
        <div class="mt-4 flex flex-col items-start gap-4 pr-4">
          <form.Field
            name="name"
            validators={{
              onChange: ({ value }: { value: string | undefined }) => {
                return value ? undefined : "Table name missing";
              },
            }}
          >
            {buildTextFormField({ label: () => <L>Table name</L> })}
          </form.Field>

          <form.Field
            name="strict"
            children={buildBoolFormField({ label: () => "STRICT Typing" })}
          />

          {/* columns */}
          <h2>Columns</h2>

          <form.Field name="columns">
            {(field) => (
              <div class="w-full">
                <div class="flex flex-col gap-2">
                  <Index each={field().state.value}>
                    {(c: Accessor<Column>, i) => (
                      <ColumnSubForm
                        form={form}
                        colIndex={i}
                        column={c()}
                        allTables={props.allTables}
                        disabled={i === 0}
                      />
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

function columnTypeField(
  disabled: boolean,
  fk: Accessor<string | undefined>,
  allTables: Table[],
) {
  // WARNING: these needs to be kept in sync with ColumnDataType. TS cannot go
  // from type union to array.
  const columnDataTypes: ColumnDataType[] = [
    "Blob",
    "Text",
    "Integer",
    "Real",
    "Null",
  ] as const;

  return (field: () => AnyFieldApi) => {
    const [isDisabled, setDisabled] = createSignal(disabled);

    createEffect(() => {
      const foreignKey = fk();
      if (foreignKey) {
        for (const table of allTables) {
          if (table.name == foreignKey) {
            const type = table.columns[0].data_type;
            console.debug(type, field().state.value);
            if (field().state.value === type) {
              break;
            }

            field().setValue(type);
            break;
          }
        }
      }

      setDisabled(foreignKey !== undefined ? true : disabled);
    });

    return buildSelectField([...columnDataTypes], {
      label: () => <L>Type</L>,
      disabled: isDisabled(),
    })(field);
  };
}

function ColumnOptionCheckField(props: {
  column: Column;
  value: ColumnOption[];
  onChange: (v: ColumnOption[]) => void;
  disabled: boolean;
}) {
  const disabled = () =>
    props.disabled || getCheckValue(props.value) === undefined;

  const HCard = () => (
    <HoverCard>
      <HoverCardTrigger
        class="size-[32px]"
        as={Button<"button">}
        variant="link"
      >
        <TbInfoCircle />
      </HoverCardTrigger>

      <HoverCardContent class="w-80">
        <div class="flex justify-between space-x-4">
          <div class="space-y-1">
            <h4 class="text-sm font-semibold">Column Constraint</h4>

            <p class="text-sm">
              Can be any boolean expression constant like{" "}
              <span class="font-mono font-bold">{`${props.column.name} < 42 `}</span>
              including SQL function calls like{" "}
              <span class="font-mono font-bold">
                is_email({props.column.name})
              </span>
              .
            </p>
          </div>
        </div>
      </HoverCardContent>
    </HoverCard>
  );

  return (
    <TextField>
      <div
        class={`grid items-center ${gapStyle}`}
        style={{ "grid-template-columns": "auto 1fr" }}
      >
        <L>
          <TextFieldLabel
            class={cn(
              "flex items-center text-right",
              disabled() ? "text-muted-foreground" : null,
            )}
          >
            <HCard />
            Check
          </TextFieldLabel>
        </L>

        <div class="flex items-center">
          <TextFieldInput
            disabled={disabled()}
            type="text"
            value={getCheckValue(props.value) ?? ""}
            onChange={(e: Event) => {
              const value: string | undefined = (
                e.currentTarget as HTMLInputElement
              ).value;
              props.onChange(setCheckValue(props.value, value));
            }}
          />

          <Checkbox
            disabled={props.disabled}
            checked={getCheckValue(props.value) !== undefined}
            onChange={(value) => {
              const newOpts = setCheckValue(
                props.value,
                value ? "" : undefined,
              );
              props.onChange(newOpts);
            }}
          />
        </div>
      </div>
    </TextField>
  );
}

function ColumnOptionDefaultField(props: {
  column: Column;
  value: ColumnOption[];
  onChange: (v: ColumnOption[]) => void;
  disabled: boolean;
}) {
  const disabled = () =>
    props.disabled || getDefaultValue(props.value) === undefined;

  const HCard = () => (
    <HoverCard>
      <HoverCardTrigger
        class="size-[32px]"
        as={Button<"button">}
        variant="link"
      >
        <TbInfoCircle />
      </HoverCardTrigger>

      <HoverCardContent class="w-80">
        <div class="flex justify-between space-x-4">
          <div class="space-y-1">
            <h4 class="text-sm font-semibold">Column Default Value</h4>

            <p class="text-sm">
              Can either be a constant like{" "}
              <span class="font-mono font-bold">'foo'</span>
              and <span class="font-mono font-bold">42</span>, or a scalar
              function like
              <span class="font-mono font-bold">
                (jsonschema('std.FileUpload', {props.column.name}))
              </span>
              .
            </p>
          </div>
        </div>
      </HoverCardContent>
    </HoverCard>
  );

  // TODO: Factor out inner component from buildTextFormField and use it here.
  return (
    <TextField>
      <div
        class={`grid items-center ${gapStyle}`}
        style={{ "grid-template-columns": "auto 1fr" }}
      >
        <L>
          <TextFieldLabel
            class={cn(
              "flex items-center text-right",
              disabled() ? "text-muted-foreground" : null,
            )}
          >
            <HCard />
            Default
          </TextFieldLabel>
        </L>

        <div class="flex items-center">
          <TextFieldInput
            disabled={disabled()}
            type="text"
            value={getDefaultValue(props.value) ?? ""}
            onChange={(e: Event) => {
              const value: string | undefined = (
                e.currentTarget as HTMLInputElement
              ).value;
              props.onChange(setDefaultValue(props.value, value));
            }}
          />

          <Checkbox
            disabled={props.disabled}
            checked={getDefaultValue(props.value) !== undefined}
            onChange={(value) => {
              // TODO: Make default dependent on column type.
              const newOpts = setDefaultValue(
                props.value,
                value ? "''" : undefined,
              );
              props.onChange(newOpts);
            }}
          />
        </div>
      </div>
    </TextField>
  );
}

function ColumnOptionFkSelect(props: {
  value: ColumnOption[];
  onChange: (v: ColumnOption[]) => void;
  allTables: Table[];
  disabled: boolean;
  setFk: Setter<undefined | string>;
}) {
  const fkTableOptions: string[] = [
    "None",
    ...props.allTables.map((schema) => schema.name),
  ];
  const fkValue = (): string =>
    getForeignKey(props.value)?.foreign_table ?? "None";

  return (
    <div
      class="grid w-full items-center gap-2"
      style={{ "grid-template-columns": "auto 1fr" }}
    >
      <Label>
        <L>Foreign Key</L>
      </Label>

      <Select
        multiple={false}
        value={fkValue()}
        options={fkTableOptions}
        onChange={(table: string | null) => {
          if (!table || table === "None") {
            props.setFk(undefined);
            props.onChange(setForeignKey(props.value, undefined));
          } else {
            const schema = props.allTables.find(
              (schema) => schema.name == table,
            )!;
            const column =
              schema.columns.find(
                (col) => getUnique(col.options)?.is_primary ?? false,
              ) ?? schema.columns[0];

            props.setFk(table);
            props.onChange(
              setForeignKey(props.value, {
                foreign_table: table,
                referred_columns: [column.name],
                on_delete: null,
                on_update: null,
              }),
            );
          }
        }}
        itemComponent={(props) => (
          <SelectItem item={props.item}>{props.item.rawValue}</SelectItem>
        )}
        disabled={props.disabled}
      >
        <SelectTrigger>
          <SelectValue<string>>{(state) => state.selectedOption()}</SelectValue>
        </SelectTrigger>

        <SelectContent />
      </Select>
    </div>
  );
}

function ColumnOptionsFields(props: {
  column: Column;
  value: ColumnOption[];
  onChange: (v: ColumnOption[]) => void;
  allTables: Table[];
  disabled: boolean;
  setFk: Setter<undefined | string>;
}) {
  // Column options: (not|null), (default), (unique), (fk), (check), (comment), (onupdate).

  return (
    <>
      <ColumnOptionFkSelect {...props} />

      <ColumnOptionDefaultField
        column={props.column}
        value={props.value}
        onChange={props.onChange}
        disabled={props.disabled}
      />

      <ColumnOptionCheckField
        column={props.column}
        value={props.value}
        onChange={props.onChange}
        disabled={props.disabled}
      />

      <div class="my-2 flex flex-col gap-4">
        <div class="flex justify-end">
          <Label class="text-right text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
            NOT NULL
          </Label>

          <div class="flex justify-end">
            <Checkbox
              disabled={props.disabled}
              checked={isNotNull(props.value)}
              onChange={(value) =>
                props.onChange(setNotNull(props.value, value))
              }
            />
          </div>
        </div>

        <div class="flex justify-end">
          <Label class="text-right text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
            UNIQUE {getUnique(props.value)?.is_primary && "(PRIMARY KEY)"}
          </Label>

          <div class="flex justify-end">
            <Checkbox
              disabled={props.disabled}
              checked={getUnique(props.value) !== undefined}
              onChange={(value: boolean) => {
                props.onChange(
                  setUnique(
                    props.value,
                    value ? { is_primary: false } : undefined,
                  ),
                );
              }}
            />
          </div>
        </div>
      </div>
    </>
  );
}

function ColumnSubForm(props: {
  form: FormType<Table>;
  colIndex: number;
  column: Column;
  allTables: Table[];
  disabled: boolean;
}): JSX.Element {
  const disabled = props.disabled;
  const [name, setName] = createSignal(props.column.name);
  const [expanded, setExpanded] = createSignal(props.column.name !== "id");

  const [fk, setFk] = createSignal<string | undefined>();

  const Header = () => (
    <div class="flex items-center justify-between">
      <h2>{name()}</h2>

      <div class="flex items-center gap-2">
        {!disabled && (
          <div class="flex justify-end">
            <button
              class="my-2"
              onClick={() => {
                // Delete this column from list of all columns.
                const columns = [...props.form.state.values.columns];
                columns.splice(props.colIndex, 1);
                props.form.setFieldValue("columns", columns);
              }}
            >
              <div class="rounded p-1 hover:bg-gray-200">
                <TbTrash size={20} />
              </div>
            </button>
          </div>
        )}
        <TbChevronDown
          size={20}
          style={{
            "justify-self": "center",
            "align-self": "center",
            transition: "rotate",
            rotate: expanded() ? "180deg" : "",
            "transition-duration": "300ms",
            "transition-timing-function": transitionTimingFunc,
          }}
        />
      </div>
    </div>
  );

  return (
    <Card>
      <Collapsible
        class="collapsible"
        open={expanded()}
        onOpenChange={setExpanded}
      >
        <CardHeader>
          <Collapsible.Trigger class="collapsible__trigger">
            <Header />
          </Collapsible.Trigger>
        </CardHeader>

        <Collapsible.Content class="collapsible__content">
          <CardContent>
            <div class="flex flex-col gap-2 py-1">
              {/* Column presets */}
              <div class="flex justify-between gap-1">
                <Label>Presets</Label>

                <div class="flex gap-1">
                  <For each={presets}>
                    {([name, preset]) => (
                      <Badge
                        class="p-1"
                        onClick={() => {
                          const columns = [...props.form.state.values.columns];
                          const column = columns[props.colIndex];

                          const v = preset(column.name);

                          column.data_type = v.data_type;
                          column.options = v.options;

                          props.form.setFieldValue("columns", columns);
                        }}
                      >
                        {name}
                      </Badge>
                    )}
                  </For>
                </div>
              </div>

              {/* Column name field */}
              <props.form.Field
                name={`columns[${props.colIndex}].name`}
                defaultValue={name()}
                validators={{
                  onChange: ({ value }: { value: string | undefined }) => {
                    setName(value ?? "<missing>");
                    return value ? undefined : "Column name missing";
                  },
                }}
              >
                {buildTextFormField({
                  label: () => <L>Name</L>,
                  disabled,
                })}
              </props.form.Field>

              {/* Column type field */}
              <props.form.Field
                name={`columns[${props.colIndex}].data_type`}
                children={columnTypeField(disabled, fk, props.allTables)}
              />

              {/* Column options: pk, not null, ... */}
              <props.form.Field
                name={`columns[${props.colIndex}].options`}
                children={(field) => {
                  return (
                    <ColumnOptionsFields
                      column={props.column}
                      value={field().state.value}
                      onChange={field().handleChange}
                      allTables={props.allTables}
                      disabled={disabled}
                      setFk={setFk}
                    />
                  );
                }}
              />
            </div>
          </CardContent>
        </Collapsible.Content>
      </Collapsible>
    </Card>
  );
}

function L(props: { children: JSXElement }) {
  return <div class="w-[100px]">{props.children}</div>;
}

const transitionTimingFunc = "cubic-bezier(.87,0,.13,1)";

type Preset = {
  data_type: ColumnDataType;
  options: ColumnOption[];
};

const presets: [string, (colName: string) => Preset][] = [
  [
    "Default",
    (_colName: string) => {
      return {
        data_type: "Text",
        options: [{ Default: "''" }, "NotNull"],
      };
    },
  ],
  [
    "UUIDv7",
    (colName: string) => {
      return {
        data_type: "Blob",
        options: [
          { Check: `is_uuid_v7(${colName})` },
          { Default: "(uuid_v7())" },
          "NotNull",
        ],
      };
    },
  ],
  [
    "JSON",
    (colName: string) => {
      return {
        data_type: "Text",
        options: [
          { Check: `is_json(${colName})` },
          { Default: "{}" },
          "NotNull",
        ],
      };
    },
  ],
  [
    "File",
    (colName: string) => {
      return {
        data_type: "Text",
        options: [
          {
            Check: `jsonschema('std.FileUpload', ${colName})`,
          },
        ],
      };
    },
  ],
  [
    "Files",
    (colName: string) => {
      return {
        data_type: "Text",
        options: [
          {
            Check: `jsonschema('std.FileUploads', ${colName})`,
          },
          { Default: "[]" },
          "NotNull",
        ],
      };
    },
  ],
];
