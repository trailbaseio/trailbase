/** @module Interface wasi:keyvalue/batch@0.2.0-draft **/
/**
 * Get the key-value pairs associated with the keys in the store. It returns a list of
 * key-value pairs.
 *
 * If any of the keys do not exist in the store, it returns a `none` value for that pair in the
 * list.
 *
 * MAY show an out-of-date value if there are concurrent writes to the store.
 *
 * If any other error occurs, it returns an `Err(error)`.
 */
export function getMany(bucket: Bucket, keys: Array<string>): Array<[string, Uint8Array] | undefined>;
/**
 * Set the values associated with the keys in the store. If the key already exists in the
 * store, it overwrites the value.
 *
 * Note that the key-value pairs are not guaranteed to be set in the order they are provided.
 *
 * If any of the keys do not exist in the store, it creates a new key-value pair.
 *
 * If any other error occurs, it returns an `Err(error)`. When an error occurs, it does not
 * rollback the key-value pairs that were already set. Thus, this batch operation does not
 * guarantee atomicity, implying that some key-value pairs could be set while others might
 * fail.
 *
 * Other concurrent operations may also be able to see the partial results.
 */
export function setMany(bucket: Bucket, keyValues: Array<[string, Uint8Array]>): void;
/**
 * Delete the key-value pairs associated with the keys in the store.
 *
 * Note that the key-value pairs are not guaranteed to be deleted in the order they are
 * provided.
 *
 * If any of the keys do not exist in the store, it skips the key.
 *
 * If any other error occurs, it returns an `Err(error)`. When an error occurs, it does not
 * rollback the key-value pairs that were already deleted. Thus, this batch operation does not
 * guarantee atomicity, implying that some key-value pairs could be deleted while others might
 * fail.
 *
 * Other concurrent operations may also be able to see the partial results.
 */
export function deleteMany(bucket: Bucket, keys: Array<string>): void;
export type Bucket = import('./wasi-keyvalue-store.js').Bucket;
export type Error = import('./wasi-keyvalue-store.js').Error;
