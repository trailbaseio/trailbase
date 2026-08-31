/// Flavor that parses integer JSON values outside Number's safe range as BigInt.
export function parseJSON(text: string): unknown {
  function reviver(_key: string, value: unknown, context: { source: string }) {
    if (
      typeof value === "number" &&
      Number.isInteger(value) &&
      !Number.isSafeInteger(value)
    ) {
      return BigInt(context.source);
    }
    return value;
  }

  // JSON.parse's reviver context is not included in TypeScript's lib.dom types.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return JSON.parse(text, reviver as any);
}
