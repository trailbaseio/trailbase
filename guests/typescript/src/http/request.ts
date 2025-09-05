import {
  Headers,
  IncomingBody,
  Scheme as WasiScheme,
} from "wasi:http/types@0.2.3";
import type { MethodType } from "trailbase:runtime/init-endpoint";
import type { HttpContextUser } from "@common/HttpContextUser";

export type Scheme = "HTTP" | "HTTPS" | "other";
export type Method = MethodType;

export type User = {
  id: string;
  email: string;
  csrf: string;
};

export interface Request {
  path(): string;
  method(): Method;
  scheme(): Scheme | undefined;
  authority(): string;
  headers(): [string, Uint8Array][];
  url(): URL;
  getQueryParam(param: string): string | null;

  // Path parameter, e.g. `/test/{param}/v0/`.
  pathParams(): [string, string][];
  getPathParam(param: string): string | null;

  // User metadata.
  user(): User | null;

  // Body accessors:
  body(): Uint8Array | undefined;
  json(): object | undefined;
}

export class RequestImpl implements Request {
  constructor(
    private readonly _method: Method,
    private readonly _path: string,
    private readonly _params: [string, string][],
    private readonly _scheme: WasiScheme | undefined,
    private readonly _authority: string,
    private readonly _headers: Headers,
    private readonly _user: HttpContextUser | null,
    private readonly _body: IncomingBody,
  ) {}

  path(): string {
    return this._path;
  }

  method(): Method {
    return this._method;
  }

  scheme(): Scheme | undefined {
    return this._scheme?.tag;
  }

  authority(): string {
    return this._authority;
  }

  headers(): [string, Uint8Array][] {
    return this._headers.entries();
  }

  user(): User | null {
    const u = this._user;
    return u === null
      ? null
      : {
          id: u.id,
          email: u.email,
          csrf: u.csrf_token,
        };
  }

  url(): URL {
    const base =
      this._scheme !== undefined
        ? `${printScheme(this._scheme)}://${this._authority}/${this._path}`
        : `/${this._path}`;
    return new URL(base);
  }

  getQueryParam(param: string): string | null {
    return this.url().searchParams.get(param);
  }

  pathParams(): [string, string][] {
    return this._params;
  }

  getPathParam(param: string): string | null {
    for (const [p, v] of this._params) {
      if (p === param) return v;
    }
    return null;
  }

  body(): Uint8Array | undefined {
    switch (this._method) {
      case "get":
      case "head":
        // NOTE: Throws stream closed for GET and HEAD requests.
        return undefined;
      default: {
        const s = this._body.stream();
        return s.read(BigInt(Number.MAX_SAFE_INTEGER));
      }
    }
  }

  json(): object | undefined {
    const b = this.body();
    if (b !== undefined) {
      return JSON.parse(new TextDecoder().decode(b));
    }
    return undefined;
  }
}

function printScheme(scheme: WasiScheme): string {
  switch (scheme.tag) {
    case "HTTP":
      return "http";
    case "HTTPS":
      return "https";
    case "other":
      return scheme.val;
  }
}
