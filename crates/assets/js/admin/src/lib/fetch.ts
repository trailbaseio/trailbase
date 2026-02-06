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
      console.debug(
        `Permission denied. Log user out to redirected to login: ${err}`,
      );
      client.logout();

      // NOTE: Not very useful other than push the exception upstream when JSON deserialization fails.
      // Return error response as opposed to uncaught err.
      // return Response.error();
    }
    throw err;
  }
}
