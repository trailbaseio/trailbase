import { adminFetch } from "@/lib/fetch";

import type { QueryResponse } from "@bindings/QueryResponse";
import type { QueryRequest } from "@bindings/QueryRequest";

export type ExecutionError = {
  code: number;
  message: string;
};

export type ExecutionResult = {
  query: string;
  timestamp: number;

  data?: QueryResponse;
  error?: ExecutionError;
};

export async function executeSql(sql: string): Promise<ExecutionResult> {
  const response = await adminFetch("/query", {
    method: "POST",
    body: JSON.stringify({
      query: sql,
    } as QueryRequest),
    throwOnError: false,
  });

  if (response.ok) {
    return {
      query: sql,
      timestamp: Date.now(),
      data: await response.json(),
    } as ExecutionResult;
  }

  return {
    query: sql,
    timestamp: Date.now(),
    error: {
      code: response.status,
      message: await response.text(),
    } as ExecutionError,
  } as ExecutionResult;
}
