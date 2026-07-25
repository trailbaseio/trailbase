import { initClient } from "trailbase";
import type { Client, Tokens } from "trailbase";

export function adminClient(): Client | undefined {
  const authTokens: string | null = localStorage.getItem("auth_tokens");
  if (authTokens === null) {
    return;
  }

  const base = document.head.baseURI;
  const tokens: Tokens = JSON.parse(authTokens);
  return initClient(base, { tokens });
}
