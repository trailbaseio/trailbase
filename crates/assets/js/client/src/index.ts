import { jwtDecode } from "jwt-decode";
import * as JSON from "@ungap/raw-json";

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

export type FetchOptions = RequestInit & {
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

    return new FetchError(
      response.status,
      `FetchError(status: ${response.status} - ${response.statusText}, ${body})`,
    );
  }

  public isClient(): boolean {
    return this.status >= 400 && this.status < 500;
  }

  public isServer(): boolean {
    return this.status >= 500;
  }
}

export interface FileUpload {
  content_type?: null | string;
  filename?: null | string;
  mime_type?: null | string;
  objectstore_path: string;
}

export type CompareOp =
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

export type Filter = {
  column: string;
  op?: CompareOp;
  value: string;
};

export type And = {
  and: FilterOrComposite[];
};

export type Or = {
  or: FilterOrComposite[];
};

export type FilterOrComposite = Filter | And | Or;

export type RecordId = string | number;

// TODO: Use `ts-rs` generated types.
interface CreateOp {
  Create: {
    api_name: string;
    value: Record<string, unknown>;
  };
}

interface UpdateOp {
  Update: {
    api_name: string;
    record_id: RecordId;
    value: Record<string, unknown>;
  };
}

interface DeleteOp {
  Delete: {
    api_name: string;
    record_id: RecordId;
  };
}

export interface DeferredOperation<ResponseType> {
  query(): Promise<ResponseType>;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface DeferredMutation<
  ResponseType,
> extends DeferredOperation<ResponseType> {}

export class CreateOperation<
  T = Record<string, unknown>,
> implements DeferredMutation<RecordId> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly record: Partial<T>,
  ) {}

  async query(): Promise<RecordId> {
    const response = await this.client.fetch(
      `${recordApiBasePath}/${this.apiName}`,
      {
        method: "POST",
        body: JSON.stringify(this.record),
        headers: jsonContentTypeHeader,
      },
    );

    return parseJSON(await response.text()).ids[0];
  }

  protected toJSON(): CreateOp {
    return {
      Create: {
        api_name: this.apiName,
        value: this.record,
      },
    };
  }
}

export class UpdateOperation<
  T = Record<string, unknown>,
> implements DeferredMutation<void> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly id: RecordId,
    private readonly record: Partial<T>,
  ) {}

  async query(): Promise<void> {
    await this.client.fetch(`${recordApiBasePath}/${this.apiName}/${this.id}`, {
      method: "PATCH",
      body: JSON.stringify(this.record),
      headers: jsonContentTypeHeader,
    });
  }

  protected toJSON(): UpdateOp {
    return {
      Update: {
        api_name: this.apiName,
        record_id: this.id,
        value: this.record,
      },
    };
  }
}

export class DeleteOperation implements DeferredMutation<void> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly id: RecordId,
  ) {}
  async query(): Promise<void> {
    await this.client.fetch(`${recordApiBasePath}/${this.apiName}/${this.id}`, {
      method: "DELETE",
    });
  }

  protected toJSON(): DeleteOp {
    return {
      Delete: {
        api_name: this.apiName,
        record_id: this.id,
      },
    };
  }
}

export interface ReadOpts {
  expand?: string[];
}

export class ReadOperation<
  T = Record<string, unknown>,
> implements DeferredOperation<T> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly id: RecordId,
    private readonly opt?: ReadOpts,
  ) {}

  async query(): Promise<T> {
    const expand = this.opt?.expand;
    const response = await this.client.fetch(
      expand
        ? `${recordApiBasePath}/${this.apiName}/${this.id}?expand=${expand.join(",")}`
        : `${recordApiBasePath}/${this.apiName}/${this.id}`,
    );
    return parseJSON(await response.text()) as T;
  }
}

export interface ListOpts {
  pagination?: Pagination;
  order?: string[];
  filters?: FilterOrComposite[];
  count?: boolean;
  expand?: string[];
}

export class ListOperation<
  T = Record<string, unknown>,
> implements DeferredOperation<ListResponse<T>> {
  constructor(
    private readonly client: Client,
    private readonly apiName: string,
    private readonly opts?: ListOpts,
  ) {}

  async query(): Promise<ListResponse<T>> {
    const params = new URLSearchParams();
    const pagination = this.opts?.pagination;
    if (pagination) {
      const cursor = pagination.cursor;
      if (cursor) params.append("cursor", cursor);

      const limit = pagination.limit;
      if (limit) params.append("limit", limit.toString());

      const offset = pagination.offset;
      if (offset) params.append("offset", offset.toString());
    }
    const order = this.opts?.order;
    if (order) params.append("order", order.join(","));

    if (this.opts?.count) params.append("count", "true");

    const expand = this.opts?.expand;
    if (expand) params.append("expand", expand.join(","));

    const filters = this.opts?.filters;
    if (filters) {
      for (const filter of filters) {
        addFiltersToParams(params, "filter", filter);
      }
    }

    const response = await this.client.fetch(
      `${recordApiBasePath}/${this.apiName}?${params}`,
    );
    return parseJSON(await response.text()) as ListResponse<T>;
  }
}

export interface SubscribeOpts {
  filters?: FilterOrComposite[];
}

export interface RecordApi<T = Record<string, unknown>> {
  list(opts?: ListOpts): Promise<ListResponse<T>>;
  listOp(opts?: ListOpts): ListOperation<T>;

  read(id: RecordId, opt?: ReadOpts): Promise<T>;
  readOp(id: RecordId, opt?: ReadOpts): ReadOperation<T>;

  create(record: T): Promise<RecordId>;
  createOp(record: T): CreateOperation<T>;
  // TODO: Retire in favor of `client.execute`.
  createBulk(records: T[]): Promise<RecordId[]>;

  update(id: RecordId, record: Partial<T>): Promise<void>;
  updateOp(id: RecordId, record: Partial<T>): UpdateOperation;

  delete(id: RecordId): Promise<void>;
  deleteOp(id: RecordId): DeleteOperation;

  subscribe(id: RecordId): Promise<ReadableStream<Event>>;
  subscribeAll(opts?: SubscribeOpts): Promise<ReadableStream<Event>>;
}

/// Provides CRUD access to records through TrailBase's record API.
export class RecordApiImpl<
  T = Record<string, unknown>,
> implements RecordApi<T> {
  constructor(
    private readonly client: Client,
    private readonly name: string,
  ) {}

  public async list(opts?: ListOpts): Promise<ListResponse<T>> {
    return new ListOperation<T>(this.client, this.name, opts).query();
  }

  public listOp(opts?: ListOpts): ListOperation<T> {
    return new ListOperation<T>(this.client, this.name, opts);
  }

  public async read<T = Record<string, unknown>>(
    id: RecordId,
    opt?: ReadOpts,
  ): Promise<T> {
    return new ReadOperation<T>(this.client, this.name, id, opt).query();
  }

  public readOp(id: RecordId, opt?: ReadOpts): ReadOperation<T> {
    return new ReadOperation<T>(this.client, this.name, id, opt);
  }

  public async create(record: T): Promise<RecordId> {
    return new CreateOperation<T>(this.client, this.name, record).query();
  }

  public createOp(record: T): CreateOperation<T> {
    return new CreateOperation<T>(this.client, this.name, record);
  }
  public async createBulk<T = Record<string, unknown>>(
    records: T[],
  ): Promise<RecordId[]> {
    const response = await this.client.fetch(
      `${recordApiBasePath}/${this.name}`,
      {
        method: "POST",
        body: JSON.stringify(records),
        headers: jsonContentTypeHeader,
      },
    );

    return parseJSON(await response.text()).ids;
  }

  public async update(id: RecordId, record: Partial<T>): Promise<void> {
    return new UpdateOperation<T>(this.client, this.name, id, record).query();
  }

  public updateOp(id: RecordId, record: Partial<T>): UpdateOperation<T> {
    return new UpdateOperation<T>(this.client, this.name, id, record);
  }

  public async delete(id: RecordId): Promise<void> {
    return new DeleteOperation(this.client, this.name, id).query();
  }

  public deleteOp(id: RecordId): DeleteOperation {
    return new DeleteOperation(this.client, this.name, id);
  }

  public async subscribe(id: RecordId): Promise<ReadableStream<Event>> {
    return await this.subscribeImpl(id);
  }

  public async subscribeAll(
    opts?: SubscribeOpts,
  ): Promise<ReadableStream<Event>> {
    return await this.subscribeImpl("*", opts);
  }

  private async subscribeImpl(
    id: RecordId,
    opts?: SubscribeOpts,
  ): Promise<ReadableStream<Event>> {
    const params = new URLSearchParams();
    const filters = opts?.filters ?? [];
    if (filters.length > 0) {
      for (const filter of filters) {
        addFiltersToParams(params, "filter", filter);
      }
    }

    const response = await this.client.fetch(
      filters.length > 0
        ? `${recordApiBasePath}/${this.name}/subscribe/${id}?${params}`
        : `${recordApiBasePath}/${this.name}/subscribe/${id}`,
    );
    const body = response.body;
    if (!body) {
      throw Error("Subscription reader is null.");
    }

    const decoder = new TextDecoder();
    const transformStream = new TransformStream<Uint8Array, Event>({
      transform(chunk: Uint8Array, controller) {
        const messages = decoder.decode(chunk).trimEnd().split("\n\n");
        for (const msg of messages) {
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

export interface ClientOptions {
  tokens?: Tokens;
  onAuthChange?: (client: Client, user?: User) => void;
}

export interface Client {
  get base(): URL | undefined;

  /// Low-level access to tokens (auth, refresh, csrf) useful for persisting them.
  tokens(): Tokens | undefined;

  /// Provides current user.
  user(): User | undefined;

  /// Provides current user.
  headers(): HeadersInit;

  /// Construct accessor for Record API with given name.
  records<T = Record<string, unknown>>(name: string): RecordApi<T>;

  avatarUrl(userId?: string): string | undefined;

  login(email: string, password: string): Promise<void>;
  logout(): Promise<boolean>;

  deleteUser(): Promise<void>;
  checkCookies(): Promise<Tokens | undefined>;
  refreshAuthToken(): Promise<void>;

  /// Fetches data from TrailBase endpoints, e.g.:
  ///    const response = await client.fetch("/api/auth/v1/status");
  ///
  /// Unlike native fetch, will throw in case !response.ok.
  fetch(path: string, init?: FetchOptions): Promise<Response>;

  /// Execute a batch query.
  execute(
    operations: (CreateOperation | UpdateOperation | DeleteOperation)[],
    transaction?: boolean,
  ): Promise<RecordId[]>;
}

/// Client for interacting with TrailBase auth and record APIs.
class ClientImpl implements Client {
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
  public records<T = Record<string, unknown>>(name: string): RecordApi<T> {
    return new RecordApiImpl<T>(this, name);
  }

  /// Execute a batch query.
  async execute(
    operations: (CreateOperation | UpdateOperation | DeleteOperation)[],
    transaction: boolean = true,
  ): Promise<RecordId[]> {
    const response = await this.fetch(transactionApiBasePath, {
      method: "POST",
      body: JSON.stringify({ operations, transaction }),
      headers: jsonContentTypeHeader,
    });

    return parseJSON(await response.text()).ids;
  }

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

/// Initialize a new TrailBase client.
export function initClient(site?: URL | string, opts?: ClientOptions): Client {
  return new ClientImpl(site, opts);
}

/// Asynchronizly initialize a new TrailBase client trying to convert any
/// potentially existing cookies into an authenticated client.
export async function initClientFromCookies(
  site?: URL | string,
  opts?: ClientOptions,
): Promise<Client> {
  const client = new ClientImpl(site, opts);

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

const recordApiBasePath = "/api/records/v1";
const authApiBasePath = "/api/auth/v1";
const transactionApiBasePath = "/api/transaction/v1/execute";

export function filePath(
  apiName: string,
  recordId: RecordId,
  columnName: string,
): string {
  return `${recordApiBasePath}/${apiName}/${recordId}/file/${columnName}`;
}

export function filesPath(
  apiName: string,
  recordId: RecordId,
  columnName: string,
  fileName: string,
): string {
  return `${recordApiBasePath}/${apiName}/${recordId}/files/${columnName}/${fileName}`;
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

/// Decode a base64 string to bytes.
function base64Decode(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

/// Decode a "url-safe" base64 string to bytes.
export function urlSafeBase64Decode(base64: string): Uint8Array {
  return base64Decode(base64.replace(/_/g, "/").replace(/-/g, "+"));
}

/// Encode an arbitrary string input as base64 string.
function base64Encode(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes));
}

/// Encode an arbitrary string input as a "url-safe" base64 string.
export function urlSafeBase64Encode(bytes: Uint8Array): string {
  return base64Encode(bytes).replace(/\//g, "_").replace(/\+/g, "-");
}

function addFiltersToParams(
  params: URLSearchParams,
  path: string,
  filter: FilterOrComposite,
) {
  if ("and" in filter) {
    for (const [i, f] of (filter as And).and.entries()) {
      addFiltersToParams(params, `${path}[$and][${i}]`, f);
    }
  } else if ("or" in filter) {
    for (const [i, f] of (filter as Or).or.entries()) {
      addFiltersToParams(params, `${path}[$or][${i}]`, f);
    }
  } else {
    const f = filter as Filter;
    const op = f.op;
    if (op) {
      params.append(`${path}[${f.column}][${formatCompareOp(op)}]`, f.value);
    } else {
      params.append(`${path}[${f.column}]`, f.value);
    }
  }
}

export const exportedForTesting = isDev
  ? {
      base64Decode,
      base64Encode,
    }
  : undefined;

// BigInt JSON stringify/parse shenanigans.
declare global {
  interface BigInt {
    toJSON(): unknown;
  }
}

BigInt.prototype.toJSON = function () {
  return JSON.rawJSON(this.toString());
};

function parseJSON(text: string) {
  function reviver(_key: string, value: unknown, context: { source: string }) {
    if (
      typeof value === "number" &&
      Number.isInteger(value) &&
      !Number.isSafeInteger(value)
    ) {
      // Ignore the value because it has already lost precision
      return BigInt(context.source);
    }
    return value;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return JSON.parse(text, reviver as any);
}
