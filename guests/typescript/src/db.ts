import {
  txBegin,
  txCommit,
  txRollback,
  txExecute,
  txQuery,
} from "trailbase:runtime/host-endpoint";

import type { SqliteRequest } from "@common/SqliteRequest";
import type { Value } from "@/value";

import { JsonValue } from "@common/serde_json/JsonValue";
import { fromJsonValue, toJsonValue, toWitValue, fromWitValue } from "@/value";

export type { Value as DbValue } from "trailbase:runtime/host-endpoint";

// export class DbError extends Error {
//   readonly error: TxError;
//
//   constructor(error: TxError) {
//     super(`${error}`);
//     this.error = error;
//   }
//
//   public override toString(): string {
//     return `DbError(${this.error})`;
//   }
// }

export class Transaction {
  constructor() {
    txBegin();
  }

  query(query: string, params: Value[]): Value[][] {
    return txQuery(query, params.map(toWitValue)).map((row) =>
      row.map(fromWitValue),
    );
  }

  execute(query: string, params: Value[]): number {
    return Number(txExecute(query, params.map(toWitValue)));
  }

  commit(): void {
    txCommit();
  }
  rollback(): void {
    txRollback();
  }
}

export async function query(
  query: string,
  params: Value[],
): Promise<Value[][]> {
  const body: SqliteRequest = {
    query,
    params: params.map(toJsonValue),
  };
  const reply = await fetch("http://__sqlite/query", {
    method: "POST",
    headers: [["content-type", "application/json"]],
    body: JSON.stringify(body),
  });

  const json = await reply.json();
  if ("Error" in json) {
    const response = json as { Error: string };
    throw new Error(response.Error);
  }

  try {
    const response = json as { Query: { rows: Array<Array<JsonValue>> } };
    return response.Query.rows.map((row) => row.map(fromJsonValue));
  } catch (e) {
    throw new Error(`Unexpected response '${JSON.stringify(json)}': ${e}`);
  }
}

export async function execute(query: string, params: Value[]): Promise<number> {
  const body: SqliteRequest = {
    query,
    params: params.map(toJsonValue),
  };
  const reply = await fetch("http://__sqlite/execute", {
    method: "POST",
    headers: [["content-type", "application/json"]],
    body: JSON.stringify(body),
  });

  const json = await reply.json();
  if ("Error" in json) {
    const response = json as { Error: string };
    throw new Error(response.Error);
  }

  try {
    const response = json as { Execute: { rows_affected: number } };
    return response.Execute.rows_affected;
  } catch (e) {
    throw new Error(`Unexpected response '${JSON.stringify(json)}': ${e}`);
  }
}
