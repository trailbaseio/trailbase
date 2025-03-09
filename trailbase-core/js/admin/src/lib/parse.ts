import { adminFetch } from "@/lib/fetch";
import { urlSafeBase64Encode } from "trailbase";

import type { ParseRequest } from "@bindings/ParseRequest";
import type { ParseResponse } from "@bindings/ParseResponse";

async function fetchParse(request: ParseRequest): Promise<ParseResponse> {
  const response = await adminFetch("/parse", {
    method: "POST",
    body: JSON.stringify(request),
  });
  return await response.json();
}

export async function parseSql(sql: string): Promise<undefined | string> {
  const response = await fetchParse({
    query: urlSafeBase64Encode(sql),
    mode: "Expression",
  });

  return response.ok ? undefined : (response.message ?? "error");
}
