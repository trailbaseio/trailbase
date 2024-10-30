import { expect, test } from "vitest"
import { copyAndConvert } from "@/components/tables/InsertAlterRow";

type UnkownRow = { [key: string]: unknown };
// eslint-disable-next-line @typescript-eslint/no-wrapper-object-types
type ObjectRow = { [key: string]: Object | undefined };

test("utils", () => {
  const x: UnkownRow = {
    "foo": "test",
    "bar": "test",
  };
  const y: ObjectRow = copyAndConvert(x);
  for (const key of Object.keys(x)) {
    expect(x[key]).toBe(y[key]);
  }
});
