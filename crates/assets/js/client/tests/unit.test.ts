import { test } from "vitest";
import { FetchError, initClient, exportedForTesting } from "../src/index";

const { parseJSON } = exportedForTesting!;

test("error-handling", async ({ expect }) => {
  expect(new FetchError(404, "test", "url").toString()).toEqual(
    "FetchError(404, test, url)",
  );

  const client = initClient("http://localhost:34444");

  // This is the actual `fetch()` failing to connect, i.e. throwing rather than yielding an error response.
  await expect(
    async () => await client.login("foo", "bar"),
  ).rejects.toThrowError(new TypeError("fetch failed"));
});

test("BigInt JSON parsing", ({ expect }) => {
  const huge = BigInt("0x1fffffffffffff"); // 9007199254740991n

  // Make sure we're actually beyond number precision.
  const clipped: number = Number(huge);
  expect(huge).not.toBe(clipped);

  const json = `{ "value": ${huge} }`;
  const obj: { value: bigint } = parseJSON(json);
  expect(obj.value, json).not.toBe(huge);
});
