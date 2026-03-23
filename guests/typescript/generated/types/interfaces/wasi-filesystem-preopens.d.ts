/// <reference path="./wasi-filesystem-types.d.ts" />
declare module "wasi:filesystem/preopens@0.2.3" {
  /**
   * Return the set of preopened directories, and their paths.
   */
  export function getDirectories(): Array<[Descriptor, string]>;
  export type Descriptor = import("wasi:filesystem/types@0.2.3").Descriptor;
}
