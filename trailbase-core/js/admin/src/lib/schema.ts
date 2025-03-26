import type { Column } from "@bindings/Column";
import type { ColumnOption } from "@bindings/ColumnOption";
import type { ReferentialAction } from "@bindings/ReferentialAction";
import type { ConflictResolution } from "@bindings/ConflictResolution";
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

function columnsSatisfyRecordApiRequirements(
  columns: Column[],
  all: Table[],
): boolean {
  for (const column of columns) {
    if (isPrimaryKeyColumn(column)) {
      if (column.data_type === "Integer") {
        return true;
      }

      if (isUUIDv7Column(column)) {
        return true;
      }

      const foreign_key = getForeignKey(column.options);
      if (foreign_key) {
        const foreign_col_name = foreign_key.referred_columns[0];
        if (!foreign_col_name) {
          continue;
        }

        const foreign_table = all.find(
          (t) => t.name === foreign_key.foreign_table,
        );
        if (!foreign_table) {
          continue;
        }

        const foreign_col = foreign_table.columns.find(
          (c) => c.name === foreign_col_name,
        );
        if (foreign_col && isUUIDv7Column(foreign_col)) {
          return true;
        }
      }
    }
  }

  return false;
}

export function tableSatisfiesRecordApiRequirements(
  table: Table,
  all: Table[],
): boolean {
  if (table.strict) {
    return columnsSatisfyRecordApiRequirements(table.columns, all);
  }
  return false;
}

export function viewSatisfiesRecordApiRequirements(
  view: View,
  all: Table[],
): boolean {
  const columns = view.columns;
  if (columns) {
    return columnsSatisfyRecordApiRequirements(columns, all);
  }
  return false;
}

export type TableType = "table" | "virtualTable" | "view";

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

export function hiddenName(name: string): boolean {
  return name.startsWith("_") || name.startsWith("sqlite_");
}
