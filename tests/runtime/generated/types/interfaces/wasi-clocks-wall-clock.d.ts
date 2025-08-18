/** @module Interface wasi:clocks/wall-clock@0.2.3 **/
/**
 * Read the current value of the clock.
 * 
 * This clock is not monotonic, therefore calling this function repeatedly
 * will not necessarily produce a sequence of non-decreasing values.
 * 
 * The returned timestamps represent the number of seconds since
 * 1970-01-01T00:00:00Z, also known as [POSIX's Seconds Since the Epoch],
 * also known as [Unix Time].
 * 
 * The nanoseconds field of the output is always less than 1000000000.
 * 
 * [POSIX's Seconds Since the Epoch]: https://pubs.opengroup.org/onlinepubs/9699919799/xrat/V4_xbd_chap04.html#tag_21_04_16
 * [Unix Time]: https://en.wikipedia.org/wiki/Unix_time
 */
export function now(): Datetime;
/**
 * Query the resolution of the clock.
 * 
 * The nanoseconds field of the output is always less than 1000000000.
 */
export function resolution(): Datetime;
/**
 * A time and date in seconds plus nanoseconds.
 */
export interface Datetime {
  seconds: bigint,
  nanoseconds: number,
}
