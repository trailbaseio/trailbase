import { test } from "vitest";
import {
  escape,
  fromJsonSqlValue,
  fromWitValue,
  toJsonSqlValue,
  toWitValue,
} from "../src/db/value";
import { urlSafeBase64Encode, urlSafeBase64Decode } from "../src/util";

test("base64", ({ expect }) => {
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

test("JSON value conversion", ({ expect }) => {
  expect(fromJsonSqlValue(toJsonSqlValue(true))).toEqual(1n);
  expect(fromJsonSqlValue(toJsonSqlValue(false))).toEqual(0n);
  expect(fromJsonSqlValue(toJsonSqlValue(5))).toEqual(5);
  expect(fromJsonSqlValue(toJsonSqlValue(-5))).toEqual(-5);
  expect(fromJsonSqlValue(toJsonSqlValue(5.123))).toEqual(5.123);
  expect(fromJsonSqlValue(toJsonSqlValue(-5.123))).toEqual(-5.123);
  expect(fromJsonSqlValue(toJsonSqlValue(""))).toEqual("");
  expect(fromJsonSqlValue(toJsonSqlValue(Uint8Array.from([])))).toEqual(
    Uint8Array.from([]),
  );
  expect(fromJsonSqlValue(toJsonSqlValue(Uint8Array.from([0, 0])))).toEqual(
    Uint8Array.from([0, 0]),
  );
  expect(fromJsonSqlValue(toJsonSqlValue(null))).toEqual(null);
});

test("Wit value conversion", ({ expect }) => {
  expect(fromWitValue(toWitValue(true))).toEqual(1n);
  expect(fromWitValue(toWitValue(false))).toEqual(0n);
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

test("SQL escape", ({ expect }) => {
  expect(escape("")).toEqual("''");
  expect(escape("foo")).toEqual("'foo'");
  expect(escape("foo'")).toEqual("'foo'''");
  expect(escape("foo\0")).toEqual("'foo\\0'");
});
