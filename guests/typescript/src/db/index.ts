import * as JSON from "../json";

import {
  txBegin,
  txCommit,
  txRollback,
  txExecute,
  txQuery,
} from "trailbase:database/sqlite@0.1.0";

import type { SqliteRequest } from "@common/SqliteRequest";
import type { Value } from "./value";

import { SqlValue } from "@common/SqlValue";
import {
  fromJsonSqlValue,
  toJsonSqlValue,
  toWitValue,
  fromWitValue,
} from "./value";

export type { Value } from "trailbase:database/sqlite@0.1.0";

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
    params: params.map(toJsonSqlValue),
  };
  const reply = await fetch("http://__sqlite/query", {
    method: "POST",
    headers: [["content-type", "application/json"]],
    body: JSON.stringify(body),
  });

  const json = parseJSON(await reply.text());
  if ("Error" in json) {
    const response = json as { Error: string };
    throw new Error(response.Error);
  }

  try {
    const response = json as { Query: { rows: Array<Array<SqlValue>> } };
    return response.Query.rows.map((row) => row.map(fromJsonSqlValue));
  } catch (e) {
    throw new Error(`Unexpected response '${JSON.stringify(json)}': ${e}`);
  }
}

export async function execute(query: string, params: Value[]): Promise<number> {
  const body: SqliteRequest = {
    query,
    params: params.map(toJsonSqlValue),
  };
  const reply = await fetch("http://__sqlite/execute", {
    method: "POST",
    headers: [["content-type", "application/json"]],
    body: JSON.stringify(body),
  });

  const json = parseJSON(await reply.text());
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

// BigInt JSON stringify/parse shenanigans.
declare global {
  interface BigInt {
    toJSON(): unknown;
  }
}

BigInt.prototype.toJSON = function () {
  return JSON.rawJSON(this.toString());
};

function parseJSON(text: string) {
  function reviver(_key: string, value: unknown, context: { source: string }) {
    if (
      typeof value === "number" &&
      Number.isInteger(value) &&
      !Number.isSafeInteger(value)
    ) {
      // Ignore the value because it has already lost precision
      return BigInt(context.source);
    }
    return value;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return JSON.parse(text, reviver as any);
}
