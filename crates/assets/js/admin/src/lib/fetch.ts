import { client } from "@/lib/client";

type FetchOptions = RequestInit & {
  throwOnError?: boolean;
};

export async function adminFetch(
  input: string,
  init?: FetchOptions,
): Promise<Response> {
  if (!input.startsWith("/")) {
    throw Error("Should start with '/'");
  }

  return await client.fetch(`/api/_admin${input}`, {
    headers: {
      "Content-Type": "application/json",
    },
    ...init,
  });
}
