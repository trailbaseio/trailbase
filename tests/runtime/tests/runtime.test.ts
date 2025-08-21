import { test, expect } from "vitest";
import { exportedForTesting, urlSafeBase64Encode, urlSafeBase64Decode } from "../src/db";

test("base64", () => {
  expect(urlSafeBase64Decode(urlSafeBase64Encode(Uint8Array.from([0])))).toEqual(Uint8Array.from([0]));
  expect(urlSafeBase64Decode(urlSafeBase64Encode(Uint8Array.from([])))).toEqual(Uint8Array.from([]));
  expect(urlSafeBase64Decode(urlSafeBase64Encode(Uint8Array.from([1, 0])))).toEqual(Uint8Array.from([1, 0]));
})

test("value conversion", () => {
  const fromJsonValue = exportedForTesting!.fromJsonValue;
  const toJsonValue = exportedForTesting!.toJsonValue;

  expect(fromJsonValue(toJsonValue(true))).toEqual(1);
  expect(fromJsonValue(toJsonValue(false))).toEqual(0);
  expect(fromJsonValue(toJsonValue(5))).toEqual(5);
  expect(fromJsonValue(toJsonValue(-5))).toEqual(-5);
  expect(fromJsonValue(toJsonValue(5.123))).toEqual(5.123);
  expect(fromJsonValue(toJsonValue(-5.123))).toEqual(-5.123);
  expect(fromJsonValue(toJsonValue(""))).toEqual("");
  expect(fromJsonValue(toJsonValue(Uint8Array.from([])))).toEqual(Uint8Array.from([]));
  expect(fromJsonValue(toJsonValue(Uint8Array.from([0, 0])))).toEqual(Uint8Array.from([0, 0]));
  expect(fromJsonValue(toJsonValue(null))).toEqual(null);
});
