import { expect, test } from "vitest"
import { copyAndConvertRow } from "@/lib/convert";

type UnknownRow = { [key: string]: unknown };
// eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
type ObjectRow = { [key: string]: Object | undefined };

test("utils", () => {
  const x: UnknownRow = {
    "foo": "test",
    "bar": "test",
  };
  const y: ObjectRow = copyAndConvertRow(x);
  for (const key of Object.keys(x)) {
    expect(x[key]).toBe(y[key]);
  }
});
