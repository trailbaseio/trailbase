import { expect, test } from "vitest"
import { copyAndConvertRow } from "@/lib/convert";

type UnknownRow = { [key: string]: unknown };

test("utils", () => {
  const x: UnknownRow = {
    "foo": "test",
    "bar": "test",
  };
  const y = copyAndConvertRow(x);
  for (const key of Object.keys(x)) {
    expect(x[key]).toBe(y[key]);
  }
});
