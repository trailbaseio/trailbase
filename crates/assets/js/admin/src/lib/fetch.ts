import { client } from "@/lib/client";
import { FetchError } from "trailbase";

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

  try {
    return await client.fetch(`/api/_admin${input}`, {
      headers: {
        "Content-Type": "application/json",
      },
      ...init,
    });
  } catch (err) {
    // Handle token and thus permission issues by redirecting users to the login screen.
    if (
      err instanceof FetchError &&
      (err.status === 401 || err.status === 403)
    ) {
      console.info(
        `Fetch failed (${err.status}), user is being logged out and should be redirected to login.`,
      );
      client.logout();
    }
    throw err;
  }
}
