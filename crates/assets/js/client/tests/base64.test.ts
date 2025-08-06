import { expect, test } from "vitest";
import {
  exportedForTesting,
  urlSafeBase64Encode,
  urlSafeBase64Decode,
  textEncode,
  textDecode,
  asyncBase64Encode,
} from "../src/index";

const { base64Encode, base64Decode } = exportedForTesting!;

test("encoding", async () => {
  const input = ".,~`!@#$%^&*()_Hi!:)/|\\";

  expect(textDecode(textEncode(input))).toBe(input);
  expect(base64Decode(base64Encode(input))).toBe(input);
  expect(urlSafeBase64Decode(urlSafeBase64Encode(input))).toBe(input);

  const blob = new Blob([textEncode(input)]);
  const base64 = await asyncBase64Encode(blob);
  const components = base64.split(",");

  expect(base64Decode(components[1])).toBe(input);
});
