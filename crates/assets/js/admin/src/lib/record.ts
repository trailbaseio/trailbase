import type { SqlValue } from "@/lib/value";

/// A record, i.e. row of SQL values (including "Null") or undefined (i.e.
/// don't submit), keyed by column name.
///
/// We use this for insert/update. The map-like structure to allow / for absence
/// and avoid schema complexities and skew. Values of `undefined` won't be
/// serialized sent across the wire.
export type Record = { [key: string]: SqlValue | undefined };

/// A(nother) record, , i.e. row of SQL values (including "Null").
///
/// We use this for reading/listing records. Every column is represented and is
/// accessed by index.
export type ArrayRecord = SqlValue[];
