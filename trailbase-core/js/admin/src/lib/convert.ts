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

// Just to make it explicit.
export function copyRow(row: FormRow): FormRow {
  return { ...row };
}
