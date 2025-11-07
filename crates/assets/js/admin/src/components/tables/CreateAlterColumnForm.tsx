import { createEffect, createMemo, createSignal, For } from "solid-js";
import type { Accessor, JSX, Setter } from "solid-js";
import { Collapsible } from "@kobalte/core/collapsible";
import { TbChevronDown, TbTrash, TbInfoCircle } from "solid-icons/tb";
import { createWritableMemo } from "@solid-primitives/memo";

import { Badge, ButtonBadge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { IconButton } from "@/components/IconButton";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
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
import {
  TextField,
  TextFieldLabel,
  TextFieldInput,
} from "@/components/ui/text-field";
import {
  SelectOneOf,
  buildTextFormField,
  floatPattern,
  gapStyle,
} from "@/components/FormFields";
import type { FormApiT, AnyFieldApi } from "@/components/FormFields";

import {
  columnDataTypes,
  getCheckValue,
  getDefaultValue,
  getForeignKey,
  getUnique,
  isNotNull,
  setCheckValue,
  setDefaultValue,
  setForeignKey,
  setNotNull,
  setUnique,
} from "@/lib/schema";
import { cn } from "@/lib/utils";

import type { Column } from "@bindings/Column";
import type { ColumnDataType } from "@bindings/ColumnDataType";
import type { ColumnOption } from "@bindings/ColumnOption";
import type { Table } from "@bindings/Table";

export function newDefaultColumn(
  index: number,
  existingNames?: string[],
): Column {
  let name = `new_${index}`;
  if (existingNames !== undefined) {
    for (let i = 0; i < 1000; ++i) {
      if (existingNames.find((n) => n === name) === undefined) {
        break;
      }
      name = `new_${index + i}`;
    }
  }

  const [_, builder] = presets[0];
  return {
    ...builder(name),
    name,
  };
}

function columnTypeField(
  form: FormApiT<Table>,
  colIndex: number,
  disabled: boolean,
  fk: Accessor<string | undefined>,
  allTables: Table[],
) {
  return (field: () => AnyFieldApi) => {
    // Note: use createMemo to avoid rebuilds for any state change.
    const value = createMemo(() => field().state.value);

    createEffect(() => {
      const foreignKey = fk();

      if (foreignKey) {
        const v = value();

        for (const table of allTables) {
          const tableName = table.name.name;
          if (tableName === foreignKey) {
            const type = table.columns[0].data_type;
            console.debug(foreignKey, tableName, type, v);
            if (v === type) {
              break;
            }

            field().setValue(type);
            break;
          }
        }
      }
    });

    return (
      <SelectOneOf<ColumnDataType>
        label={() => <L>Type</L>}
        disabled={disabled || fk() !== undefined}
        options={columnDataTypes}
        value={field().state.value}
        onChange={(v: ColumnDataType) => {
          field().handleChange(v);

          // Ultimately, it's `type_name` that matters, not `data_type`, when
          // rendering the query from the schema structure. This is an
          // artifact of us reusing the parsed structure. Fix up the relevant
          // field (and affinity type just for consistency).
          patchColumn(form, colIndex, (col: Column): Column => {
            switch (v) {
              case "Any":
                return {
                  ...col,
                  type_name: "ANY",
                  affinity_type: "Blob",
                };
              case "Blob":
                return {
                  ...col,
                  type_name: "BLOB",
                  affinity_type: "Blob",
                };
              case "Text":
                return {
                  ...col,
                  type_name: "TEXT",
                  affinity_type: "Text",
                };
              case "Integer":
                return {
                  ...col,
                  type_name: "INTEGER",
                  affinity_type: "Integer",
                };
              case "Real":
                return {
                  ...col,
                  type_name: "REAL",
                  affinity_type: "Real",
                };
            }
          });
        }}
        handleBlur={field().handleBlur}
      />
    );
  };
}

function ColumnOptionCheckField(props: {
  column: Column;
  value: ColumnOption[];
  onChange: (v: ColumnOption[]) => void;
  disabled: boolean;
}) {
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
              props.disabled ? "text-muted-foreground" : null,
            )}
          >
            <HCard />
            Check
          </TextFieldLabel>
        </L>

        <TextFieldInput
          disabled={props.disabled}
          type="text"
          value={getCheckValue(props.value) ?? ""}
          onChange={(e: Event) => {
            const value: string | undefined = (
              e.currentTarget as HTMLInputElement
            ).value;

            props.onChange(
              setCheckValue(props.value, value === "" ? undefined : value),
            );
          }}
        />
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
              <span class="font-mono font-bold">'foo'</span>,{" "}
              <span class="font-mono font-bold">42</span>, and{" "}
              <span class="font-mono font-bold">X'face'</span>, or a scalar
              function like{" "}
              <span class="font-mono font-bold">(unixepoch())</span>.
            </p>
          </div>
        </div>
      </HoverCardContent>
    </HoverCard>
  );

  const validationPattern = () => {
    switch (props.column.data_type) {
      case "Any":
        return undefined;
      case "Blob":
        return "^X'.*'$";
      case "Text":
        return "^'.*'$";
      case "Integer":
        return undefined;
      case "Real":
        return floatPattern;
    }
  };

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
              props.disabled ? "text-muted-foreground" : null,
            )}
          >
            <HCard />
            Default
          </TextFieldLabel>
        </L>

        <TextFieldInput
          disabled={props.disabled}
          type={props.column.data_type === "Integer" ? "number" : "text"}
          pattern={validationPattern()}
          value={getDefaultValue(props.value) ?? ""}
          onChange={(e: Event) => {
            const value: string | undefined = (
              e.currentTarget as HTMLInputElement
            ).value;

            props.onChange(
              setDefaultValue(props.value, value === "" ? undefined : value),
            );
          }}
        />
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
  const fkTableOptions = createMemo((): string[] => [
    "None",
    ...props.allTables.map((schema) => schema.name.name),
  ]);
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
        options={fkTableOptions()}
        onChange={(table: string | null) => {
          if (!table || table === "None") {
            props.setFk(undefined);
            props.onChange(setForeignKey(props.value, undefined));
          } else {
            const schema = props.allTables.find(
              (schema) => schema.name.name == table,
            )!;
            const column =
              schema.columns.find(
                (col) => getUnique(col.options)?.is_primary ?? false,
              ) ?? schema.columns[0];

            props.setFk(table);

            let newColumnOptions = [...props.value];
            newColumnOptions = setForeignKey(props.value, {
              foreign_table: table,
              referred_columns: [column.name],
              on_delete: null,
              on_update: null,
            });
            newColumnOptions = setCheckValue(newColumnOptions, undefined);
            newColumnOptions = setDefaultValue(newColumnOptions, undefined);

            props.onChange(newColumnOptions);
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
  pk: boolean;
  fk: string | undefined;
  setFk: Setter<string | undefined>;
}) {
  // Column options: (not|null), (default), (unique), (fk), (check), (comment), (onupdate).
  return (
    <>
      {/* FOREIGN KEY constraint */}
      {!props.pk && <ColumnOptionFkSelect {...props} />}

      {/* DEFAULT constraint */}
      <ColumnOptionDefaultField
        column={props.column}
        value={props.value}
        onChange={props.onChange}
        disabled={props.disabled || props.fk !== undefined}
      />

      {/* CHECK constraint */}
      <ColumnOptionCheckField
        column={props.column}
        value={props.value}
        onChange={props.onChange}
        disabled={props.disabled || props.fk !== undefined}
      />

      {/* NOT NULL constraint */}
      <button
        type="button"
        class={customCheckBoxStyle}
        onClick={() => {
          const current = isNotNull(props.value);
          props.onChange(setNotNull(props.value, !current));
        }}
      >
        <Label class="text-right text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
          NOT NULL
        </Label>

        <Checkbox disabled={props.disabled} checked={isNotNull(props.value)} />
      </button>

      {/* UNIQUE (pk) constraint */}
      {!props.pk && (
        <button
          class={customCheckBoxStyle}
          type="button"
          onClick={() => {
            const current = getUnique(props.value) !== undefined;
            props.onChange(
              setUnique(
                props.value,
                !current
                  ? { is_primary: false, conflict_clause: null }
                  : undefined,
              ),
            );
          }}
        >
          <Label class="text-right text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
            UNIQUE {getUnique(props.value)?.is_primary && "(PRIMARY KEY)"}
          </Label>

          <Checkbox
            disabled={props.disabled}
            checked={getUnique(props.value) !== undefined}
          />
        </button>
      )}
    </>
  );
}

export function ColumnSubForm(props: {
  form: FormApiT<Table>;
  colIndex: number;
  column: Column;
  allTables: Table[];
  disabled: boolean;
  onDelete: () => void;
}): JSX.Element {
  const disabled = () => props.disabled;
  const [name, setName] = createWritableMemo(() => props.column.name);
  const [expanded, setExpanded] = createSignal(true);

  const [fk, setFk] = createSignal<string | undefined>();

  const Header = () => (
    <div class="flex items-center justify-between">
      <h3 class="truncate">{name()}</h3>

      <div class="flex items-center gap-2">
        {
          // Delete column button.
          !disabled() && (
            <div class="flex justify-end">
              <IconButton onClick={props.onDelete}>
                <TbTrash />
              </IconButton>
            </div>
          )
        }
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
              <div
                class={cn("grid items-center", gapStyle)}
                style={{ "grid-template-columns": "auto 1fr" }}
              >
                <L>Presets</L>

                <div class="flex flex-wrap gap-1">
                  <For each={presets}>
                    {([name, preset]) => (
                      <ButtonBadge
                        class="p-1 active:scale-90"
                        type="button"
                        onClick={() => {
                          patchColumn(props.form, props.colIndex, (col) =>
                            preset(col.name),
                          );
                        }}
                      >
                        {name}
                      </ButtonBadge>
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
                  disabled: disabled(),
                })}
              </props.form.Field>

              {/* Column type field */}
              <props.form.Field name={`columns[${props.colIndex}].data_type`}>
                {columnTypeField(
                  props.form,
                  props.colIndex,
                  disabled(),
                  fk,
                  props.allTables,
                )}
              </props.form.Field>

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
                      disabled={disabled()}
                      pk={false}
                      fk={fk()}
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

export function PrimaryKeyColumnSubForm(props: {
  form: FormApiT<Table>;
  colIndex: number;
  column: Column;
  allTables: Table[];
  disabled: boolean;
}): JSX.Element {
  const disabled = () => props.disabled;
  const [name, setName] = createWritableMemo(() => props.column.name);
  const [expanded, setExpanded] = createSignal(false);

  const [fk, setFk] = createSignal<string | undefined>();

  const Header = () => (
    <div class="flex items-center justify-between">
      <h3 class="truncate">{name()}</h3>

      <div class="flex items-center gap-2">
        <Badge class="p-1">Primary Key</Badge>

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
              {!disabled() && (
                <div
                  class={cn("grid items-center", gapStyle)}
                  style={{ "grid-template-columns": "auto 1fr" }}
                >
                  <L>Presets</L>

                  <div class="flex flex-wrap gap-1">
                    <For each={primaryKeyPresets}>
                      {([name, preset]) => (
                        <ButtonBadge
                          class="p-1 active:scale-90"
                          type="button"
                          onClick={() => {
                            patchColumn(props.form, props.colIndex, (col) =>
                              preset(col.name),
                            );
                          }}
                        >
                          {name}
                        </ButtonBadge>
                      )}
                    </For>
                  </div>
                </div>
              )}

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
                  disabled: disabled(),
                })}
              </props.form.Field>

              {/* Column type field */}
              <props.form.Field
                name={`columns[${props.colIndex}].data_type`}
                children={columnTypeField(
                  props.form,
                  props.colIndex,
                  /*disabled=*/ true,
                  fk,
                  props.allTables,
                )}
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
                      disabled={true}
                      pk={true}
                      fk={fk()}
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

function L(props: { children: JSX.Element }) {
  return <div class="w-[100px]">{props.children}</div>;
}

function patchColumn(
  form: FormApiT<Table>,
  colIndex: number,
  patch: (col: Column) => Column,
) {
  const columns = [...form.state.values.columns];

  const column = columns[colIndex];
  columns[colIndex] = patch(column);

  form.setFieldValue("columns", columns);
}

export const primaryKeyPresets: [string, (colName: string) => Column][] = [
  [
    "INTEGER",
    (colName: string) => {
      return {
        name: colName,
        type_name: "INTEGER",
        data_type: "Integer",
        affinity_type: "Integer",
        options: [
          { Unique: { is_primary: true, conflict_clause: null } },
          "NotNull",
        ],
      };
    },
  ],
  [
    "UUIDv4",
    (colName: string) => {
      return {
        name: colName,
        type_name: "BLOB",
        data_type: "Blob",
        affinity_type: "Blob",
        options: [
          { Unique: { is_primary: true, conflict_clause: null } },
          { Check: `is_uuid(${colName})` },
          { Default: "(uuid_v4())" },
          "NotNull",
        ],
      };
    },
  ],
  [
    "UUIDv7",
    (colName: string) => {
      return {
        name: colName,
        type_name: "BLOB",
        data_type: "Blob",
        affinity_type: "Blob",
        options: [
          { Unique: { is_primary: true, conflict_clause: null } },
          { Check: `is_uuid_v7(${colName})` },
          { Default: "(uuid_v7())" },
          "NotNull",
        ],
      };
    },
  ],
];

const presets: [string, (colName: string) => Column][] = [
  [
    "Default",
    (colName: string) => {
      return {
        name: colName,
        type_name: "TEXT",
        data_type: "Text",
        affinity_type: "Text",
        options: ["NotNull"],
      };
    },
  ],
  [
    "UUIDv4",
    (colName: string) => {
      return {
        name: colName,
        type_name: "BLOB",
        data_type: "Blob",
        affinity_type: "Blob",
        options: [
          { Check: `is_uuid(${colName})` },
          { Default: "(uuid_v4())" },
          "NotNull",
        ],
      };
    },
  ],
  [
    "UUIDv7",
    (colName: string) => {
      return {
        name: colName,
        type_name: "BLOB",
        data_type: "Blob",
        affinity_type: "Blob",
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
        name: colName,
        type_name: "TEXT",
        data_type: "Text",
        affinity_type: "Text",
        options: [
          { Check: `is_json(${colName})` },
          { Default: "'{}'" },
          "NotNull",
        ],
      };
    },
  ],
  [
    "File",
    (colName: string) => {
      return {
        name: colName,
        type_name: "TEXT",
        data_type: "Text",
        affinity_type: "Text",
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
        name: colName,
        type_name: "TEXT",
        data_type: "Text",
        affinity_type: "Text",
        options: [
          {
            Check: `jsonschema('std.FileUploads', ${colName})`,
          },
          { Default: "'[]'" },
          "NotNull",
        ],
      };
    },
  ],
];

const customCheckBoxStyle = "flex items-center justify-end py-1 gap-2";
const transitionTimingFunc = "cubic-bezier(.87,0,.13,1)";
