import { test, describe } from "vitest";
import { urlSafeBase64Encode } from "trailbase";

import { parseFilter } from "@/lib/list";
import { urlSafeBase64EncodeStream } from "@/lib/base64";
import { intPattern, uintPattern, floatPattern } from "@/components/FormFields";

describe("filterParser", () => {
  test("basic", ({ expect }) => {
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

describe("base64 stream", () => {
  test("empty", async ({ expect }) => {
    const input = new Uint8Array([]);

    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(input);
        controller.close();
      },
    });

    expect(await urlSafeBase64EncodeStream(stream)).toEqual(
      urlSafeBase64Encode(input),
    );
  });

  test("some", async ({ expect }) => {
    const input = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]);

    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(input);
        controller.close();
      },
    });

    expect(await urlSafeBase64EncodeStream(stream)).toEqual(
      urlSafeBase64Encode(input),
    );
  });

  test("large", async ({ expect }) => {
    const input = new Uint8Array(100 * 1024);

    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(input);
        controller.close();
      },
    });

    expect(await urlSafeBase64EncodeStream(stream)).toEqual(
      urlSafeBase64Encode(input),
    );
  });
});

describe("regexPatters", () => {
  test("int", ({ expect }) => {
    expect(intPattern.exec("0")?.[0]).toEqual("0");
    expect(intPattern.exec("123456789")?.[0]).toEqual("123456789");
    expect(intPattern.exec("1.1")).toBeNull();
    expect(intPattern.exec("+1")?.[0]).toEqual("+1");
    expect(intPattern.exec("-1")?.[0]).toEqual("-1");
  });

  test("uint", ({ expect }) => {
    expect(uintPattern.exec("0")?.[0]).toEqual("0");
    expect(uintPattern.exec("123456789")?.[0]).toEqual("123456789");
    expect(uintPattern.exec("1.1")).toBeNull();
    expect(uintPattern.exec("+1")?.[0]).toEqual("+1");
    expect(uintPattern.exec("-1")).toBeNull();
  });

  test("float", ({ expect }) => {
    expect(floatPattern.exec("0")?.[0]).toEqual("0");
    expect(floatPattern.exec("123456789")?.[0]).toEqual("123456789");
    expect(floatPattern.exec("1.1")?.[0]).toEqual("1.1");
    expect(floatPattern.exec("+1.1")?.[0]).toEqual("+1.1");
    expect(floatPattern.exec("+1")?.[0]).toEqual("+1");
    expect(floatPattern.exec("-1")?.[0]).toEqual("-1");
  });
});
