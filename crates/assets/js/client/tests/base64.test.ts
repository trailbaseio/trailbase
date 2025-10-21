import { expect, test } from "vitest";
import {
  exportedForTesting,
  urlSafeBase64Encode,
  urlSafeBase64Decode,
} from "../src/index";

const { base64Encode, base64Decode } = exportedForTesting!;

test("encoding", async () => {
  const byteInput = Uint8Array.from(".,~`!@#$%^&*()_Hi!:)/|\\");

  expect(base64Decode(base64Encode(byteInput))).toStrictEqual(byteInput);
  expect(urlSafeBase64Decode(urlSafeBase64Encode(byteInput))).toStrictEqual(
    byteInput,
  );
});
