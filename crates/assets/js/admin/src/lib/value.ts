import type { Blob } from "@bindings/Blob";
import type { SqlValue } from "@bindings/SqlValue";

export type { Blob } from "@bindings/Blob";
export type { SqlValue } from "@bindings/SqlValue";

// Define partial types (afterwards assert they match the generated type union).
export type SqlNullValue = "Null";
export type SqlIntegerValue = { Integer: bigint };
export type SqlRealValue = { Real: number };
export type SqlTextValue = { Text: string };
export type SqlBlobValue = { Blob: Blob };

export type SqlNotNullValue =
  | SqlIntegerValue
  | SqlRealValue
  | SqlTextValue
  | SqlBlobValue;

export function assert<_T extends never>() {}
type TypeEqualityGuard<A, B> = Exclude<A, B> | Exclude<B, A>;

// Make sure our partial types match the ts-rs generated type.
assert<
  TypeEqualityGuard<
    SqlValue,
    SqlNullValue | SqlIntegerValue | SqlRealValue | SqlTextValue | SqlBlobValue
  >
>(); // no error
assert<
  TypeEqualityGuard<
    SqlNotNullValue,
    SqlIntegerValue | SqlRealValue | SqlTextValue | SqlBlobValue
  >
>(); // no error

export function sqlValueToString(value: SqlValue): string {
  if (value === "Null") {
    return "NULL";
  }

  if ("Integer" in value) {
    return value.Integer.toString();
  }

  if ("Real" in value) {
    return value.Real.toString();
  }

  if ("Blob" in value) {
    const blob: Blob = value.Blob;
    if ("Base64UrlSafe" in blob) {
      return blob.Base64UrlSafe;
    }
    throw Error("Expected Base64UrlSafe");
  }

  return value.Text;
}

export function getReal(value: SqlValue | undefined): number | undefined {
  if (value !== undefined && value !== "Null" && "Real" in value) {
    return value.Real;
  }
}

export function getInteger(value: SqlValue | undefined): bigint | undefined {
  if (value !== undefined && value !== "Null" && "Integer" in value) {
    return value.Integer;
  }
}

export function getText(value: SqlValue | undefined): string | undefined {
  if (value !== undefined && value !== "Null" && "Text" in value) {
    return value.Text;
  }
}

export function getBlob(value: SqlValue | undefined): string | undefined {
  if (value !== undefined && value !== "Null" && "Blob" in value) {
    const blob = value.Blob;
    if ("Base64UrlSafe" in blob) {
      return blob.Base64UrlSafe;
    }
    throw Error("Expected Base64UrlSafe");
  }
}
