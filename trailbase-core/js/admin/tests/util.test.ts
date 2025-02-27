import { expect, test, describe } from "vitest";
import { copyRow, type FormRow } from "@/lib/convert";

describe("utils", () => {
  test("coypAndConvertRow", () => {
    const x: FormRow = {
      foo: "test",
      bar: "test",
    };
    const y = copyRow(x);
    for (const key of Object.keys(x)) {
      expect(x[key]).toBe(y[key]);
    }
  });
});
