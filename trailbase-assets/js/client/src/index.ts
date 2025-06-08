import { jwtDecode } from "jwt-decode";

import type { ChangeEmailRequest } from "@bindings/ChangeEmailRequest";
import type { LoginRequest } from "@bindings/LoginRequest";
import type { LoginResponse } from "@bindings/LoginResponse";
import type { LoginStatusResponse } from "@bindings/LoginStatusResponse";
import type { LogoutRequest } from "@bindings/LogoutRequest";
import type { RefreshRequest } from "@bindings/RefreshRequest";
import type { RefreshResponse } from "@bindings/RefreshResponse";

export type User = {
  id: string;
  email: string;
};

export type Pagination = {
  cursor?: string;
  limit?: number;
  offset?: number;
};

export type ListResponse<T> = {
  cursor?: string;
  records: T[];
  total_count?: number;
};

export type Tokens = {
  auth_token: string;
  refresh_token: string | null;
  csrf_token: string | null;
};

type TokenClaims = {
  sub: string;
  iat: number;
  exp: number;
  email: string;
  csrf_token: string;
};

type TokenState = {
  state?: {
    tokens: Tokens;
    claims: TokenClaims;
  };
  headers: HeadersInit;
};

export type Event =
  | { Insert: object }
  | { Update: object }
  | { Delete: object }
  | { Error: string };

function buildTokenState(tokens?: Tokens): TokenState {
  return {
    state: tokens && {
      tokens,
      claims: jwtDecode(tokens.auth_token),
    },
    headers: headers(tokens),
  };
}

function buildUser(state: TokenState): User | undefined {
  const claims = state.state?.claims;
  if (claims) {
    return {
      id: claims.sub,
      email: claims.email,
    };
  }
}

function isExpired(state: TokenState): boolean {
  const claims = state.state?.claims;
  if (claims) {
    const now = Date.now() / 1000;
    if (claims.exp < now) {
      return true;
    }
  }

  return false;
}

/// Returns the refresh token if should refresh.
function shouldRefresh(tokenState: TokenState): string | undefined {
  const state = tokenState.state;
  if (state && state.claims.exp - 60 < Date.now() / 1000) {
    return state.tokens?.refresh_token ?? undefined;
  }
}

type FetchOptions = RequestInit & {
  throwOnError?: boolean;
};

export class FetchError extends Error {
  public status: number;

  constructor(status: number, msg: string) {
    super(msg);
    this.status = status;
  }

  static async from(response: Response): Promise<FetchError> {
    let body: string | undefined;
    try {
      body = await response.text();
    } catch {}

    console.debug(response);

    return new FetchError(
      response.status,
      body ? `${response.statusText}: ${body}` : response.statusText,
    );
  }

  public isClient(): boolean {
    return this.status >= 400 && this.status < 500;
  }

  public isServer(): boolean {
    return this.status >= 500;
  }

  public toString(): string {
    return `[${this.status}] ${this.message}`;
  }
}

export interface FileUpload {
  content_type?: null | string;
  filename?: null | string;
  mime_type?: null | string;
  objectstore_path: string;
}

type CompareOp =
  | "equal"
  | "notEqual"
  | "lessThan"
  | "lessThanEqual"
  | "greaterThan"
  | "greaterThanEqual"
  | "like"
  | "regexp";

function formatCompareOp(op: CompareOp): string {
  switch (op) {
    case "equal":
      return "$eq";
    case "notEqual":
      return "$ne";
    case "lessThan":
      return "$lt";
    case "lessThanEqual":
      return "$lte";
    case "greaterThan":
      return "$gt";
    case "greaterThanEqual":
      return "$gte";
    case "like":
      return "$like";
    case "regexp":
      return "$re";
  }
}

type Filter = {
  column: string;
  op?: CompareOp;
  value: string;
};

type And = {
  and: FilterOrComposite[];
};

type Or = {
  or: FilterOrComposite[];
};

type FilterOrComposite = Filter | And | Or;

/// Provides CRUD access to records through TrailBase's record API.
///
/// TODO: add file upload/download.
export class RecordApi {
  private readonly _path: string;

  constructor(
    private readonly client: Client,
    private readonly name: string,
  ) {
    this._path = `${recordApiBasePath}/${this.name}`;
  }

  public async list<T = Record<string, unknown>>(opts?: {
    pagination?: Pagination;
    order?: string[];
    filters?: FilterOrComposite[];
    count?: boolean;
    expand?: string[];
  }): Promise<ListResponse<T>> {
    const params = new URLSearchParams();
    const pagination = opts?.pagination;
    if (pagination) {
      const cursor = pagination.cursor;
      if (cursor) params.append("cursor", cursor);

      const limit = pagination.limit;
      if (limit) params.append("limit", limit.toString());

      const offset = pagination.offset;
      if (offset) params.append("offset", offset.toString());
    }
    const order = opts?.order;
    if (order) params.append("order", order.join(","));

    if (opts?.count) params.append("count", "true");

    const expand = opts?.expand;
    if (expand) params.append("expand", expand.join(","));

    function traverseFilters(path: string, filter: FilterOrComposite) {
      if ("and" in filter) {
        for (const [i, f] of (filter as And).and.entries()) {
          traverseFilters(`${path}[$and][${i}]`, f);
        }
      } else if ("or" in filter) {
        for (const [i, f] of (filter as Or).or.entries()) {
          traverseFilters(`${path}[$or][${i}]`, f);
        }
      } else {
        const f = filter as Filter;
        const op = f.op;
        if (op) {
          params.append(
            `${path}[${f.column}][${formatCompareOp(op)}]`,
            f.value,
          );
        } else {
          params.append(`${path}[${f.column}]`, f.value);
        }
      }
    }

    const filters = opts?.filters;
    if (filters) {
      for (const filter of filters) {
        traverseFilters("filter", filter);
      }
    }

    const response = await this.client.fetch(`${this._path}?${params}`);
    return (await response.json()) as ListResponse<T>;
  }

  public async read<T = Record<string, unknown>>(
    id: string | number,
    opt?: {
      expand?: string[];
    },
  ): Promise<T> {
    const expand = opt?.expand;
    const response = await this.client.fetch(
      expand
        ? `${this._path}/${id}?expand=${expand.join(",")}`
        : `${this._path}/${id}`,
    );
    return (await response.json()) as T;
  }

  public async create<T = Record<string, unknown>>(
    record: T,
  ): Promise<string | number> {
    const response = await this.client.fetch(this._path, {
      method: "POST",
      body: JSON.stringify(record),
      headers: jsonContentTypeHeader,
    });

    return (await response.json()).ids[0];
  }

  public async createBulk<T = Record<string, unknown>>(
    records: T[],
  ): Promise<(string | number)[]> {
    const response = await this.client.fetch(this._path, {
      method: "POST",
      body: JSON.stringify(records),
      headers: jsonContentTypeHeader,
    });

    return (await response.json()).ids;
  }

  public async update<T = Record<string, unknown>>(
    id: string | number,
    record: Partial<T>,
  ): Promise<void> {
    await this.client.fetch(`${this._path}/${id}`, {
      method: "PATCH",
      body: JSON.stringify(record),
      headers: jsonContentTypeHeader,
    });
  }

  public async delete(id: string | number): Promise<void> {
    await this.client.fetch(`${this._path}/${id}`, {
      method: "DELETE",
    });
  }

  public async subscribe(id: string | number): Promise<ReadableStream<Event>> {
    const response = await this.client.fetch(`${this._path}/subscribe/${id}`);
    const body = response.body;
    if (!body) {
      throw Error("Subscription reader is null.");
    }

    const decoder = new TextDecoder();
    const transformStream = new TransformStream<Uint8Array, Event>({
      transform(chunk: Uint8Array, controller) {
        const msgs = decoder.decode(chunk).trimEnd().split("\n\n");
        for (const msg of msgs) {
          if (msg.startsWith("data: ")) {
            controller.enqueue(JSON.parse(msg.substring(6)));
          }
        }
      },
      flush(controller) {
        controller.terminate();
      },
    });

    return body.pipeThrough(transformStream);
  }
}

class ThinClient {
  constructor(public readonly base: URL | undefined) {}

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

type ClientOptions = {
  tokens?: Tokens;
  onAuthChange?: (client: Client, user?: User) => void;
};

/// Client for interacting with TrailBase auth and record APIs.
export class Client {
  private readonly _client: ThinClient;
  private readonly _authChange:
    | undefined
    | ((client: Client, user?: User) => void);
  private _tokenState: TokenState;

  constructor(baseUrl: URL | string | undefined, opts?: ClientOptions) {
    this._client = new ThinClient(baseUrl ? new URL(baseUrl) : undefined);
    this._authChange = opts?.onAuthChange;

    const tokens = opts?.tokens;
    // Note: this is a double assignment to _tokenState to ensure the linter
    // that it's really initialized in the constructor.
    this._tokenState = this.setTokenState(buildTokenState(tokens), true);

    if (tokens?.refresh_token !== undefined) {
      // Validate session. This is currently async, which allows to initialize
      // a Client synchronously from invalid tokens. We may want to consider
      // offering a safer async initializer to avoid "racy" behavior. Especially,
      // when the auth token is valid while the session has already been closed.
      this.checkAuthStatus()
        .then((tokens) => {
          if (tokens === undefined) {
            // In this case, the auth state has changed, so we should invoke the callback.
            this.setTokenState(buildTokenState(undefined), false);
          } else {
            // In this case, the auth state has remained the same, we're merely
            // updating the reminted auth token.
            this.setTokenState(buildTokenState(tokens), true);
          }
        })
        .catch(console.error);
    }
  }

  public static init(site?: URL | string, opts?: ClientOptions): Client {
    return new Client(site, opts);
  }

  public static async tryFromCookies(
    site?: URL | string,
    opts?: ClientOptions,
  ): Promise<Client> {
    const client = new Client(site, opts);

    // Prefer explicit tokens. When given, do not update/refresh infinite recursion
    // with `($token) => Client` factories.
    if (!client.tokens()) {
      try {
        await client.checkCookies();
      } catch (err) {
        console.debug("No valid cookies found: ", err);
      }
    }

    return client;
  }

  public get base(): URL | undefined {
    return this._client.base;
  }

  /// Low-level access to tokens (auth, refresh, csrf) useful for persisting them.
  public tokens = (): Tokens | undefined => this._tokenState?.state?.tokens;

  /// Provides current user.
  public user = (): User | undefined => buildUser(this._tokenState);

  /// Provides current user.
  public headers = (): HeadersInit => this._tokenState.headers;

  /// Construct accessor for Record API with given name.
  public records = (name: string): RecordApi => new RecordApi(this, name);

  public avatarUrl(userId?: string): string | undefined {
    const id = userId ?? this.user()?.id;
    if (id) {
      return `${authApiBasePath}/avatar/${id}`;
    }
    return undefined;
  }

  public async login(email: string, password: string): Promise<void> {
    const response = await this.fetch(`${authApiBasePath}/login`, {
      method: "POST",
      body: JSON.stringify({
        email: email,
        password: password,
      } as LoginRequest),
      headers: jsonContentTypeHeader,
    });

    this.setTokenState(
      buildTokenState((await response.json()) as LoginResponse),
    );
  }

  public async logout(): Promise<boolean> {
    try {
      const refresh_token = this._tokenState.state?.tokens.refresh_token;
      if (refresh_token) {
        await this.fetch(`${authApiBasePath}/logout`, {
          method: "POST",
          body: JSON.stringify({
            refresh_token,
          } as LogoutRequest),
          headers: jsonContentTypeHeader,
        });
      } else {
        await this.fetch(`${authApiBasePath}/logout`);
      }
    } catch (err) {
      console.debug(err);
    }
    this.setTokenState(buildTokenState(undefined));
    return true;
  }

  public async deleteUser(): Promise<void> {
    await this.fetch(`${authApiBasePath}/delete`);
    this.setTokenState(buildTokenState(undefined));
  }

  public async changeEmail(email: string): Promise<void> {
    await this.fetch(`${authApiBasePath}/change_email`, {
      method: "POST",
      body: JSON.stringify({
        new_email: email,
      } as ChangeEmailRequest),
      headers: jsonContentTypeHeader,
    });
  }

  /// This will call the status endpoint, which validates any provided tokens
  /// but also hoists any tokens provided as cookies into a JSON response.
  private async checkAuthStatus(): Promise<Tokens | undefined> {
    const response = await this.fetch(`${authApiBasePath}/status`, {
      throwOnError: false,
    });
    if (response.ok) {
      const status: LoginStatusResponse = await response.json();
      const auth_token = status.auth_token;
      if (auth_token) {
        return {
          auth_token,
          refresh_token: status.refresh_token,
          csrf_token: status.csrf_token,
        };
      }
    }
    return undefined;
  }

  public async checkCookies(): Promise<Tokens | undefined> {
    const tokens = await this.checkAuthStatus();
    if (tokens) {
      const newState = buildTokenState(tokens);
      this.setTokenState(newState);
      return newState.state?.tokens;
    }
  }

  public async refreshAuthToken(): Promise<void> {
    const refreshToken = shouldRefresh(this._tokenState);
    if (refreshToken) {
      // Note: refreshTokenImpl will auto-logout on 401.
      this.setTokenState(await this.refreshTokensImpl(refreshToken));
    }
  }

  private async refreshTokensImpl(refreshToken: string): Promise<TokenState> {
    const response = await this._client.fetch(
      `${authApiBasePath}/refresh`,
      this._tokenState.headers,
      {
        method: "POST",
        body: JSON.stringify({
          refresh_token: refreshToken,
        } as RefreshRequest),
        headers: jsonContentTypeHeader,
      },
    );

    if (!response.ok) {
      if (response.status === 401) {
        this.logout();
      }
      throw await FetchError.from(response);
    }

    return buildTokenState({
      ...((await response.json()) as RefreshResponse),
      refresh_token: refreshToken,
    });
  }

  private setTokenState(
    state: TokenState,
    skipCb: boolean = false,
  ): TokenState {
    this._tokenState = state;
    if (!skipCb) {
      this._authChange?.(this, buildUser(state));
    }

    if (isExpired(state)) {
      // This can happen on initial construction, i.e. if a client is
      // constructed from older, persisted tokens.
      console.debug(`Set token state (expired)`);
    }

    return this._tokenState;
  }

  /// Fetches data from TrailBase endpoints, e.g.:
  ///    const response = await client.fetch("/api/auth/v1/status");
  ///
  /// Unlike native fetch, will throw in case !response.ok.
  public async fetch(path: string, init?: FetchOptions): Promise<Response> {
    let tokenState = this._tokenState;
    const refreshToken = shouldRefresh(tokenState);
    if (refreshToken) {
      tokenState = this.setTokenState(
        await this.refreshTokensImpl(refreshToken),
      );
    }

    try {
      const response = await this._client.fetch(path, tokenState.headers, init);
      if (!response.ok && (init?.throwOnError ?? true)) {
        throw await FetchError.from(response);
      }
      return response;
    } catch (err) {
      if (err instanceof TypeError) {
        console.debug(`Connection refused ${err}. TrailBase down or CORS?`);
      }
      throw err;
    }
  }
}

const recordApiBasePath = "/api/records/v1";
const authApiBasePath = "/api/auth/v1";

export function filePath(
  apiName: string,
  recordId: string | number,
  columnName: string,
): string {
  return `${recordApiBasePath}/${apiName}/${recordId}/file/${columnName}`;
}

export function filesPath(
  apiName: string,
  recordId: string | number,
  columnName: string,
  index: number,
): string {
  return `${recordApiBasePath}/${apiName}/${recordId}/files/${columnName}/${index}`;
}

function _isDev(): boolean {
  type ImportMeta = {
    env: object | undefined;
  };
  const env = (import.meta as unknown as ImportMeta).env;
  const key = "DEV" as keyof typeof env;
  const isDev = env?.[key] ?? false;

  return isDev;
}
const isDev = _isDev();

function headers(tokens?: Tokens): HeadersInit {
  if (tokens) {
    const { auth_token, refresh_token, csrf_token } = tokens;
    return {
      ...(auth_token && {
        Authorization: `Bearer ${auth_token}`,
      }),
      ...(refresh_token && {
        "Refresh-Token": refresh_token,
      }),
      ...(csrf_token && {
        "CSRF-Token": csrf_token,
      }),
    };
  }

  return {};
}

const jsonContentTypeHeader = {
  "Content-Type": "application/json",
};

export function textEncode(s: string): Uint8Array {
  return new TextEncoder().encode(s);
}

export function textDecode(ar: Uint8Array): string {
  return new TextDecoder().decode(ar);
}

/// Decode a base64 string to bytes.
export function base64Decode(base64: string): string {
  return atob(base64);
}

/// Decode a "url-safe" base64 string to bytes.
export function urlSafeBase64Decode(base64: string): string {
  return base64Decode(base64.replace(/_/g, "/").replace(/-/g, "+"));
}

/// Encode an arbitrary string input as base64 string.
export function base64Encode(s: string): string {
  return btoa(s);
}

/// Encode an arbitrary string input as a "url-safe" base64 string.
export function urlSafeBase64Encode(s: string): string {
  return base64Encode(s).replace(/\//g, "_").replace(/\+/g, "-");
}

export function asyncBase64Encode(blob: Blob): Promise<string> {
  return new Promise((resolve, _) => {
    const reader = new FileReader();
    reader.onloadend = () => resolve(reader.result as string);
    reader.readAsDataURL(blob);
  });
}

export const exportedForTesting = isDev
  ? {
      base64Decode,
      base64Encode,
    }
  : undefined;
