/** @module Interface wasi:keyvalue/atomics@0.2.0-draft **/
/**
 * Atomically increment the value associated with the key in the store by the given delta. It
 * returns the new value.
 *
 * If the key does not exist in the store, it creates a new key-value pair with the value set
 * to the given delta.
 *
 * If any other error occurs, it returns an `Err(error)`.
 */
export function increment(bucket: Bucket, key: string, delta: bigint): bigint;
export type Bucket = import('./wasi-keyvalue-store.js').Bucket;
export type Error = import('./wasi-keyvalue-store.js').Error;
