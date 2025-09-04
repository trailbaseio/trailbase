import type { Value as WitValue } from "trailbase:runtime/host-endpoint";
import { JsonValue } from "@common/serde_json/JsonValue";

import { urlSafeBase64Encode, urlSafeBase64Decode } from "../util";

export type Blob = { blob: string };
export type Value = number | string | boolean | Uint8Array | Blob | null;

export function toJsonValue(value: Value): JsonValue {
  if (value === null) {
    return null;
  } else if (typeof value === "number") {
    return value;
  } else if (typeof value === "string") {
    return value;
  } else if (typeof value === "boolean") {
    return value ? 1 : 0;
  } else if (value instanceof Uint8Array) {
    return { blob: urlSafeBase64Encode(value) };
  } else if ("blob" in value) {
    return value;
  }

  throw new Error(`Invalid value: ${value}`);
}

export function fromJsonValue(value: JsonValue): Value {
  if (value === null) {
    return value;
  } else if (typeof value === "number") {
    return value;
  } else if (typeof value === "string") {
    return value;
  } else if (typeof value === "boolean") {
    return value ? 1 : 0;
  } else if ("blob" in value) {
    return urlSafeBase64Decode((value as Blob).blob);
  } else if (value == null) {
    return null;
  }

  throw new Error(`Invalid value: ${value}`);
}

export function toWitValue(val: Value): WitValue {
  if (val === null) {
    return { tag: "null" };
  } else if (typeof val === "number") {
    if (Number.isInteger(val)) {
      return { tag: "integer", val: BigInt(val) };
    }
    return { tag: "real", val };
  } else if (typeof val === "string") {
    return { tag: "text", val };
  } else if (typeof val === "boolean") {
    return { tag: "integer", val: val ? BigInt(1) : BigInt(0) };
  } else if (val instanceof Uint8Array) {
    return { tag: "blob", val };
  } else if ("blob" in val) {
    return { tag: "blob", val: urlSafeBase64Decode(val.blob) };
  }

  throw new Error(`Invalid value: ${val}`);
}

export function fromWitValue(val: WitValue): Value {
  switch (val.tag) {
    case "null":
      return null;
    case "integer":
      return Number(val.val);
    case "real":
      return val.val;
    case "text":
      return val.val;
    case "blob":
      return val.val;
  }
}
