/** @module Interface wasi:filesystem/preopens@0.2.3 **/
/**
 * Return the set of preopened directories, and their paths.
 */
export function getDirectories(): Array<[Descriptor, string]>;
export type Descriptor = import('./wasi-filesystem-types.js').Descriptor;
