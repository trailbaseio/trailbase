import { test } from "vitest";
import { Bench } from "tinybench";
import {
  urlSafeBase64Encode,
  urlSafeBase64Decode,
  exportedForTesting,
} from "../src/index";

const { base64Encode, base64Decode } = exportedForTesting!;

test("encoding benchmark", async () => {
  const bench = new Bench({ time: 500 });

  const input = Uint8Array.from("!@#$%^&*(!@#$%^&*@".repeat(1000));
  const standardInput = base64Encode(input);
  const urlSafeInput = urlSafeBase64Encode(input);

  bench
    .add("Url-Safe decode", () => {
      urlSafeBase64Decode(urlSafeInput);
    })
    .add("Standard decode", () => {
      base64Decode(standardInput);
    });

  await bench.run();

  console.table(bench.table());
});
