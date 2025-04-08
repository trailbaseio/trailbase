import { expect, test, describe } from "vitest";
import { copyRow, type FormRow } from "@/lib/convert";

describe("utils", () => {
  test("coypAndConvertRow", () => {
    const x: FormRow = {
      text: "test",
      number: 5,
      boolean: true,
    };

    const y = copyRow(x);
    for (const key of Object.keys(x)) {
      expect(x[key]).toBe(y[key]);
    }

    // Make sure it's an actual copy.
    y["text"] = "update";

    expect(x["text"]).toBe("test");
  });
});
