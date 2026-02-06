import { test } from "vitest";
import { FetchError, initClient } from "../src/index";

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
