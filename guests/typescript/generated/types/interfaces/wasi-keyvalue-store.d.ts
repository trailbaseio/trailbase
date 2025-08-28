/** @module Interface wasi:keyvalue/store@0.2.0-draft **/
/**
 * Get the bucket with the specified identifier.
 *
 * `identifier` must refer to a bucket provided by the host.
 *
 * `error::no-such-store` will be raised if the `identifier` is not recognized.
 */
export function open(identifier: string): Bucket;
/**
 * The set of errors which may be raised by functions in this package
 */
export type Error = ErrorNoSuchStore | ErrorAccessDenied | ErrorOther;
/**
 * The host does not recognize the store identifier requested.
 */
export interface ErrorNoSuchStore {
  tag: 'no-such-store',
}
/**
 * The requesting component does not have access to the specified store
 * (which may or may not exist).
 */
export interface ErrorAccessDenied {
  tag: 'access-denied',
}
/**
 * Some implementation-specific error has occurred (e.g. I/O)
 */
export interface ErrorOther {
  tag: 'other',
  val: string,
}
/**
 * A response to a `list-keys` operation.
 */
export interface KeyResponse {
  /**
   * The list of keys returned by the query.
   */
  keys: Array<string>,
  /**
   * The continuation token to use to fetch the next page of keys. If this is `null`, then
   * there are no more keys to fetch.
   */
  cursor?: bigint,
}

export class Bucket implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
  /**
  * Get the value associated with the specified `key`
  *
  * The value is returned as an option. If the key-value pair exists in the
  * store, it returns `Ok(value)`. If the key does not exist in the
  * store, it returns `Ok(none)`.
  *
  * If any other error occurs, it returns an `Err(error)`.
  */
  get(key: string): Uint8Array | undefined;
  /**
  * Set the value associated with the key in the store. If the key already
  * exists in the store, it overwrites the value.
  *
  * If the key does not exist in the store, it creates a new key-value pair.
  *
  * If any other error occurs, it returns an `Err(error)`.
  */
  set(key: string, value: Uint8Array): void;
  /**
  * Delete the key-value pair associated with the key in the store.
  *
  * If the key does not exist in the store, it does nothing.
  *
  * If any other error occurs, it returns an `Err(error)`.
  */
  'delete'(key: string): void;
  /**
  * Check if the key exists in the store.
  *
  * If the key exists in the store, it returns `Ok(true)`. If the key does
  * not exist in the store, it returns `Ok(false)`.
  *
  * If any other error occurs, it returns an `Err(error)`.
  */
  exists(key: string): boolean;
  /**
  * Get all the keys in the store with an optional cursor (for use in pagination). It
  * returns a list of keys. Please note that for most KeyValue implementations, this is a
  * can be a very expensive operation and so it should be used judiciously. Implementations
  * can return any number of keys in a single response, but they should never attempt to
  * send more data than is reasonable (i.e. on a small edge device, this may only be a few
  * KB, while on a large machine this could be several MB). Any response should also return
  * a cursor that can be used to fetch the next page of keys. See the `key-response` record
  * for more information.
  *
  * Note that the keys are not guaranteed to be returned in any particular order.
  *
  * If the store is empty, it returns an empty list.
  *
  * MAY show an out-of-date list of keys if there are concurrent writes to the store.
  *
  * If any error occurs, it returns an `Err(error)`.
  */
  listKeys(cursor: bigint | undefined): KeyResponse;
    [Symbol.dispose]?: () => void;
}
