import { tryParseFloat, tryParseBigInt } from "@/lib/utils";
import { unescapeLiteral, unescapeLiteralBlob } from "@/lib/schema";
import type { SqlValue } from "@/lib/value";

import type { ColumnDataType } from "@bindings/ColumnDataType";

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
  value: string,
): string | bigint | number | undefined {
  // Non literal if missing or function call, e.g. '(fun([col]))'.
  if (value === undefined || value.startsWith("(")) {
    return undefined;
  }

  if (type === "Blob") {
    // e.g. for X'abba' return "abba".
    const blob = unescapeLiteralBlob(value);
    if (blob !== undefined) {
      return blob;
    }
    return undefined;
  } else if (type === "Text") {
    // e.g. 'bar'.
    return unescapeLiteral(value);
  } else if (type === "Integer") {
    return tryParseBigInt(value);
  } else if (type === "Real") {
    return tryParseFloat(value);
  }

  return undefined;
}
