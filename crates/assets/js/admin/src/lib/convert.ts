import { urlSafeBase64Decode, urlSafeBase64Encode } from "trailbase";

import { tryParseInt, tryParseFloat } from "@/lib/utils";
import {
  unescapeLiteral,
  unescapeLiteralBlob,
  getDefaultValue,
  getForeignKey,
  isPrimaryKeyColumn,
  isNotNull,
  isNullableColumn,
  isInt,
  isReal,
} from "@/lib/schema";

import type { Column } from "@bindings/Column";
import type { ColumnDataType } from "@bindings/ColumnDataType";
import type { Table } from "@bindings/Table";

// NOTE: We use a simpler type here over `Object`, `JsonValue` or other recursive
// type definitions to prevent solid-forms from doing infinite-recursion type
// gymnastics.
//
// There's a stark difference between null and undefined, the former is an
// explicit database value and the latter will be skipped in updates.
export type RowValue = string | number | boolean | null;

// A record representation of a single row keyed by column name.
export type FormRow = { [key: string]: RowValue };

// An array representation of a single row.
export type RowData = RowValue[];

export function literalDefault(
  type: ColumnDataType,
  value: string | undefined,
): RowValue | undefined {
  // Non literal if missing or function call, e.g. '(fun([col]))'.
  if (value === undefined || value.startsWith("(")) {
    return undefined;
  }

  if (type === "Blob") {
    // e.g. X'foo'.
    const blob = unescapeLiteralBlob(value);
    return blob !== undefined ? urlSafeBase64Encode(blob) : undefined;
  } else if (type === "Text") {
    // e.g. 'bar'.
    return unescapeLiteral(value);
  } else if (isInt(type)) {
    return tryParseInt(value);
  } else if (isReal(type)) {
    return tryParseFloat(value);
  }

  return undefined;
}

export function preProcessInsertValue(
  col: Column,
  value: RowValue | undefined,
): RowValue | undefined {
  const type = col.data_type;
  const isPk = isPrimaryKeyColumn(col);
  const notNull = isNotNull(col.options);
  const defaultValue = getDefaultValue(col.options);

  const nullable = isNullableColumn({
    type: col.data_type,
    notNull,
    isPk,
  });

  if (value === undefined || value === null) {
    if (nullable) {
      // NOTE: this may return undefined or null, which are explicitly different.
      return value;
    }

    if (defaultValue !== undefined) {
      return undefined;
    }

    throw Error(`Missing value for: ${col.name}`);
  }

  if (type === "Blob") {
    if (typeof value === "string") {
      if (value === "") {
        if (nullable || defaultValue !== undefined) {
          return undefined;
        }
      }

      try {
        urlSafeBase64Decode(value);
      } catch {
        throw new Error("Url-safe base64 decoding error");
      }

      return value;
    }

    throw Error(`Unexpected blob value for: ${col.name}: ${value}`);
  } else if (type === "Text") {
    if (typeof value === "string") {
      return value;
    }

    throw Error(`Unexpected string value for: ${col.name}: ${value}`);
  } else if (isInt(type)) {
    if (typeof value === "string") {
      if (value === "") {
        if (defaultValue !== undefined) {
          return undefined;
        }
      }

      const number = tryParseInt(value);
      if (number === undefined) {
        throw Error(`Unexpected int value for: ${col.name}: ${value}`);
      }
      return number;
    } else if (typeof value === "number") {
      return value;
    }

    throw Error(`Unexpected int value for: ${col.name}: ${value}`);
  } else if (isReal(type)) {
    if (typeof value === "string") {
      if (value === "") {
        if (defaultValue !== undefined) {
          return undefined;
        }
      }

      const number = tryParseFloat(value);
      if (number === undefined) {
        throw Error(`Unexpected real value for: ${col.name}: ${value}`);
      }
      return number;
    } else if (typeof value === "number") {
      return value;
    }

    throw Error(`Unexpected real value for: ${col.name}: ${value}`);
  }

  return value;
}

/// Updates and inserts are different with inserts not being able to tap into
/// default values.
export function preProcessUpdateValue(
  col: Column,
  value: RowValue | undefined,
): RowValue | undefined {
  const type = col.data_type;
  const isPk = isPrimaryKeyColumn(col);
  const notNull = isNotNull(col.options);

  if (value === undefined) {
    throw Error(`Missing value for: ${col.name}`);
  }

  const nullable = isNullableColumn({
    type: col.data_type,
    notNull,
    isPk,
  });

  if (value === null && nullable) {
    return null;
  }

  if (type === "Blob") {
    if (typeof value === "string") {
      if (value === "") {
        if (nullable) {
          return undefined;
        }
      }

      try {
        urlSafeBase64Decode(value);
      } catch {
        throw new Error("Url-safe base64 decoding error");
      }

      return value;
    }

    throw Error(`Unexpected blob value for: ${col.name}: ${value}`);
  } else if (type === "Text") {
    if (typeof value === "string") {
      return value;
    }

    throw Error(`Unexpected string value for: ${col.name}: ${value}`);
  } else if (isInt(type)) {
    if (typeof value === "string") {
      const number = tryParseInt(value);
      if (number === undefined) {
        throw Error(`Unexpected int value for: ${col.name}: ${value}`);
      }
      return number;
    } else if (typeof value === "number") {
      return value;
    }

    throw Error(`Unexpected int value for: ${col.name}: ${value}`);
  } else if (isReal(type)) {
    if (typeof value === "string") {
      const number = tryParseFloat(value);
      if (number === undefined) {
        throw Error(`Unexpected real value for: ${col.name}: ${value}`);
      }
      return number;
    } else if (typeof value === "number") {
      return value;
    }

    throw Error(`Unexpected real value for: ${col.name}: ${value}`);
  }

  return value;
}

export function preProcessRow(
  table: Table,
  row: FormRow,
  isUpdate: boolean,
): FormRow {
  const result: FormRow = {};
  for (const col of table.columns) {
    const value = isUpdate
      ? preProcessUpdateValue(col, row[col.name])
      : preProcessInsertValue(col, row[col.name]);
    if (value !== undefined) {
      result[col.name] = value;
    }
  }
  return result;
}

// Just to make it explicit.
export function copyRow(row: FormRow): FormRow {
  return { ...row };
}

export function buildDefaultRow(schema: Table): FormRow {
  const obj: FormRow = {};
  for (const col of schema.columns) {
    const type = col.data_type;
    const isPk = isPrimaryKeyColumn(col);
    const foreignKey = getForeignKey(col.options);
    const notNull = isNotNull(col.options);
    const defaultValue = getDefaultValue(col.options);
    const nullable = isNullableColumn({
      type: col.data_type,
      notNull,
      isPk,
    });

    /// If there's no default and the column is nullable we default to null.
    if (defaultValue !== undefined) {
      // If there is a default, we leave the form field empty and show the default as a textinput placeholder.
      obj[col.name] = null;
      continue;
    } else if (nullable) {
      obj[col.name] = null;
      continue;
    }

    // No default and non-nullable, i.e required...
    //
    // ...we fall back to generic defaults. We may be wrong based on CHECK constraints.
    if (type === "Blob") {
      if (foreignKey !== undefined) {
        obj[col.name] = `<${foreignKey.foreign_table.toUpperCase()}_ID>`;
      } else {
        obj[col.name] = "";
      }
    } else if (type === "Text") {
      obj[col.name] = "";
    } else if (isInt(type)) {
      obj[col.name] = 0;
    } else if (isReal(type)) {
      obj[col.name] = 0.0;
    } else {
      console.warn(`No fallback for ${type} column: ${col.name}`);
    }
  }
  return obj;
}
