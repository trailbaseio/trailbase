declare module "trailbase:database/sqlite@0.2.0" {
  /**
   * WARNING: Evolving a variant currently breaks the ABI:
   *   https://github.com/WebAssembly/component-model/issues/454
   */
  export type TxError = TxErrorOther;
  export interface TxErrorOther {
    tag: "other";
    val: string;
  }
  export type Value =
    | ValueNull
    | ValueText
    | ValueBlob
    | ValueInteger
    | ValueReal;
  export interface ValueNull {
    tag: "null";
  }
  export interface ValueText {
    tag: "text";
    val: string;
  }
  export interface ValueBlob {
    tag: "blob";
    val: Uint8Array;
  }
  export interface ValueInteger {
    tag: "integer";
    val: bigint;
  }
  export interface ValueReal {
    tag: "real";
    val: number;
  }

  export class Transaction implements Disposable {
    constructor();
    begin(): void;
    commit(): void;
    rollback(): void;
    execute(query: string, params: Array<Value>): bigint;
    query(query: string, params: Array<Value>): Array<Array<Value>>;
    [Symbol.dispose](): void;
  }
}
