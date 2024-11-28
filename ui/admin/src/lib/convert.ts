import type { JsonValue } from "@bindings/serde_json/JsonValue";

// NOTE: We use `unknown` here over `Object`, `JsonValue` or other recursive
// type definitions to prevent forms from doing infinite-recursion type
// gymnastics.
export type FormRow = { [key: string]: unknown };
type JsonRow = { [key in string]?: JsonValue };

export function copyAndConvertRow(row: FormRow): JsonRow {
  return Object.fromEntries(
    Object.entries(row).map(([k, v]) => [k, v as JsonValue | undefined]),
  );
}
