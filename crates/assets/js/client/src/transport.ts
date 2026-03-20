import { isDev } from "./constants";

export interface Transport {
  fetch: (
    path: string,
    headers: HeadersInit,
    init?: RequestInit,
  ) => Promise<Response>;
}

export class ThinClient implements Transport {
  constructor(private readonly base: URL | undefined) {}

  async fetch(
    path: string,
    headers: HeadersInit,
    init?: RequestInit,
  ): Promise<Response> {
    // NOTE: We need to merge the headers in such a complicated fashion
    // to avoid user-provided `init` with headers unintentionally suppressing
    // the credentials.
    const response = await fetch(this.base ? new URL(path, this.base) : path, {
      credentials: isDev ? "include" : "same-origin",
      ...init,
      headers: init
        ? {
            ...headers,
            ...init?.headers,
          }
        : headers,
    });

    return response;
  }
}
