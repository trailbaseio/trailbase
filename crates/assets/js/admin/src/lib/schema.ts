import type { Column } from "@bindings/Column";
import type { ColumnDataType } from "@bindings/ColumnDataType";
import type { ColumnOption } from "@bindings/ColumnOption";
import type { ConflictResolution } from "@bindings/ConflictResolution";
import type { ReferentialAction } from "@bindings/ReferentialAction";
import type { QualifiedName } from "@bindings/QualifiedName";
import type { Table } from "@bindings/Table";
import type { View } from "@bindings/View";

export function isNotNull(options: ColumnOption[]): boolean {
  return options.findIndex((o: ColumnOption) => o === "NotNull") >= 0;
}

export function setNotNull(
  options: ColumnOption[],
  value: boolean,
): ColumnOption[] {
  const newOpts: ColumnOption[] = options.filter(
    (o) => o !== "Null" && o !== "NotNull",
  );
  newOpts.push(value ? "NotNull" : "Null");
  return newOpts;
}

function unpackDefaultValue(col: ColumnOption): string | undefined {
  if (typeof col === "object" && "Default" in col) {
    return col.Default as string;
  }
}

export function getDefaultValue(options: ColumnOption[]): string | undefined {
  for (const opt of options) {
    const v = unpackDefaultValue(opt);
    if (v !== undefined) {
      return v;
    }
  }
}

export function setDefaultValue(
  options: ColumnOption[],
  defaultValue: string | undefined,
): ColumnOption[] {
  const newOpts = options.filter((o) => unpackDefaultValue(o) === undefined);
  if (defaultValue !== undefined) {
    newOpts.push({ Default: defaultValue });
  }
  return newOpts;
}

function unpackCheckValue(col: ColumnOption): string | undefined {
  if (typeof col === "object" && "Check" in col) {
    return col.Check as string;
  }
}

export function getCheckValue(options: ColumnOption[]): string | undefined {
  return options.reduce<string | undefined>((acc, cur: ColumnOption) => {
    const maybeCheck = unpackCheckValue(cur);
    if (maybeCheck !== undefined) {
      return maybeCheck;
    }
    return acc;
  }, undefined);
}

export function setCheckValue(
  options: ColumnOption[],
  checkValue: string | undefined,
): ColumnOption[] {
  const newOpts = options.filter((o) => unpackCheckValue(o) === undefined);
  if (checkValue !== undefined) {
    newOpts.push({ Check: checkValue });
  }
  return newOpts;
}

export function isOptional(options: ColumnOption[]): boolean {
  let required = false;
  for (const opt of options) {
    if (opt === "NotNull") {
      required = true;
    }

    if (unpackDefaultValue(opt)) {
      return true;
    }
  }
  return !required;
}

export type ForeignKey = {
  foreign_table: string;
  referred_columns: Array<string>;
  on_delete: ReferentialAction | null;
  on_update: ReferentialAction | null;
};

export function getForeignKey(options: ColumnOption[]): ForeignKey | undefined {
  return options.reduce<ForeignKey | undefined>((acc, cur: ColumnOption) => {
    type U = { ForeignKey: ForeignKey };

    return typeof cur === "object" && "ForeignKey" in cur
      ? ((cur as U).ForeignKey as ForeignKey)
      : acc;
  }, undefined);
}

export function setForeignKey(
  options: ColumnOption[],
  fk: ForeignKey | undefined,
): ColumnOption[] {
  const newOpts = options.filter(
    (o) => typeof o !== "object" || !("ForeignKey" in o),
  );
  if (fk) {
    newOpts.push({ ForeignKey: fk });
  }
  return newOpts;
}

export type Unique = {
  is_primary: boolean;
  conflict_clause: ConflictResolution | null;
};

export function getUnique(options: ColumnOption[]): Unique | undefined {
  return options.reduce<Unique | undefined>((acc, cur: ColumnOption) => {
    type U = { Unique: { is_primary: boolean } };

    return typeof cur === "object" && "Unique" in cur
      ? ((cur as U).Unique as Unique)
      : acc;
  }, undefined);
}

export function setUnique(
  options: ColumnOption[],
  unique: Unique | undefined,
): ColumnOption[] {
  const newOpts = options.filter(
    (o) => typeof o !== "object" || !("Unique" in o),
  );
  if (unique) {
    newOpts.push({ Unique: unique });
  }
  return newOpts;
}

export function isPrimaryKeyColumn(column: Column): boolean {
  return getUnique(column.options)?.is_primary ?? false;
}

export function findPrimaryKeyColumnIndex(columns: Column[]): number {
  const candidate = columns.findIndex(isPrimaryKeyColumn);
  return candidate >= 0 ? candidate : 0;
}

export function isUUIDv7Column(column: Column): boolean {
  if (column.data_type === "Blob") {
    const check = getCheckValue(column.options);
    return (check?.search(/^is_uuid_v7\s*\(/g) ?? -1) === 0;
  }
  return false;
}

export function isUUIDColumn(column: Column): boolean {
  if (column.data_type === "Blob") {
    const check = getCheckValue(column.options);
    return (check?.search(/^is_uuid(|_v7|_v4)\s*\(/g) ?? -1) === 0;
  }
  return false;
}

export function isFileUploadColumn(column: Column): boolean {
  if (column.data_type === "Text") {
    const check = getCheckValue(column.options);
    return (check?.search(/^jsonschema\s*\('std.FileUpload'/g) ?? -1) === 0;
  }
  return false;
}

export function isFileUploadsColumn(column: Column): boolean {
  if (column.data_type === "Text") {
    const check = getCheckValue(column.options);
    return (check?.search(/jsonschema\s*\('std.FileUploads'/g) ?? -1) === 0;
  }
  return false;
}

export function isJSONColumn(column: Column): boolean {
  if (column.data_type === "Text") {
    const check = getCheckValue(column.options);
    return (check?.search(/^is_json\s*\(/g) ?? -1) === 0;
  }
  return false;
}

function isSuitableRecordPkColumn(column: Column, all: Table[]): boolean {
  if (!isPrimaryKeyColumn(column)) {
    return false;
  }

  switch (column.data_type) {
    case "Integer":
      return true;
    case "Blob": {
      if (isUUIDColumn(column)) {
        return true;
      }

      const foreign_key = getForeignKey(column.options);
      if (foreign_key) {
        const foreign_col_name = foreign_key.referred_columns[0];
        if (!foreign_col_name) {
          return false;
        }

        const foreign_table = all.find(
          (t) => t.name.name === foreign_key.foreign_table,
        );
        if (!foreign_table) {
          return false;
        }

        const foreign_col = foreign_table.columns.find(
          (c) => c.name === foreign_col_name,
        );
        if (foreign_col && isUUIDColumn(foreign_col)) {
          return true;
        }
      }
      break;
    }
  }

  return false;
}

export function tableSatisfiesRecordApiRequirements(
  table: Table,
  all: Table[],
): boolean {
  if (table.strict) {
    for (const column of table.columns) {
      if (isSuitableRecordPkColumn(column, all)) {
        return true;
      }
    }
  }
  return false;
}

export function viewSatisfiesRecordApiRequirements(
  view: View,
  all: Table[],
): boolean {
  const mapping = view.column_mapping;
  if (!mapping) {
    return false;
  }

  const groupBy = mapping.group_by;
  if (groupBy != null) {
    if (isSuitableRecordPkColumn(mapping.columns[groupBy].column, all)) {
      return true;
    }
  }

  const RIGHT = 0x10;
  const CROSS = 0x02;
  const NATURAL = 0x04;
  const MASK = RIGHT | CROSS | NATURAL;
  for (const joinType of mapping.joins) {
    if (joinType & MASK) {
      return false;
    }
  }

  for (const column of mapping.columns.map((c) => c.column)) {
    if (isSuitableRecordPkColumn(column, all)) {
      return true;
    }
  }

  return false;
}

export type TableType = "table" | "virtualTable" | "view";

export function getColumns(tableOrView: Table | View): undefined | Column[] {
  switch (tableType(tableOrView)) {
    case "table":
    case "virtualTable":
      return (tableOrView as Table).columns;
    case "view":
      return (tableOrView as View).column_mapping?.columns.map((c) => c.column);
  }
}

export function tableType(table: Table | View): TableType {
  if ("virtual_table" in table) {
    if (table.virtual_table) {
      return "virtualTable";
    }
    return "table";
  }

  return "view";
}

export function hiddenTable(table: Table | View): boolean {
  return hiddenName(table.name);
}

function hiddenName(name: QualifiedName): boolean {
  return name.name.startsWith("_") || name.name.startsWith("sqlite_");
}

export function isInt(type: ColumnDataType): boolean {
  switch (type) {
    case "Integer":
      return true;
    default:
      return false;
  }
}

export function isReal(type: ColumnDataType): boolean {
  switch (type) {
    case "Real":
      return true;
    default:
      return false;
  }
}

export function isNumber(type: ColumnDataType): boolean {
  return isInt(type) || isReal(type);
}

export function isNullableColumn(opts: {
  type: ColumnDataType;
  notNull: boolean;
  isPk: boolean;
}): boolean {
  // The column is optional, if nulls are allowed.
  if (!opts.notNull) {
    return true;
  }

  // Or if it's an integer primary key.
  if (opts.isPk && isInt(opts.type)) {
    return true;
  }

  // Anything goes.
  //
  // NOTE: Technically, any non-strict column is nullable. But that's not an input we allow to construct.
  if (opts.type === "Any") {
    return true;
  }

  return false;
}

export function unescapeLiteral(value: string): string {
  if (value === "") {
    return value;
  }

  const first = value[0];
  switch (first) {
    case "'":
    case '"':
    case "`":
    case "[":
      return value.substring(1, value.length - 1);
    default:
      return value;
  }
}

export function unescapeLiteralBlob(value: string): string | undefined {
  if (value === "") {
    return undefined;
  }

  const first = value[0];
  switch (first) {
    case "X":
    case "x":
      return value.substring(2, value.length - 1);
    default:
      return value;
  }
}

export function prettyFormatQualifiedName(name: QualifiedName): string {
  if (name.database_schema) {
    return `${name.database_schema}.${name.name}`;
  }
  return name.name;
}

export function equalQualifiedNames(
  a: QualifiedName,
  b: QualifiedName,
): boolean {
  if (a.name === b.name) {
    return (a.database_schema ?? "main") === (b.database_schema ?? "main");
  }
  return false;
}

export function compareQualifiedNames(
  a: QualifiedName,
  b: QualifiedName,
): number {
  const cmp = a.name.localeCompare(b.name);
  if (cmp === 0) {
    return (a.database_schema ?? "main").localeCompare(
      b.database_schema ?? "main",
    );
  }
  return cmp;
}
