import * as JSON from "@ungap/raw-json";

// BigInt JSON stringify/parse shenanigans.
declare global {
  interface BigInt {
    toJSON(): unknown;
  }
}

BigInt.prototype.toJSON = function () {
  return JSON.rawJSON(this.toString());
};

export function parseJSON(text: string) {
  function reviver(_key: string, value: unknown, context: { source: string }) {
    if (
      typeof value === "number" &&
      Number.isInteger(value) &&
      !Number.isSafeInteger(value)
    ) {
      // Ignore the value because it has already lost precision
      return BigInt(context.source);
    }
    return value;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return JSON.parse(text, reviver as any);
}
