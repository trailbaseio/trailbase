declare module 'trailbase:component/sqlite-function-endpoint@0.1.0' {
  export function dispatchScalarFunction(args: Arguments): Value;
  /**
   * WARNING: Evolving a variant currently breaks the ABI:
   *   https://github.com/WebAssembly/component-model/issues/454
   */
  export type Error = ErrorOther;
  export interface ErrorOther {
    tag: 'other',
    val: string,
  }
  export type Value = ValueNull | ValueText | ValueBlob | ValueInteger | ValueReal;
  export interface ValueNull {
    tag: 'null',
  }
  export interface ValueText {
    tag: 'text',
    val: string,
  }
  export interface ValueBlob {
    tag: 'blob',
    val: Uint8Array,
  }
  export interface ValueInteger {
    tag: 'integer',
    val: bigint,
  }
  export interface ValueReal {
    tag: 'real',
    val: number,
  }
  export interface Arguments {
    functionName: string,
    arguments: Array<Value>,
  }
}
