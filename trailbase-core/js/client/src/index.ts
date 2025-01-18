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
};

export type ListResponse<T> = {
  cursor?: string;
  records: T[];
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

    console.warn(response);

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

/// Provides CRUD access to records through TrailBase's record API.
///
/// TODO: add file upload/download.
export class RecordApi {
  private static readonly _recordApi = "api/records/v1";
  private readonly _createApi: string;

  constructor(
    private readonly client: Client,
    private readonly name: string,
  ) {
    this._createApi = `${RecordApi._recordApi}/${this.name}`;
  }

  public async list<T = Record<string, unknown>>(opts?: {
    pagination?: Pagination;
    order?: string[];
    filters?: string[];
  }): Promise<ListResponse<T>> {
    const params = new URLSearchParams();
    const pagination = opts?.pagination;
    if (pagination) {
      const cursor = pagination.cursor;
      if (cursor) params.append("cursor", cursor);

      const limit = pagination.limit;
      if (limit) params.append("limit", limit.toString());
    }
    const order = opts?.order;
    if (order) params.append("order", order.join(","));

    const filters = opts?.filters;
    if (filters) {
      for (const filter of filters) {
        const pos = filter.indexOf("=");
        if (pos <= 0) {
          throw Error(`Filter '${filter}' does not match: 'name[op]=value'`);
        }
        const nameOp = filter.slice(0, pos);
        const value = filter.slice(pos + 1);
        params.append(nameOp, value);
      }
    }

    const response = await this.client.fetch(
      `${RecordApi._recordApi}/${this.name}?${params}`,
    );
    return (await response.json()) as ListResponse<T>;
  }

  public async read<T = Record<string, unknown>>(
    id: string | number,
  ): Promise<T> {
    const response = await this.client.fetch(
      `${RecordApi._recordApi}/${this.name}/${id}`,
    );
    return (await response.json()) as T;
  }

  public async create<T = Record<string, unknown>>(
    record: T,
  ): Promise<Response> {
    return this.client.fetch(this._createApi, {
      method: "POST",
      body: JSON.stringify(record),
    });
  }

  public async createId<T = Record<string, unknown>>(
    record: T,
  ): Promise<string | number> {
    const response = await this.create(record);
    return (await response.json()).id;
  }

  public async update<T = Record<string, unknown>>(
    id: string | number,
    record: Partial<T>,
  ): Promise<void> {
    await this.client.fetch(`${RecordApi._recordApi}/${this.name}/${id}`, {
      method: "PATCH",
      body: JSON.stringify(record),
    });
  }

  public async delete(id: string | number): Promise<void> {
    await this.client.fetch(`${RecordApi._recordApi}/${this.name}/${id}`, {
      method: "DELETE",
    });
  }

  public async subscribe(id: string | number): Promise<ReadableStream<Event>> {
    const response = await this.client.fetch(
      `${RecordApi._recordApi}/${this.name}/subscribe/${id}`,
    );
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

  public imageUri(id: string | number, colName: string): string {
    return `${this.client.site}/${RecordApi._recordApi}/${this.name}/${id}/file/${colName}`;
  }

  public imagesUri(
    id: string | number,
    colName: string,
    index: number,
  ): string {
    return `${this.client.site}/${RecordApi._recordApi}/${this.name}/${id}/files/${colName}/${index}`;
  }
}

class ThinClient {
  constructor(public readonly site: string) {}

  public async fetch(
    path: string,
    tokenState: TokenState,
    init?: RequestInit,
  ): Promise<Response> {
    if (path.startsWith("/")) {
      throw Error("Path starts with '/'. Relative path expected.");
    }

    const response = await fetch(`${this.site}/${path}`, {
      ...init,
      credentials: isDev ? "include" : "same-origin",
      headers: tokenState.headers,
    });

    return response;
  }
}

type ClientOptions = {
  tokens?: Tokens;
  onAuthChange?: (client: Client, user?: User) => void;
};

/// Client for interacting with TrailBase auth and record APIs.
///
/// TODO: Add
///  * issue_password_reset_email
///  * issue_change_email
///  * status
export class Client {
  private static readonly _authApi = "api/auth/v1";
  private static readonly _authUi = "_/auth";

  private readonly _client: ThinClient;
  private readonly _authChange:
    | undefined
    | ((client: Client, user?: User) => void);
  private _tokenState: TokenState;

  constructor(site: string, opts?: ClientOptions) {
    this._client = new ThinClient(site);
    this._authChange = opts?.onAuthChange;

    this._tokenState = this.updateTokens(opts?.tokens);
  }

  public static init(site: string, opts?: ClientOptions): Client {
    return new Client(site, opts);
  }

  public static async tryFromCookies(
    site: string,
    opts?: ClientOptions,
  ): Promise<Client> {
    const client = new Client(site, opts);

    // Prefer explicit tokens. When given, do not update/refresh infinite recursion
    // with `($token) => Client` factories.
    if (!client.tokens()) {
      try {
        const response = await client.fetch(`${Client._authApi}/status`);
        const status: LoginStatusResponse = await response.json();

        const authToken = status?.auth_token;
        if (authToken) {
          client.updateTokens({
            auth_token: authToken,
            refresh_token: status.refresh_token,
            csrf_token: status.csrf_token,
          });
        }
      } catch (err) {
        console.debug("No valid cookies found: ", err);
      }
    }

    return client;
  }

  private updateTokens(tokens?: Tokens): TokenState {
    const state = buildTokenState(tokens);

    this._tokenState = state;
    this._authChange?.(this, this.user());

    const claims = state.state?.claims;
    if (claims) {
      const now = Date.now() / 1000;
      if (claims.exp < now) {
        console.warn("Token expired");
      }
    }

    return state;
  }

  public get site() {
    return this._client.site;
  }

  /// Low-level access to tokens (auth, refresh, csrf) useful for persisting them.
  public tokens = (): Tokens | undefined => this._tokenState?.state?.tokens;

  /// Provides current user.
  public user(): User | undefined {
    const claims = this._tokenState.state?.claims;
    if (claims) {
      return {
        id: claims.sub,
        email: claims.email,
      };
    }
  }
  public records = (name: string): RecordApi => new RecordApi(this, name);

  public async avatarUrl(): Promise<string | undefined> {
    const user = this.user();
    if (user) {
      const response = await this.fetch(`${Client._authApi}/avatar/${user.id}`);
      const json = (await response.json()) as { avatar_url: string };
      return json.avatar_url;
    }
    return undefined;
  }

  public async login(email: string, password: string): Promise<void> {
    const response = await this.fetch(`${Client._authApi}/login`, {
      method: "POST",
      body: JSON.stringify({
        email: email,
        password: password,
      } as LoginRequest),
    });

    this.updateTokens((await response.json()) as LoginResponse);
  }

  public loginUri(redirect?: string): string {
    return `${this._client.site}/${Client._authUi}/login?${redirect ? `redirect_to=${redirect}` : ""}`;
  }

  public async logout(): Promise<boolean> {
    try {
      const refresh_token = this._tokenState.state?.tokens.refresh_token;
      if (refresh_token) {
        await this.fetch(`${Client._authApi}/logout`, {
          method: "POST",
          body: JSON.stringify({
            refresh_token,
          } as LogoutRequest),
        });
      } else {
        await this.fetch(`${Client._authApi}/logout`);
      }
    } catch (err) {
      console.warn(err);
    }
    this.updateTokens(undefined);
    return true;
  }

  public logoutUri(redirect?: string): string {
    return `${this._client.site}/${Client._authApi}/logout?${redirect ? `redirect_to=${redirect}` : ""}`;
  }

  public async deleteUser(): Promise<void> {
    await this.fetch(`${Client._authApi}/delete`);
    this.updateTokens(undefined);
  }

  public async changeEmail(email: string): Promise<void> {
    await this.fetch(`${Client._authApi}/change_email`, {
      method: "POST",
      body: JSON.stringify({
        new_email: email,
      } as ChangeEmailRequest),
    });
  }

  public async refreshAuthToken(): Promise<void> {
    const refreshToken = Client.shouldRefresh(this._tokenState);
    if (refreshToken) {
      this._tokenState = await this.refreshTokensImpl(refreshToken);
    }
  }

  /// Returns the refresh token if should refresh.
  private static shouldRefresh(tokenState: TokenState): string | undefined {
    const state = tokenState.state;
    if (state && state.claims.exp - 60 < Date.now() / 1000) {
      return state.tokens?.refresh_token ?? undefined;
    }
  }

  private async refreshTokensImpl(refreshToken: string): Promise<TokenState> {
    const response = await this._client.fetch(
      `${Client._authApi}/refresh`,
      this._tokenState,
      {
        method: "POST",
        body: JSON.stringify({
          refresh_token: refreshToken,
        } as RefreshRequest),
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

  /// Fetches data from TrailBase endpoints, e.g.:
  //    const response = await client.fetch("api/auth/v1/status");
  //
  //  Unlike native fetch, will throw in case !response.ok.
  public async fetch(path: string, init?: FetchOptions): Promise<Response> {
    let tokenState = this._tokenState;
    const refreshToken = Client.shouldRefresh(tokenState);
    if (refreshToken) {
      this._tokenState = tokenState =
        await this.refreshTokensImpl(refreshToken);
    }

    try {
      const response = await this._client.fetch(path, tokenState, init);
      if (!response.ok && (init?.throwOnError ?? true)) {
        throw await FetchError.from(response);
      }
      return response;
    } catch (err) {
      if (err instanceof TypeError) {
        throw Error(`Connection refused ${err}. TrailBase down or CORS?`);
      }
      throw err;
    }
  }
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

export function headers(tokens?: Tokens): HeadersInit {
  const base = {
    "Content-Type": "application/json",
  };

  if (tokens) {
    const { auth_token, refresh_token, csrf_token } = tokens;
    return {
      ...base,
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

  return base;
}

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
