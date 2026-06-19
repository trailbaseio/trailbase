import { jwtDecode } from "jwt-decode";
import * as JSON from "@ungap/raw-json";

import { isDev } from "./constants";
import { jsonContentTypeHeader } from "./constants";
import { parseJSON } from "./json";
import type {
  RecordApi,
  RecordId,
  CreateOperation,
  UpdateOperation,
  DeleteOperation,
} from "./record_api";
import { RecordApiImpl } from "./record_api";
import { DefaultTransport, Transport } from "./transport";

export type { Transport } from "./transport";

import type { ChangeEmailRequest } from "@bindings/ChangeEmailRequest";
import type { ConfirmRegisterTotpRequest } from "@bindings/ConfirmRegisterTotpRequest";
import type { DisableTotpRequest } from "@bindings/DisableTotpRequest";
import type { LoginAnonymousRequest } from "@bindings/LoginAnonymousRequest";
import type { LoginMfaRequest } from "@bindings/LoginMfaRequest";
import type { LoginOtpRequest } from "@bindings/LoginOtpRequest";
import type { LoginRequest } from "@bindings/LoginRequest";
import type { LoginResponse } from "@bindings/LoginResponse";
import type { LoginStatusResponse } from "@bindings/LoginStatusResponse";
import type { LogoutRequest } from "@bindings/LogoutRequest";
import type { MfaTokenResponse } from "@bindings/MfaTokenResponse";
import type { PromoteAnonymousRequest } from "@bindings/PromoteAnonymousRequest";
import type { RefreshRequest } from "@bindings/RefreshRequest";
import type { RefreshResponse } from "@bindings/RefreshResponse";
import type { RegisterTotpResponse } from "@bindings/RegisterTotpResponse";
import type { RequestOtpRequest } from "@bindings/RequestOtpRequest";

export type User = {
  id: string;
  email: string | null;
  username: string | null;
  admin?: boolean;
  mfa?: boolean;
  provider?: number;
};

export interface MultiFactorAuthToken {
  token: string;
}

export type RegisterTotp = { url: string; png: string | null };

export type Tokens = {
  auth_token: string;
  refresh_token: string | null;
  csrf_token: string | null;
};

type TokenClaims = {
  sub: string;
  iat: number;
  exp: number;
  email: string | null;
  username: string | null;
  csrf_token: string;
  admin?: boolean;
  mfa?: boolean;
  provider?: number;
};

type TokenState = {
  state?: {
    tokens: Tokens;
    claims: TokenClaims;
  };
  headers: HeadersInit;
};

type PromotionOptions = {
  username?: string;
  email?: string;
  password: string;
};

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
      username: claims.username,
      admin: claims.admin,
      mfa: claims.mfa,
      provider: claims.provider,
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

export class FetchError implements Error {
  public readonly status: number;
  public readonly url: string | URL | undefined;
  public readonly message: string;
  public readonly name: string = "FetchError";

  constructor(status: number, msg: string, url?: string | URL) {
    this.message = msg;
    this.status = status;
    this.url = url;
  }

  static async from(
    response: Response,
    url?: string | URL,
  ): Promise<FetchError> {
    // Some IntoResponse implementations return a body, e.g. RecordError::BadRequest.
    const msg: string = await response.text().then(
      (b) => (b !== "" ? b : response.statusText),
      (_err) => response.statusText,
    );
    return new FetchError(response.status, msg, url);
  }

  public isClient(): boolean {
    return this.status >= 400 && this.status < 500;
  }

  public isServer(): boolean {
    return this.status >= 500;
  }

  public toString(): string {
    return `FetchError(${[this.status, this.message, this.url].filter((e) => e !== undefined).join(", ")})`;
  }
}

export interface ClientOptions {
  tokens?: Tokens;
  onAuthChange?: (client: Client, user?: User) => void;
  transport?: Transport;
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

  login(
    emailOrUsername: string,
    password: string,
  ): Promise<MultiFactorAuthToken | undefined>;
  loginSecond(opts: {
    mfaToken: MultiFactorAuthToken;
    totpCode: string;
  }): Promise<void>;
  requestOtp(
    emailOrUsername: string,
    opts?: { redirectUri?: string },
  ): Promise<void>;
  loginOtp(emailOrUsername: string, code: string): Promise<void>;
  loginAnonymously(): Promise<void>;
  logout(): Promise<boolean>;

  registerTOTP(opts?: { png: boolean }): Promise<RegisterTotp>;
  confirmTOTP(totpUrl: string, totp: string): Promise<void>;
  unregisterTOTP(totp: string): Promise<void>;

  /// Promote an anonymous user to "proper" user. If an email is provided, a verification
  /// email will be sent out.
  promoteAnonymous(opts: PromotionOptions): Promise<void>;
  /// Deletes the current user.
  deleteUser(): Promise<void>;
  /// Checks status endpoint. Can be used in Browser environments to promote token
  /// cookies to tokens.
  checkCookies(): Promise<Tokens | undefined>;
  /// Update auth token using longer-lived refresh tokens.
  refreshAuthToken(opts?: { force?: boolean }): Promise<void>;

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
  private readonly _base: URL | undefined;
  private readonly _transport: Transport;
  private readonly _authChange:
    | undefined
    | ((client: Client, user?: User) => void);
  private _tokenState: TokenState;

  constructor(baseUrl: URL | string | undefined, opts?: ClientOptions) {
    this._base = baseUrl ? new URL(baseUrl) : undefined;
    this._transport = opts?.transport ?? new DefaultTransport(this._base);
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
    return this._base;
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

  public async login(
    emailOrUsername: string,
    password: string,
  ): Promise<MultiFactorAuthToken | undefined> {
    try {
      const response = await this.fetch(`${authApiBasePath}/login`, {
        method: "POST",
        body: JSON.stringify({
          email_or_username: emailOrUsername,
          password,
        } as LoginRequest),
      });

      this.setTokenState(
        buildTokenState((await response.json()) as LoginResponse),
      );
    } catch (err) {
      if (err instanceof FetchError && err.status === 403) {
        const mfaTokenResponse = JSON.parse(err.message) as MfaTokenResponse;
        return {
          token: mfaTokenResponse.mfa_token,
        };
      }

      throw err;
    }
  }

  public async loginSecond(opts: {
    mfaToken: MultiFactorAuthToken;
    totpCode: string;
  }): Promise<void> {
    const response = await this.fetch(`${authApiBasePath}/login_mfa`, {
      method: "POST",
      body: JSON.stringify({
        mfa_token: opts.mfaToken.token,
        totp: opts.totpCode,
      } as LoginMfaRequest),
    });

    this.setTokenState(
      buildTokenState((await response.json()) as LoginResponse),
    );
  }

  public async requestOtp(
    emailOrUsername: string,
    opts?: { redirectUri?: string },
  ): Promise<void> {
    const redirect = opts?.redirectUri;
    const params = redirect ? `?redirect_uri=${redirect}` : "";

    const request =
      emailOrUsername.indexOf("@") === -1
        ? {
            username: emailOrUsername,
          }
        : ({
            email: emailOrUsername,
          } as RequestOtpRequest);

    await this.fetch(`${authApiBasePath}/otp/request${params}`, {
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  public async loginOtp(emailOrUsername: string, code: string): Promise<void> {
    const request =
      emailOrUsername.indexOf("@") >= 0
        ? {
            email: emailOrUsername,
            code,
          }
        : ({
            username: emailOrUsername,
            code,
          } as LoginOtpRequest);

    const response = await this.fetch(`${authApiBasePath}/otp/login`, {
      method: "POST",
      body: JSON.stringify(request),
    });

    this.setTokenState(
      buildTokenState((await response.json()) as LoginResponse),
    );
  }

  public async loginAnonymously(): Promise<void> {
    const response = await this.fetch(`${authApiBasePath}/login_anonymous`, {
      method: "POST",
      body: JSON.stringify({} as LoginAnonymousRequest),
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

  public async promoteAnonymous(opts: PromotionOptions): Promise<void> {
    await this.fetch(`${authApiBasePath}/promote_anonymous`, {
      method: "POST",
      body: JSON.stringify({
        new_password: opts.password,
        new_email: opts.email,
        new_username: opts.username,
      } as PromoteAnonymousRequest),
    });
  }

  public async deleteUser(): Promise<void> {
    await this.fetch(`${authApiBasePath}/delete`, {
      method: "DELETE",
    });
    this.setTokenState(buildTokenState(undefined));
  }

  public async changeEmail(email: string): Promise<void> {
    await this.fetch(`${authApiBasePath}/change_email`, {
      method: "POST",
      body: JSON.stringify({
        new_email: email,
      } as ChangeEmailRequest),
    });
  }

  public async registerTOTP(opts?: { png: boolean }): Promise<RegisterTotp> {
    const response = await this.fetch(
      `${authApiBasePath}/totp/register?png=${opts?.png ?? false}`,
      {
        method: "GET",
      },
    );

    const parsed: RegisterTotpResponse = parseJSON(await response.text());
    return {
      url: parsed.totp_url,
      png: parsed.png,
    };
  }

  public async confirmTOTP(totpUrl: string, totp: string): Promise<void> {
    await this.fetch(`${authApiBasePath}/totp/confirm`, {
      method: "POST",
      body: JSON.stringify({
        totp_url: totpUrl,
        totp,
      } as ConfirmRegisterTotpRequest),
    });
    await this.refreshAuthToken({ force: true });
  }

  public async unregisterTOTP(totp: string): Promise<void> {
    await this.fetch(`${authApiBasePath}/totp/unregister`, {
      method: "POST",
      body: JSON.stringify({
        totp,
      } as DisableTotpRequest),
    });
    await this.refreshAuthToken({ force: true });
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

  public async refreshAuthToken(opts?: { force?: boolean }): Promise<void> {
    const force = opts?.force ?? false;
    const refreshToken = force
      ? this._tokenState.state?.tokens.refresh_token
      : shouldRefresh(this._tokenState);
    if (refreshToken) {
      // Note: refreshTokenImpl will auto-logout on 401.
      this.setTokenState(
        await refreshTokensImpl(this._transport, refreshToken),
      );
    }
  }

  private setTokenState(
    state: TokenState,
    skipCb: boolean = false,
  ): TokenState {
    if (isExpired(state)) {
      // This can happen on initial construction, i.e. if a client is constructed
      // from older, persisted tokens. This will normally fix itself up with the
      // next refresh unless there's no valid refresh token.
      console.debug(`auth token expired`);
    }

    this._tokenState = state;
    if (!skipCb) {
      this._authChange?.(this, buildUser(state));
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
        await refreshTokensImpl(this._transport, refreshToken),
      );
    }

    try {
      const response = await this._transport.fetch(path, {
        ...init,
        headers: {
          credentials: isDev ? "include" : "same-origin",
          ...jsonContentTypeHeader,
          ...tokenState?.headers,
          ...init?.headers,
        },
      });

      if (!response.ok && (init?.throwOnError ?? true)) {
        throw await FetchError.from(
          response,
          isDev ? new URL(path, this.base) : undefined,
        );
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

/// Asynchronously initialize a new TrailBase client trying to convert any
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

// NOTE: We cannot not use ClientIMpl.fetch, which itself does token refreshing to avoid a loop.
async function refreshTokensImpl(
  transport: Transport,
  refreshToken: string,
): Promise<TokenState> {
  const path = `${authApiBasePath}/refresh`;
  try {
    const response = await transport.fetch(path, {
      method: "POST",
      body: JSON.stringify({
        refresh_token: refreshToken,
      } as RefreshRequest),
      headers: jsonContentTypeHeader,
    });

    switch (response.status) {
      case 401:
        // Refresh token was rejected w/o means to recover. May as well log out.
        return buildTokenState(undefined);
      case 200:
        return buildTokenState({
          ...((await response.json()) as RefreshResponse),
          refresh_token: refreshToken,
        });
      default:
        throw await FetchError.from(
          response,
          isDev ? new URL(path, path) : undefined,
        );
    }
  } catch (err) {
    if (err instanceof TypeError) {
      console.debug(`Connection refused ${err}. TrailBase down or CORS?`);
    }
    throw err;
  }
}

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

const authApiBasePath = "/api/auth/v1";
const transactionApiBasePath = "/api/transaction/v1/execute";
