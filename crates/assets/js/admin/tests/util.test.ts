import { expect, test, describe } from "vitest";

import { parseFilter } from "@/lib/list";

describe("filterParser", () => {
  test("basic", () => {
    expect(() => parseFilter("x = 3)")).toThrow();
    expect(() => parseFilter("(x = 3 && x = 5 || x = 7)")).toThrow();

    expect(parseFilter("")).toEqual([]);

    expect(parseFilter("x = 3 || x = 4")).toEqual([
      ["filter[$or][0][x][$eq]", "3"],
      ["filter[$or][1][x][$eq]", "4"],
    ]);

    expect(parseFilter("x = 3 || x = 4 || x != 5")).toEqual([
      ["filter[$or][0][x][$eq]", "3"],
      ["filter[$or][1][x][$eq]", "4"],
      ["filter[$or][2][x][$ne]", "5"],
    ]);

    expect(parseFilter("(x = 3 || x = 4 || x != 5)")).toEqual([
      ["filter[$or][0][x][$eq]", "3"],
      ["filter[$or][1][x][$eq]", "4"],
      ["filter[$or][2][x][$ne]", "5"],
    ]);

    expect(parseFilter("(x = 3 || x = 4) && y != foo")).toEqual([
      ["filter[$and][0][$or][0][x][$eq]", "3"],
      ["filter[$and][0][$or][1][x][$eq]", "4"],
      ["filter[$and][1][y][$ne]", "foo"],
    ]);
  });
});
