/** @module Interface wasi:io/streams@0.2.3 **/
export type Error = import('./wasi-io-error.js').Error;
export type Pollable = import('./wasi-io-poll.js').Pollable;
/**
 * An error for input-stream and output-stream operations.
 */
export type StreamError = StreamErrorLastOperationFailed | StreamErrorClosed;
/**
 * The last operation (a write or flush) failed before completion.
 * 
 * More information is available in the `error` payload.
 * 
 * After this, the stream will be closed. All future operations return
 * `stream-error::closed`.
 */
export interface StreamErrorLastOperationFailed {
  tag: 'last-operation-failed',
  val: Error,
}
/**
 * The stream is closed: no more input will be accepted by the
 * stream. A closed output-stream will return this error on all
 * future operations.
 */
export interface StreamErrorClosed {
  tag: 'closed',
}

export class InputStream implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
  /**
  * Perform a non-blocking read from the stream.
  * 
  * When the source of a `read` is binary data, the bytes from the source
  * are returned verbatim. When the source of a `read` is known to the
  * implementation to be text, bytes containing the UTF-8 encoding of the
  * text are returned.
  * 
  * This function returns a list of bytes containing the read data,
  * when successful. The returned list will contain up to `len` bytes;
  * it may return fewer than requested, but not more. The list is
  * empty when no bytes are available for reading at this time. The
  * pollable given by `subscribe` will be ready when more bytes are
  * available.
  * 
  * This function fails with a `stream-error` when the operation
  * encounters an error, giving `last-operation-failed`, or when the
  * stream is closed, giving `closed`.
  * 
  * When the caller gives a `len` of 0, it represents a request to
  * read 0 bytes. If the stream is still open, this call should
  * succeed and return an empty list, or otherwise fail with `closed`.
  * 
  * The `len` parameter is a `u64`, which could represent a list of u8 which
  * is not possible to allocate in wasm32, or not desirable to allocate as
  * as a return value by the callee. The callee may return a list of bytes
  * less than `len` in size while more bytes are available for reading.
  */
  read(len: bigint): Uint8Array;
  /**
  * Read bytes from a stream, after blocking until at least one byte can
  * be read. Except for blocking, behavior is identical to `read`.
  */
  blockingRead(len: bigint): Uint8Array;
  /**
  * Skip bytes from a stream. Returns number of bytes skipped.
  * 
  * Behaves identical to `read`, except instead of returning a list
  * of bytes, returns the number of bytes consumed from the stream.
  */
  skip(len: bigint): bigint;
  /**
  * Skip bytes from a stream, after blocking until at least one byte
  * can be skipped. Except for blocking behavior, identical to `skip`.
  */
  blockingSkip(len: bigint): bigint;
  /**
  * Create a `pollable` which will resolve once either the specified stream
  * has bytes available to read or the other end of the stream has been
  * closed.
  * The created `pollable` is a child resource of the `input-stream`.
  * Implementations may trap if the `input-stream` is dropped before
  * all derived `pollable`s created with this function are dropped.
  */
  subscribe(): Pollable;
    [Symbol.dispose]?: () => void;
}

export class OutputStream implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
  /**
  * Check readiness for writing. This function never blocks.
  * 
  * Returns the number of bytes permitted for the next call to `write`,
  * or an error. Calling `write` with more bytes than this function has
  * permitted will trap.
  * 
  * When this function returns 0 bytes, the `subscribe` pollable will
  * become ready when this function will report at least 1 byte, or an
  * error.
  */
  checkWrite(): bigint;
  /**
  * Perform a write. This function never blocks.
  * 
  * When the destination of a `write` is binary data, the bytes from
  * `contents` are written verbatim. When the destination of a `write` is
  * known to the implementation to be text, the bytes of `contents` are
  * transcoded from UTF-8 into the encoding of the destination and then
  * written.
  * 
  * Precondition: check-write gave permit of Ok(n) and contents has a
  * length of less than or equal to n. Otherwise, this function will trap.
  * 
  * returns Err(closed) without writing if the stream has closed since
  * the last call to check-write provided a permit.
  */
  write(contents: Uint8Array): void;
  /**
  * Perform a write of up to 4096 bytes, and then flush the stream. Block
  * until all of these operations are complete, or an error occurs.
  * 
  * This is a convenience wrapper around the use of `check-write`,
  * `subscribe`, `write`, and `flush`, and is implemented with the
  * following pseudo-code:
  * 
  * ```text
  * let pollable = this.subscribe();
  * while !contents.is_empty() {
    *     // Wait for the stream to become writable
    *     pollable.block();
    *     let Ok(n) = this.check-write(); // eliding error handling
    *     let len = min(n, contents.len());
    *     let (chunk, rest) = contents.split_at(len);
    *     this.write(chunk  );            // eliding error handling
    *     contents = rest;
    * }
    * this.flush();
    * // Wait for completion of `flush`
    * pollable.block();
    * // Check for any errors that arose during `flush`
    * let _ = this.check-write();         // eliding error handling
    * ```
    */
    blockingWriteAndFlush(contents: Uint8Array): void;
    /**
    * Request to flush buffered output. This function never blocks.
    * 
    * This tells the output-stream that the caller intends any buffered
    * output to be flushed. the output which is expected to be flushed
    * is all that has been passed to `write` prior to this call.
    * 
    * Upon calling this function, the `output-stream` will not accept any
    * writes (`check-write` will return `ok(0)`) until the flush has
    * completed. The `subscribe` pollable will become ready when the
    * flush has completed and the stream can accept more writes.
    */
    flush(): void;
    /**
    * Request to flush buffered output, and block until flush completes
    * and stream is ready for writing again.
    */
    blockingFlush(): void;
    /**
    * Create a `pollable` which will resolve once the output-stream
    * is ready for more writing, or an error has occurred. When this
    * pollable is ready, `check-write` will return `ok(n)` with n>0, or an
    * error.
    * 
    * If the stream is closed, this pollable is always ready immediately.
    * 
    * The created `pollable` is a child resource of the `output-stream`.
    * Implementations may trap if the `output-stream` is dropped before
    * all derived `pollable`s created with this function are dropped.
    */
    subscribe(): Pollable;
    /**
    * Write zeroes to a stream.
    * 
    * This should be used precisely like `write` with the exact same
    * preconditions (must use check-write first), but instead of
    * passing a list of bytes, you simply pass the number of zero-bytes
    * that should be written.
    */
    writeZeroes(len: bigint): void;
    /**
    * Perform a write of up to 4096 zeroes, and then flush the stream.
    * Block until all of these operations are complete, or an error
    * occurs.
    * 
    * This is a convenience wrapper around the use of `check-write`,
    * `subscribe`, `write-zeroes`, and `flush`, and is implemented with
    * the following pseudo-code:
    * 
    * ```text
    * let pollable = this.subscribe();
    * while num_zeroes != 0 {
      *     // Wait for the stream to become writable
      *     pollable.block();
      *     let Ok(n) = this.check-write(); // eliding error handling
      *     let len = min(n, num_zeroes);
      *     this.write-zeroes(len);         // eliding error handling
      *     num_zeroes -= len;
      * }
      * this.flush();
      * // Wait for completion of `flush`
      * pollable.block();
      * // Check for any errors that arose during `flush`
      * let _ = this.check-write();         // eliding error handling
      * ```
      */
      blockingWriteZeroesAndFlush(len: bigint): void;
      /**
      * Read from one stream and write to another.
      * 
      * The behavior of splice is equivalent to:
      * 1. calling `check-write` on the `output-stream`
      * 2. calling `read` on the `input-stream` with the smaller of the
      * `check-write` permitted length and the `len` provided to `splice`
      * 3. calling `write` on the `output-stream` with that read data.
      * 
      * Any error reported by the call to `check-write`, `read`, or
      * `write` ends the splice and reports that error.
      * 
      * This function returns the number of bytes transferred; it may be less
      * than `len`.
      */
      splice(src: InputStream, len: bigint): bigint;
      /**
      * Read from one stream and write to another, with blocking.
      * 
      * This is similar to `splice`, except that it blocks until the
      * `output-stream` is ready for writing, and the `input-stream`
      * is ready for reading, before performing the `splice`.
      */
      blockingSplice(src: InputStream, len: bigint): bigint;
        [Symbol.dispose]?: () => void;
    }
    