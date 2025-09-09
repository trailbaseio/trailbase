import { test, expect } from "vitest";
import {
  fromJsonValue,
  toJsonValue,
  fromWitValue,
  toWitValue,
} from "../src/db/value";
import { urlSafeBase64Encode, urlSafeBase64Decode } from "../src/util";

test("base64", () => {
  expect(
    urlSafeBase64Decode(urlSafeBase64Encode(Uint8Array.from([0]))),
  ).toEqual(Uint8Array.from([0]));
  expect(urlSafeBase64Decode(urlSafeBase64Encode(Uint8Array.from([])))).toEqual(
    Uint8Array.from([]),
  );
  expect(
    urlSafeBase64Decode(urlSafeBase64Encode(Uint8Array.from([1, 0]))),
  ).toEqual(Uint8Array.from([1, 0]));
});

test("JSON value conversion", () => {
  expect(fromJsonValue(toJsonValue(true))).toEqual(1);
  expect(fromJsonValue(toJsonValue(false))).toEqual(0);
  expect(fromJsonValue(toJsonValue(5))).toEqual(5);
  expect(fromJsonValue(toJsonValue(-5))).toEqual(-5);
  expect(fromJsonValue(toJsonValue(5.123))).toEqual(5.123);
  expect(fromJsonValue(toJsonValue(-5.123))).toEqual(-5.123);
  expect(fromJsonValue(toJsonValue(""))).toEqual("");
  expect(fromJsonValue(toJsonValue(Uint8Array.from([])))).toEqual(
    Uint8Array.from([]),
  );
  expect(fromJsonValue(toJsonValue(Uint8Array.from([0, 0])))).toEqual(
    Uint8Array.from([0, 0]),
  );
  expect(fromJsonValue(toJsonValue(null))).toEqual(null);
});

test("Wit value conversion", () => {
  expect(fromWitValue(toWitValue(true))).toEqual(1);
  expect(fromWitValue(toWitValue(false))).toEqual(0);
  expect(fromWitValue(toWitValue(5))).toEqual(5);
  expect(fromWitValue(toWitValue(-5))).toEqual(-5);
  expect(fromWitValue(toWitValue(5.123))).toEqual(5.123);
  expect(fromWitValue(toWitValue(-5.123))).toEqual(-5.123);
  expect(fromWitValue(toWitValue(""))).toEqual("");
  expect(fromWitValue(toWitValue(Uint8Array.from([])))).toEqual(
    Uint8Array.from([]),
  );
  expect(fromWitValue(toWitValue(Uint8Array.from([0, 0])))).toEqual(
    Uint8Array.from([0, 0]),
  );
  expect(fromWitValue(toWitValue(null))).toEqual(null);
});
