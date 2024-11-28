// NOTE: We use `unknown` here over `Object` to prevent forms from doing infinite-recursion type gymnastics.
export type Row = { [key: string]: unknown };

export function copyAndConvertRow(row: Row): {
  // eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
  [key: string]: Object | undefined;
} {
  return Object.fromEntries(
    // eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
    Object.entries(row).map(([k, v]) => [k, v as Object | undefined]),
  );
}
