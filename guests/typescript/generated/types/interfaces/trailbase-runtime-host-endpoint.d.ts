/** @module Interface trailbase:runtime/host-endpoint **/
/**
 * NOTE: Ideally, we'd use these but they currently block guests.
 */
export function execute(query: string, params: Array<Value>): bigint;
export function query(query: string, params: Array<Value>): Array<Array<Value>>;
/**
 * However, transactions have to be sync.
 */
export function txBegin(): void;
export function txCommit(): void;
export function txRollback(): void;
export function txExecute(query: string, params: Array<Value>): bigint;
export function txQuery(query: string, params: Array<Value>): Array<Array<Value>>;
export type TxError = TxErrorOther;
export interface TxErrorOther {
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
