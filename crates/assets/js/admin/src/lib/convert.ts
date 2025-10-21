import { urlSafeBase64Encode } from "trailbase";

import { tryParseFloat, tryParseBigInt } from "@/lib/utils";
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
import type { SqlValue } from "@/lib/value";

import type { ColumnDataType } from "@bindings/ColumnDataType";
import type { Table } from "@bindings/Table";

/// A record, i.e. row of SQL values (including "Null") or undefined (i.e.
/// don't submit), keyed by column name. We use a map-like structure to allow
/// for absence and avoid schema complexities and skew.
export type Record = { [key: string]: SqlValue | undefined };

// An array representation of a single row.
export type RowData = SqlValue[];

export function castToInteger(value: SqlValue): bigint {
  if (typeof value === "object" && "Integer" in value) {
    return value.Integer;
  }
  throw Error(`Expected integer, got: ${value}`);
}

export function hashSqlValue(value: SqlValue): string {
  return `__${JSON.stringify(value)}`;
}

export function shallowCopySqlValue(
  value: SqlValue | undefined,
): SqlValue | undefined {
  if (value === undefined) {
    return undefined;
  }
  if (value === "Null") {
    return "Null";
  }
  return { ...value };
}

export function literalDefault(
  type: ColumnDataType,
  value: string | undefined,
): string | bigint | number | undefined {
  // Non literal if missing or function call, e.g. '(fun([col]))'.
  if (value === undefined || value.startsWith("(")) {
    return undefined;
  }

  if (type === "Blob") {
    // e.g. X'foo'.
    const blob = unescapeLiteralBlob(value);
    if (blob !== undefined) {
      return urlSafeBase64Encode(Buffer.from(blob, "hex"));
    }
    return undefined;
  } else if (type === "Text") {
    // e.g. 'bar'.
    return unescapeLiteral(value);
  } else if (isInt(type)) {
    return tryParseBigInt(value);
  } else if (isReal(type)) {
    return tryParseFloat(value);
  }

  return undefined;
}

export function buildDefaultRow(schema: Table): Record {
  const obj: Record = {};

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
      obj[col.name] = undefined;
      continue;
    } else if (nullable) {
      obj[col.name] = "Null";
      continue;
    }

    // No default and non-nullable, i.e required...
    //
    // ...we fall back to generic defaults. We may be wrong based on CHECK constraints.
    if (type === "Blob") {
      if (foreignKey !== undefined) {
        obj[col.name] = {
          Blob: {
            Base64UrlSafe: `<${foreignKey.foreign_table.toUpperCase()}_ID>`,
          },
        };
      } else {
        obj[col.name] = { Blob: { Base64UrlSafe: "" } };
      }
    } else if (type === "Text") {
      obj[col.name] = { Text: "" };
    } else if (isInt(type)) {
      obj[col.name] = { Integer: BigInt(0) };
    } else if (isReal(type)) {
      obj[col.name] = { Real: 0.0 };
    } else {
      console.warn(
        `No fallback for column: ${col.name}, type: '${type}' - skipping default`,
      );
    }
  }
  return obj;
}
