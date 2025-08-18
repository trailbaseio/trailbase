// https://github.com/bytecodealliance/jco/blob/b703b2850d3170d786812a56f40456870c780311/packages/preview2-shim/types/interfaces/wasi-io-error.d.ts
export namespace WasiIoError {
  /**
   * Returns a string that is suitable to assist humans in debugging
   * this error.
   *
   * WARNING: The returned string should not be consumed mechanically!
   * It may change across platforms, hosts, or other implementation
   * details. Parsing this string is a major platform-compatibility
   * hazard.
   */
  export { Error };
}

export class Error {
  toDebugString(): string;
}
