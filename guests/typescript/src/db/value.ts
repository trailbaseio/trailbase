import type { Value as WitValue } from "trailbase:runtime/host-endpoint";
import { SqlValue } from "@common/SqlValue";
import { Blob } from "@common/Blob";

import { urlSafeBase64Encode, urlSafeBase64Decode } from "../util";

/// Public JS types for DB params.
///
/// TODO: We should probably remove "boolean" since, it can be encoded but never decoded.
export type Value = null | boolean | bigint | number | string | Uint8Array;

// TODO: this only exists while piggy-backing on HTTP and sending params as JSON requests. Remove with WASIp3 and use WitValue only.
export function toJsonSqlValue(value: Value): SqlValue {
  if (value === null) {
    return "Null";
  } else if (typeof value === "boolean") {
    return { Integer: BigInt(value ? 1 : 0) };
  } else if (typeof value === "bigint") {
    return { Integer: value };
  } else if (typeof value === "number") {
    return { Real: value };
  } else if (typeof value === "string") {
    return { Text: value };
  } else if (value instanceof Uint8Array) {
    return {
      Blob: {
        Base64UrlSafe: urlSafeBase64Encode(value),
      },
    };
  }

  throw new Error(`Invalid value: ${value}`);
}

// TODO: this only exists while piggy-backing on HTTP and sending params as JSON requests. Remove with WASIp3 and use WitValue only.
export function fromJsonSqlValue(value: SqlValue): Value {
  if (value === "Null") {
    return null;
  } else if ("Integer" in value) {
    return value.Integer;
  } else if ("Real" in value) {
    return value.Real;
  } else if ("Text" in value) {
    return value.Text;
  } else if ("Blob" in value) {
    const blob: Blob = value.Blob;
    if ("Array" in blob) {
      return Uint8Array.from(blob.Array);
    } else if ("Base64UrlSafe" in blob) {
      return urlSafeBase64Decode(blob.Base64UrlSafe);
    } else if ("Hex" in blob) {
      // NOTE: the Host always uses Base64UrlSafe for better compression ratio.
      throw new Error(`Hex not supported: ${value}`);
    }
  }

  throw new Error(`Invalid value: ${value}`);
}

export function toWitValue(val: Value): WitValue {
  if (val === null) {
    return { tag: "null" };
  } else if (typeof val === "boolean") {
    return { tag: "integer", val: val ? BigInt(1) : BigInt(0) };
  } else if (typeof val === "bigint") {
    return { tag: "integer", val: BigInt(val) };
  } else if (typeof val === "number") {
    return { tag: "real", val };
  } else if (typeof val === "string") {
    return { tag: "text", val };
  } else if (val instanceof Uint8Array) {
    return { tag: "blob", val };
  }

  throw new Error(`Invalid value: ${val}`);
}

export function fromWitValue(val: WitValue): Value {
  switch (val.tag) {
    case "null":
      return null;
    case "integer":
      return BigInt(val.val);
    case "real":
      return val.val;
    case "text":
      return val.val;
    case "blob":
      return val.val;
  }
}
