import type { TxError } from "trailbase:runtime/host-endpoint";
import type { SqliteRequest } from "@common/SqliteRequest";
import { JsonValue } from "@common/serde_json/JsonValue";

export type { Value as DbValue } from "trailbase:runtime/host-endpoint";

// export class DbError extends Error {
//   readonly error: TxError;
//
//   constructor(error: TxError) {
//     super(`${error}`);
//     this.error = error;
//   }
//
//   public override toString(): string {
//     return `DbError(${this.error})`;
//   }
// }

export type Blob = { blob: string };
export type Value = number | string | boolean | Uint8Array | Blob | null;

function toJsonValue(value: Value): JsonValue {
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

function fromJsonValue(value: JsonValue): Value {
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

export async function query(
  query: string,
  params: Value[],
): Promise<Value[][]> {
  const body: SqliteRequest = {
    query,
    params: params.map(toJsonValue),
  };
  const reply = await fetch("http://__sqlite/query", {
    method: "POST",
    headers: [["content-type", "application/json"]],
    body: JSON.stringify(body),
  });

  const json = await reply.json();
  if ("Error" in json) {
    const response = json as { Error: string };
    throw new Error(response.Error);
  }

  try {
    const response = json as { Query: { rows: Array<Array<JsonValue>> } };
    return response.Query.rows.map((row) => row.map(fromJsonValue));
  } catch (e) {
    throw new Error(`Unexpected response '${JSON.stringify(json)}': ${e}`);
  }
}

export async function execute(query: string, params: Value[]): Promise<number> {
  const body: SqliteRequest = {
    query,
    params: params.map(toJsonValue),
  };
  const reply = await fetch("http://__sqlite/execute", {
    method: "POST",
    headers: [["content-type", "application/json"]],
    body: JSON.stringify(body),
  });

  const json = await reply.json();
  if ("Error" in json) {
    const response = json as { Error: string };
    throw new Error(response.Error);
  }

  try {
    const response = json as { Execute: { rows_affected: number } };
    return response.Execute.rows_affected;
  } catch (e) {
    throw new Error(`Unexpected response '${JSON.stringify(json)}': ${e}`);
  }
}

/// Decode a base64 string to bytes.
export function base64Decode(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

/// Decode a "url-safe" base64 string to bytes.
export function urlSafeBase64Decode(base64: string): Uint8Array {
  return base64Decode(base64.replace(/_/g, "/").replace(/-/g, "+"));
}

/// Encode an arbitrary string input as base64 string.
export function base64Encode(b: Uint8Array): string {
  return btoa(String.fromCharCode(...b));
}

/// Encode an arbitrary string input as a "url-safe" base64 string.
export function urlSafeBase64Encode(b: Uint8Array): string {
  return base64Encode(b).replace(/\//g, "_").replace(/\+/g, "-");
}

function isDev(): boolean {
  type ImportMeta = {
    env: object | undefined;
  };
  const env = (import.meta as unknown as ImportMeta).env;
  const key = "DEV" as keyof typeof env;
  const isDev = env?.[key] ?? false;

  return isDev;
}

export const exportedForTesting = isDev()
  ? { fromJsonValue, toJsonValue }
  : undefined;
