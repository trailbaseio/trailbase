import { createEffect, createMemo, createSignal, For, Show } from "solid-js";
import type { JSX } from "solid-js";
import { Collapsible } from "@kobalte/core/collapsible";
import {
  TbChevronDown,
  TbTrash,
  TbInfoCircle,
  TbArrowUp,
  TbArrowDown,
} from "solid-icons/tb";
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
  intPattern,
  gapStyle,
} from "@/components/FormFields";
import type { FormApiT, AnyFieldApi } from "@/components/FormFields";

import {
  columnDataTypes,
  equalQualifiedNames,
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
  allTables: Table[],
) {
  return (field: () => AnyFieldApi) => {
    const fk = () =>
      getForeignKey(form.getFieldValue(`columns[${colIndex}].options`));

    // Note: use createMemo to avoid rebuilds for any state change.
    const value = createMemo(() => field().state.value);

    // Effect that updates the data type when a foreign key is being selected.
    createEffect(() => {
      const foreignKey = fk();

      if (foreignKey) {
        const targetTable = {
          name: foreignKey.foreign_table,
          database_schema: form.state.values.name.database_schema,
        };

        for (const table of allTables) {
          if (equalQualifiedNames(table.name, targetTable)) {
            const targetType = table.columns[0].data_type;
            if (value() !== targetType) {
              field().setValue(targetType);

              patchColumn(form, colIndex, typeNameAndAffinityType(targetType));
            }
            return;
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
          patchColumn(form, colIndex, typeNameAndAffinityType(v));
        }}
        handleBlur={field().handleBlur}
      />
    );
  };
}

function ColumnOptionCheckField(props: {
  columnName: string;
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
              <span class="font-mono font-bold">{`${props.columnName} < 42 `}</span>
              including SQL function calls like{" "}
              <span class="font-mono font-bold">
                is_email({props.columnName})
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
  data_type: ColumnDataType;
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

  const defaultFieldValidationPattern = () => {
    const functionPattern = "[(].*[)]";
    const blobPattern = "X'.*'";
    const textPattern = "'.*'";

    switch (props.data_type) {
      case "Any":
        return `^\\s*(${functionPattern}|${blobPattern}|${textPattern}|${intPattern}|${floatPattern}})\\s*$`;
      case "Blob":
        return `^\\s*(${functionPattern}|${blobPattern})\\s*$`;
      case "Text":
        return `^\\s*(${functionPattern}|${textPattern})\\s*$`;
      case "Integer":
        return `^\\s*(${functionPattern}|${intPattern})\\s*$`;
      case "Real":
        return `^\\s*(${functionPattern}|${floatPattern})\\s*$`;
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

        {/* `type` needs to be "text" to allow for functions, e.g.: (unixepoch()) */}
        <TextFieldInput
          disabled={props.disabled}
          type="text"
          pattern={defaultFieldValidationPattern()}
          value={getDefaultValue(props.value) ?? ""}
          onChange={(e: Event) => {
            const value: string | undefined = (
              e.currentTarget as HTMLInputElement
            ).value;

            props.onChange(
              setDefaultValue(
                props.value,
                value === "" ? undefined : value.trim(),
              ),
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
  databaseSchema: string | null;
}) {
  const fkTableOptions = createMemo((): string[] => [
    "None",
    ...props.allTables
      .filter((schema) => {
        if (schema.temporary || schema.virtual_table) {
          return false;
        }

        const db = schema.name.database_schema;
        if (props.databaseSchema === null) {
          return !db || db === "main";
        }
        return db === props.databaseSchema;
      })
      .map((schema) => schema.name.name),
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
            props.onChange(setForeignKey(props.value, undefined));
            return;
          }

          const schema = props.allTables.find(
            (schema) => schema.name.name == table,
          )!;
          const referredColumn =
            schema.columns.find(
              (col) => getUnique(col.options)?.is_primary ?? false,
            ) ?? schema.columns[0];

          let newColumnOptions = [...props.value];
          newColumnOptions = setForeignKey(props.value, {
            foreign_table: table,
            referred_columns: [referredColumn.name],
            on_delete: null,
            on_update: null,
          });
          newColumnOptions = setCheckValue(newColumnOptions, undefined);
          newColumnOptions = setDefaultValue(newColumnOptions, undefined);

          props.onChange(newColumnOptions);
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
  value: ColumnOption[];
  onChange: (v: ColumnOption[]) => void;
  allTables: Table[];
  disabled: boolean;
  pk: boolean;
  columnName: string;
  data_type: ColumnDataType;
  databaseSchema: string | null;
}) {
  const fk = () => getForeignKey(props.value);

  // SQLite column options: (not|null), (default), (unique), (fk), (check), (comment), (on-update trigger).
  return (
    <>
      {/* FOREIGN KEY constraint */}
      {!props.pk && <ColumnOptionFkSelect {...props} />}

      {/* DEFAULT constraint */}
      <ColumnOptionDefaultField
        data_type={props.data_type}
        value={props.value}
        onChange={props.onChange}
        disabled={props.disabled || fk() !== undefined}
      />

      {/* CHECK constraint */}
      <ColumnOptionCheckField
        columnName={props.columnName}
        value={props.value}
        onChange={props.onChange}
        disabled={props.disabled || fk() !== undefined}
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
  allTables: Table[];
  disabled: boolean;
  onDelete: () => void;
  moveUp?: () => void;
  moveDown?: () => void;
}): JSX.Element {
  const [name, setName] = createWritableMemo(() =>
    props.form.getFieldValue(`columns[${props.colIndex}].name`),
  );
  const dataType = () =>
    props.form.getFieldValue(`columns[${props.colIndex}].data_type`);

  const [expanded, setExpanded] = createSignal(true);

  const databaseSchema = createMemo(() =>
    props.form.useStore((state) => state.values.name.database_schema)(),
  );

  const Header = () => (
    <div class="flex items-center justify-between">
      <h3 class="truncate">{name()}</h3>

      <div class="flex items-center gap-2">
        <Show when={props.moveUp}>
          <IconButton onClick={props.moveUp}>
            <TbArrowUp />
          </IconButton>
        </Show>

        <Show when={props.moveDown}>
          <IconButton onClick={props.moveDown}>
            <TbArrowDown />
          </IconButton>
        </Show>

        {/* Delete column button. */}
        <Show when={!props.disabled}>
          <IconButton onClick={props.onDelete}>
            <TbTrash />
          </IconButton>
        </Show>

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
                          patchColumn(
                            props.form,
                            props.colIndex,
                            preset(
                              props.form.state.values.columns[props.colIndex]
                                .name,
                            ),
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
                validators={{
                  onChange: ({ value }: { value: string | undefined }) => {
                    return value ? undefined : "Column name missing";
                  },
                }}
              >
                {buildTextFormField({
                  label: () => <L>Name</L>,
                  disabled: props.disabled,
                  onInput: (e) => {
                    const value: string = (e.target as HTMLInputElement).value;
                    setName(value === "" ? "<empty>" : value);
                  },
                })}
              </props.form.Field>

              {/* Column type field */}
              <props.form.Field name={`columns[${props.colIndex}].data_type`}>
                {columnTypeField(
                  props.form,
                  props.colIndex,
                  props.disabled,
                  props.allTables,
                )}
              </props.form.Field>

              {/* Column options: pk, not null, ... */}
              <props.form.Field
                name={`columns[${props.colIndex}].options`}
                children={(field) => {
                  return (
                    <ColumnOptionsFields
                      columnName={name()}
                      data_type={dataType()}
                      value={field().state.value}
                      onChange={field().handleChange}
                      allTables={props.allTables}
                      disabled={props.disabled}
                      pk={false}
                      databaseSchema={databaseSchema()}
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
  allTables: Table[];
  disabled: boolean;
}): JSX.Element {
  const [name, setName] = createWritableMemo(() =>
    props.form.getFieldValue(`columns[${props.colIndex}].name`),
  );
  const dataType = () =>
    props.form.getFieldValue(`columns[${props.colIndex}].data_type`);

  // NOTE: createSignal state gets discarded when reordering columns, we should probably inject a signal instead.
  const [expanded, setExpanded] = createSignal(false);

  const databaseSchema = createMemo(() =>
    props.form.useStore((state) => state.values.name.database_schema)(),
  );

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
              <Show when={!props.disabled}>
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
                            patchColumn(
                              props.form,
                              props.colIndex,
                              preset(
                                props.form.state.values.columns[props.colIndex]
                                  .name,
                              ),
                            );
                          }}
                        >
                          {name}
                        </ButtonBadge>
                      )}
                    </For>
                  </div>
                </div>
              </Show>

              {/* Column name field */}
              <props.form.Field
                name={`columns[${props.colIndex}].name`}
                validators={{
                  onChange: ({ value }: { value: string | undefined }) => {
                    return value ? undefined : "Column name missing";
                  },
                }}
              >
                {buildTextFormField({
                  label: () => <L>Name</L>,
                  disabled: props.disabled,
                  onInput: (e) => {
                    const value: string = (e.target as HTMLInputElement).value;
                    setName(value === "" ? "<empty>" : value);
                  },
                })}
              </props.form.Field>

              {/* Column type field */}
              <props.form.Field
                name={`columns[${props.colIndex}].data_type`}
                children={columnTypeField(
                  props.form,
                  props.colIndex,
                  /*disabled=*/ true,
                  props.allTables,
                )}
              />

              {/* Column options: pk, not null, ... */}
              <props.form.Field
                name={`columns[${props.colIndex}].options`}
                children={(field) => {
                  return (
                    <ColumnOptionsFields
                      columnName={name()}
                      data_type={dataType()}
                      value={field().state.value}
                      onChange={field().handleChange}
                      allTables={props.allTables}
                      disabled={true}
                      pk={true}
                      databaseSchema={databaseSchema()}
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
  update: Partial<Column>,
) {
  const col = form.state.values.columns[colIndex];
  Object.assign(col, update);
  form.setFieldValue(`columns[${colIndex}]`, col);
}

function typeNameAndAffinityType(
  dataType: ColumnDataType,
): Pick<Column, "affinity_type" | "type_name"> {
  switch (dataType) {
    case "Any":
      return {
        type_name: "ANY",
        affinity_type: "Blob",
      };
    case "Blob":
      return {
        type_name: "BLOB",
        affinity_type: "Blob",
      };
    case "Text":
      return {
        type_name: "TEXT",
        affinity_type: "Text",
      };
    case "Integer":
      return {
        type_name: "INTEGER",
        affinity_type: "Integer",
      };
    case "Real":
      return {
        type_name: "REAL",
        affinity_type: "Real",
      };
  }
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
