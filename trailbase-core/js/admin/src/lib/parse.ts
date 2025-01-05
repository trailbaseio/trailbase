import type { ParseRequest, ParseResponse } from "@/lib/bindings";
import { adminFetch } from "@/lib/fetch";
import { urlSafeBase64Encode } from "trailbase";

async function fetchParse(request: ParseRequest): Promise<ParseResponse> {
  const response = await adminFetch("/parse", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
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
