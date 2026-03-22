export interface Transport {
  fetch: (path: string, init?: RequestInit) => Promise<Response>;
}

export class DefaultTransport implements Transport {
  constructor(
    private readonly base: URL | undefined,
    private readonly headers?: HeadersInit,
  ) {}

  async fetch(path: string, init?: RequestInit): Promise<Response> {
    const response = await fetch(this.base ? new URL(path, this.base) : path, {
      ...init,
      headers: this.headers
        ? {
            // NOTE: user-provided headers first to avoid them accidentally
            // overriding critical headers like credentials and content-type.
            ...this.headers,
            ...init?.headers,
          }
        : init?.headers,
    });

    return response;
  }
}
