/** @module Interface wasi:random/random@0.2.3 **/
/**
 * Return `len` cryptographically-secure random or pseudo-random bytes.
 * 
 * This function must produce data at least as cryptographically secure and
 * fast as an adequately seeded cryptographically-secure pseudo-random
 * number generator (CSPRNG). It must not block, from the perspective of
 * the calling program, under any circumstances, including on the first
 * request and on requests for numbers of bytes. The returned data must
 * always be unpredictable.
 * 
 * This function must always return fresh data. Deterministic environments
 * must omit this function, rather than implementing it with deterministic
 * data.
 */
export function getRandomBytes(len: bigint): Uint8Array;
/**
 * Return a cryptographically-secure random or pseudo-random `u64` value.
 * 
 * This function returns the same type of data as `get-random-bytes`,
 * represented as a `u64`.
 */
export function getRandomU64(): bigint;
