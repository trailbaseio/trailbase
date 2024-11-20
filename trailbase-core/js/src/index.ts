declare var rustyscript: any;
declare var globalThis: any;

export type HeaderMapType = { [key: string]: string };
export type PathParamsType = { [key: string]: string };
export type UserType = {
  /// Base64 encoded UUIDv7 user id.
  id: string;
  /// The user's email address.
  email: string;
  /// The user's CSRF token.
  csrf: string;
};
export type RequestType = {
  uri: string;
  params: PathParamsType;
  headers: HeaderMapType;
  user?: UserType;
  body?: Uint8Array;
};
export type ResponseType = {
  headers?: [string, string][];
  status?: number;
  body?: Uint8Array;
};
export type MaybeResponse<T> = Promise<T | undefined> | T | undefined;
export type CallbackType = (req: RequestType) => MaybeResponse<ResponseType>;
export type Method =
  | "DELETE"
  | "GET"
  | "HEAD"
  | "OPTIONS"
  | "PATCH"
  | "POST"
  | "PUT"
  | "TRACE";

/// HTTP status codes.
///
// source: https://github.com/prettymuchbryce/http-status-codes/blob/master/src/status-codes.ts
export enum StatusCodes {
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.2.1
  ///
  /// This interim response indicates that everything so far is OK and that the
  /// client should continue with the request or ignore it if it is already
  /// finished.
  CONTINUE = 100,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.2.2
  ///
  /// This code is sent in response to an Upgrade request header by the client,
  /// and indicates the protocol the server is switching too.
  SWITCHING_PROTOCOLS = 101,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.1
  ///
  /// This code indicates that the server has received and is processing the
  /// request, but no response is available yet.
  PROCESSING = 102,
  /// Official Documentation @ https://www.rfc-editor.org/rfc/rfc8297#page-3
  ///
  /// This code indicates to the client that the server is likely to send a
  /// final response with the header fields included in the informational
  /// response.
  EARLY_HINTS = 103,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.1
  ///
  /// The request has succeeded. The meaning of a success varies depending on the HTTP method:
  /// GET: The resource has been fetched and is transmitted in the message body.
  /// HEAD: The entity headers are in the message body.
  /// POST: The resource describing the result of the action is transmitted in the message body.
  /// TRACE: The message body contains the request message as received by the server
  OK = 200,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.2
  ///
  /// The request has succeeded and a new resource has been created as a result
  /// of it. This is typically the response sent after a PUT request.
  CREATED = 201,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.3
  ///
  /// The request has been received but not yet acted upon. It is
  /// non-committal, meaning that there is no way in HTTP to later send an
  /// asynchronous response indicating the outcome of processing the request. It
  /// is intended for cases where another process or server handles the request,
  /// or for batch processing.
  ACCEPTED = 202,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.4
  ///
  /// This response code means returned meta-information set is not exact set
  /// as available from the origin server, but collected from a local or a third
  /// party copy. Except this condition, 200 OK response should be preferred
  /// instead of this response.
  NON_AUTHORITATIVE_INFORMATION = 203,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.5
  ///
  /// There is no content to send for this request, but the headers may be
  /// useful. The user-agent may update its cached headers for this resource with
  /// the new ones.
  NO_CONTENT = 204,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.3.6
  ///
  /// This response code is sent after accomplishing request to tell user agent
  /// reset document view which sent this request.
  RESET_CONTENT = 205,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7233#section-4.1
  ///
  /// This response code is used because of range header sent by the client to
  /// separate download into multiple streams.
  PARTIAL_CONTENT = 206,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.2
  ///
  /// A Multi-Status response conveys information about multiple resources in
  /// situations where multiple status codes might be appropriate.
  MULTI_STATUS = 207,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.1
  ///
  /// The request has more than one possible responses. User-agent or user
  /// should choose one of them. There is no standardized way to choose one of
  /// the responses.
  MULTIPLE_CHOICES = 300,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.2
  ///
  /// This response code means that URI of requested resource has been changed.
  /// Probably, new URI would be given in the response.
  MOVED_PERMANENTLY = 301,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.3
  ///
  /// This response code means that URI of requested resource has been changed
  /// temporarily. New changes in the URI might be made in the future. Therefore,
  /// this same URI should be used by the client in future requests.
  MOVED_TEMPORARILY = 302,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.4
  ///
  /// Server sent this response to directing client to get requested resource
  /// to another URI with an GET request.
  SEE_OTHER = 303,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7232#section-4.1
  ///
  /// This is used for caching purposes. It is telling to client that response
  /// has not been modified. So, client can continue to use same cached version
  /// of response.
  NOT_MODIFIED = 304,
  /// @deprecated
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.6
  ///
  /// Was defined in a previous version of the HTTP specification to indicate
  /// that a requested response must be accessed by a proxy. It has been
  /// deprecated due to security concerns regarding in-band configuration of a
  /// proxy.
  USE_PROXY = 305,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.4.7
  ///
  /// Server sent this response to directing client to get requested resource
  /// to another URI with same method that used prior request. This has the same
  /// semantic than the 302 Found HTTP response code, with the exception that the
  /// user agent must not change the HTTP method used: if a POST was used in the
  /// first request, a POST must be used in the second request.
  TEMPORARY_REDIRECT = 307,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7538#section-3
  ///
  /// This means that the resource is now permanently located at another URI,
  /// specified by the Location: HTTP Response header. This has the same
  /// semantics as the 301 Moved Permanently HTTP response code, with the
  /// exception that the user agent must not change the HTTP method used: if a
  /// POST was used in the first request, a POST must be used in the second
  /// request.
  PERMANENT_REDIRECT = 308,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.1
  ///
  /// This response means that server could not understand the request due to invalid syntax.
  BAD_REQUEST = 400,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7235#section-3.1
  ///
  /// Although the HTTP standard specifies "unauthorized", semantically this
  /// response means "unauthenticated". That is, the client must authenticate
  /// itself to get the requested response.
  UNAUTHORIZED = 401,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.2
  ///
  /// This response code is reserved for future use. Initial aim for creating
  /// this code was using it for digital payment systems however this is not used
  /// currently.
  PAYMENT_REQUIRED = 402,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.3
  ///
  /// The client does not have access rights to the content, i.e. they are
  /// unauthorized, so server is rejecting to give proper response. Unlike 401,
  /// the client's identity is known to the server.
  FORBIDDEN = 403,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.4
  ///
  /// The server can not find requested resource. In the browser, this means
  /// the URL is not recognized. In an API, this can also mean that the endpoint
  /// is valid but the resource itself does not exist. Servers may also send this
  /// response instead of 403 to hide the existence of a resource from an
  /// unauthorized client. This response code is probably the most famous one due
  /// to its frequent occurence on the web.
  NOT_FOUND = 404,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.5
  ///
  /// The request method is known by the server but has been disabled and
  /// cannot be used. For example, an API may forbid DELETE-ing a resource. The
  /// two mandatory methods, GET and HEAD, must never be disabled and should not
  /// return this error code.
  METHOD_NOT_ALLOWED = 405,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.6
  ///
  /// This response is sent when the web server, after performing server-driven
  /// content negotiation, doesn't find any content following the criteria given
  /// by the user agent.
  NOT_ACCEPTABLE = 406,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7235#section-3.2
  ///
  /// This is similar to 401 but authentication is needed to be done by a proxy.
  PROXY_AUTHENTICATION_REQUIRED = 407,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.7
  ///
  /// This response is sent on an idle connection by some servers, even without
  /// any previous request by the client. It means that the server would like to
  /// shut down this unused connection. This response is used much more since
  /// some browsers, like Chrome, Firefox 27+, or IE9, use HTTP pre-connection
  /// mechanisms to speed up surfing. Also note that some servers merely shut
  /// down the connection without sending this message.
  REQUEST_TIMEOUT = 408,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.8
  ///
  /// This response is sent when a request conflicts with the current state of the server.
  CONFLICT = 409,
  ///
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.9
  ///
  /// This response would be sent when the requested content has been
  /// permenantly deleted from server, with no forwarding address. Clients are
  /// expected to remove their caches and links to the resource. The HTTP
  /// specification intends this status code to be used for "limited-time,
  /// promotional services". APIs should not feel compelled to indicate resources
  /// that have been deleted with this status code.
  GONE = 410,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.10
  ///
  /// The server rejected the request because the Content-Length header field
  /// is not defined and the server requires it.
  LENGTH_REQUIRED = 411,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7232#section-4.2
  ///
  /// The client has indicated preconditions in its headers which the server
  /// does not meet.
  PRECONDITION_FAILED = 412,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.11
  ///
  /// Request entity is larger than limits defined by server; the server might
  /// close the connection or return an Retry-After header field.
  REQUEST_TOO_LONG = 413,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.12
  ///
  /// The URI requested by the client is longer than the server is willing to interpret.
  REQUEST_URI_TOO_LONG = 414,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.13
  ///
  /// The media format of the requested data is not supported by the server, so
  /// the server is rejecting the request.
  UNSUPPORTED_MEDIA_TYPE = 415,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7233#section-4.4
  ///
  /// The range specified by the Range header field in the request can't be
  /// fulfilled; it's possible that the range is outside the size of the target
  /// URI's data.
  REQUESTED_RANGE_NOT_SATISFIABLE = 416,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.5.14
  ///
  /// This response code means the expectation indicated by the Expect request
  /// header field can't be met by the server.
  EXPECTATION_FAILED = 417,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2324#section-2.3.2
  ///
  /// Any attempt to brew coffee with a teapot should result in the error code
  /// "418 I'm a teapot". The resulting entity body MAY be short and stout.
  IM_A_TEAPOT = 418,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.6
  ///
  /// The 507 (Insufficient Storage) status code means the method could not be
  /// performed on the resource because the server is unable to store the
  /// representation needed to successfully complete the request. This condition
  /// is considered to be temporary. If the request which received this status
  /// code was the result of a user action, the request MUST NOT be repeated
  /// until it is requested by a separate user action.
  INSUFFICIENT_SPACE_ON_RESOURCE = 419,
  /// @deprecated
  /// Official Documentation @ https://tools.ietf.org/rfcdiff?difftype=--hwdiff&url2=draft-ietf-webdav-protocol-06.txt
  ///
  /// A deprecated response used by the Spring Framework when a method has failed.
  METHOD_FAILURE = 420,
  /// Official Documentation @ https://datatracker.ietf.org/doc/html/rfc7540#section-9.1.2
  ///
  /// Defined in the specification of HTTP/2 to indicate that a server is not
  /// able to produce a response for the combination of scheme and authority that
  /// are included in the request URI.
  MISDIRECTED_REQUEST = 421,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.3
  ///
  /// The request was well-formed but was unable to be followed due to semantic errors.
  UNPROCESSABLE_ENTITY = 422,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.4
  ///
  /// The resource that is being accessed is locked.
  LOCKED = 423,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.5
  ///
  /// The request failed due to failure of a previous request.
  FAILED_DEPENDENCY = 424,
  /// Official Documentation @ https://datatracker.ietf.org/doc/html/rfc7231#section-6.5.15
  ///
  /// The server refuses to perform the request using the current protocol but
  /// might be willing to do so after the client upgrades to a different
  /// protocol.
  UPGRADE_REQUIRED = 426,
  /// Official Documentation @ https://tools.ietf.org/html/rfc6585#section-3
  ///
  /// The origin server requires the request to be conditional. Intended to
  /// prevent the 'lost update' problem, where a client GETs a resource's state,
  /// modifies it, and PUTs it back to the server, when meanwhile a third party
  /// has modified the state on the server, leading to a conflict.
  PRECONDITION_REQUIRED = 428,
  /// Official Documentation @ https://tools.ietf.org/html/rfc6585#section-4
  ///
  /// The user has sent too many requests in a given amount of time ("rate limiting").
  TOO_MANY_REQUESTS = 429,
  /// Official Documentation @ https://tools.ietf.org/html/rfc6585#section-5
  ///
  /// The server is unwilling to process the request because its header fields
  /// are too large. The request MAY be resubmitted after reducing the size of
  /// the request header fields.
  REQUEST_HEADER_FIELDS_TOO_LARGE = 431,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7725
  ///
  /// The user-agent requested a resource that cannot legally be provided, such
  /// as a web page censored by a government.
  UNAVAILABLE_FOR_LEGAL_REASONS = 451,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.1
  ///
  /// The server encountered an unexpected condition that prevented it from
  /// fulfilling the request.
  INTERNAL_SERVER_ERROR = 500,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.2
  ///
  /// The request method is not supported by the server and cannot be handled.
  /// The only methods that servers are required to support (and therefore that
  /// must not return this code) are GET and HEAD.
  NOT_IMPLEMENTED = 501,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.3
  ///
  /// This error response means that the server, while working as a gateway to
  /// get a response needed to handle the request, got an invalid response.
  BAD_GATEWAY = 502,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.4
  ///
  /// The server is not ready to handle the request. Common causes are a server
  /// that is down for maintenance or that is overloaded. Note that together with
  /// this response, a user-friendly page explaining the problem should be sent.
  /// This responses should be used for temporary conditions and the Retry-After:
  /// HTTP header should, if possible, contain the estimated time before the
  /// recovery of the service. The webmaster must also take care about the
  /// caching-related headers that are sent along with this response, as these
  /// temporary condition responses should usually not be cached.
  SERVICE_UNAVAILABLE = 503,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.5
  ///
  /// This error response is given when the server is acting as a gateway and
  /// cannot get a response in time.
  GATEWAY_TIMEOUT = 504,
  /// Official Documentation @ https://tools.ietf.org/html/rfc7231#section-6.6.6
  ///
  /// The HTTP version used in the request is not supported by the server.
  HTTP_VERSION_NOT_SUPPORTED = 505,
  /// Official Documentation @ https://tools.ietf.org/html/rfc2518#section-10.6
  ///
  /// The server has an internal configuration error: the chosen variant
  /// resource is configured to engage in transparent content negotiation itself,
  /// and is therefore not a proper end point in the negotiation process.
  INSUFFICIENT_STORAGE = 507,
  /// Official Documentation @ https://tools.ietf.org/html/rfc6585#section-6
  ///
  /// The 511 status code indicates that the client needs to authenticate to
  /// gain network access.
  NETWORK_AUTHENTICATION_REQUIRED = 511,
}

export class HttpError extends Error {
  readonly statusCode: number;
  readonly headers: [string, string][] | undefined;

  constructor(
    statusCode: number,
    message?: string,
    headers?: [string, string][],
  ) {
    super(message);
    this.statusCode = statusCode;
    this.headers = headers;
  }

  public override toString(): string {
    return `HttpError(${this.statusCode}, ${this.message})`;
  }

  toResponse(): ResponseType {
    const m = this.message;
    return {
      headers: this.headers,
      status: this.statusCode,
      body: m !== "" ? encodeFallback(m) : undefined,
    };
  }
}

export type StringRequestType = {
  uri: string;
  params: PathParamsType;
  headers: HeaderMapType;
  user?: UserType;
  body?: string;
};
export type StringResponseType = {
  headers?: [string, string][];
  status?: number;
  body: string;
};

export function stringHandler(
  f: (req: StringRequestType) => MaybeResponse<StringResponseType | string>,
): CallbackType {
  return async (req: RequestType): Promise<ResponseType | undefined> => {
    try {
      let body = req.body;
      let resp: StringResponseType | string | undefined = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body),
      });

      if (resp === undefined) {
        return undefined;
      }

      if (typeof resp === "string") {
        return {
          status: StatusCodes.OK,
          body: encodeFallback(resp),
        };
      }

      const respBody = resp.body;
      return {
        headers: resp.headers,
        status: resp.status,
        body: respBody ? encodeFallback(respBody) : undefined,
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        status: StatusCodes.INTERNAL_SERVER_ERROR,
        body: encodeFallback(`Uncaught error: ${err}`),
      };
    }
  };
}

export type HtmlResponseType = {
  headers?: [string, string][];
  status?: number;
  body: string;
};

export function htmlHandler(
  f: (req: StringRequestType) => MaybeResponse<HtmlResponseType | string>,
): CallbackType {
  return async (req: RequestType): Promise<ResponseType | undefined> => {
    try {
      let body = req.body;
      let resp: HtmlResponseType | string | undefined = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body),
      });

      if (resp === undefined) {
        return undefined;
      }

      if (typeof resp === "string") {
        return {
          headers: [["content-type", "text/html"]],
          status: StatusCodes.OK,
          body: encodeFallback(resp),
        };
      }

      const respBody = resp.body;
      return {
        headers: [["content-type", "text/html"], ...(resp.headers ?? [])],
        status: resp.status,
        body: respBody ? encodeFallback(respBody) : undefined,
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        status: StatusCodes.INTERNAL_SERVER_ERROR,
        body: encodeFallback(`Uncaught error: ${err}`),
      };
    }
  };
}

export type JsonRequestType = {
  uri: string;
  params: PathParamsType;
  headers: HeaderMapType;
  user?: UserType;
  body?: object | string;
};
export interface JsonResponseType {
  headers?: [string, string][];
  status?: number;
  body: object;
}

export function jsonHandler(
  f: (req: JsonRequestType) => MaybeResponse<JsonRequestType | object>,
): CallbackType {
  return async (req: RequestType): Promise<ResponseType | undefined> => {
    try {
      let body = req.body;
      let resp: JsonResponseType | object | undefined = await f({
        uri: req.uri,
        params: req.params,
        headers: req.headers,
        user: req.user,
        body: body && decodeFallback(body),
      });

      if (resp === undefined) {
        return undefined;
      }

      if ("body" in resp) {
        const r = resp as JsonResponseType;
        const rBody = r.body;
        return {
          headers: [["content-type", "application/json"], ...(r.headers ?? [])],
          status: r.status,
          body: rBody ? encodeFallback(JSON.stringify(rBody)) : undefined,
        };
      }

      return {
        headers: [["content-type", "application/json"]],
        status: StatusCodes.OK,
        body: encodeFallback(JSON.stringify(resp)),
      };
    } catch (err) {
      if (err instanceof HttpError) {
        return err.toResponse();
      }
      return {
        headers: [["content-type", "application/json"]],
        status: StatusCodes.INTERNAL_SERVER_ERROR,
        body: encodeFallback(`Uncaught error: ${err}`),
      };
    }
  };
}

const callbacks = new Map<string, CallbackType>();

export function addRoute(
  method: Method,
  route: string,
  callback: CallbackType,
) {
  rustyscript.functions.route(method, route);
  callbacks.set(`${method}:${route}`, callback);
  console.debug("JS: Added route:", method, route);
}

export async function dispatch(
  method: Method,
  route: string,
  uri: string,
  pathParams: [string, string][],
  headers: [string, string][],
  user: UserType | undefined,
  body: Uint8Array,
): Promise<ResponseType> {
  const key = `${method}:${route}`;
  const cb: CallbackType | undefined = callbacks.get(key);
  if (!cb) {
    throw Error(`Missing callback: ${key}`);
  }

  return (
    (await cb({
      uri,
      params: Object.fromEntries(pathParams),
      headers: Object.fromEntries(headers),
      user: user,
      body,
    })) ?? { status: StatusCodes.OK }
  );
}

globalThis.__dispatch = dispatch;

export async function query(
  queryStr: string,
  params: unknown[],
): Promise<unknown[][]> {
  return await rustyscript.async_functions.query(queryStr, params);
}

export async function execute(
  queryStr: string,
  params: unknown[],
): Promise<number> {
  return await rustyscript.async_functions.execute(queryStr, params);
}

export type ParsedPath = {
  path: string;
  query: URLSearchParams;
};

export function parsePath(path: string): ParsedPath {
  const queryIndex = path.indexOf("?");
  if (queryIndex >= 0) {
    return {
      path: path.slice(0, queryIndex),
      query: new URLSearchParams(path.slice(queryIndex + 1)),
    };
  }

  return {
    path,
    query: new URLSearchParams(),
  };
}

/// @param {Uint8Array} bytes
/// @return {string}
///
/// source: https://github.com/samthor/fast-text-encoding
export function decodeFallback(bytes: Uint8Array): string {
  var inputIndex = 0;

  // Create a working buffer for UTF-16 code points, but don't generate one
  // which is too large for small input sizes. UTF-8 to UCS-16 conversion is
  // going to be at most 1:1, if all code points are ASCII. The other extreme
  // is 4-byte UTF-8, which results in two UCS-16 points, but this is still 50%
  // fewer entries in the output.
  var pendingSize = Math.min(256 * 256, bytes.length + 1);
  var pending = new Uint16Array(pendingSize);
  var chunks = [];
  var pendingIndex = 0;

  for (;;) {
    var more = inputIndex < bytes.length;

    // If there's no more data or there'd be no room for two UTF-16 values,
    // create a chunk. This isn't done at the end by simply slicing the data
    // into equal sized chunks as we might hit a surrogate pair.
    if (!more || pendingIndex >= pendingSize - 1) {
      // nb. .apply and friends are *really slow*. Low-hanging fruit is to
      // expand this to literally pass pending[0], pending[1], ... etc, but
      // the output code expands pretty fast in this case.
      // These extra vars get compiled out: they're just to make TS happy.
      // Turns out you can pass an ArrayLike to .apply().
      var subarray = pending.subarray(0, pendingIndex);
      var arraylike = subarray as unknown as number[];
      chunks.push(String.fromCharCode.apply(null, arraylike));

      if (!more) {
        return chunks.join("");
      }

      // Move the buffer forward and create another chunk.
      bytes = bytes.subarray(inputIndex);
      inputIndex = 0;
      pendingIndex = 0;
    }

    // The native TextDecoder will generate "REPLACEMENT CHARACTER" where the
    // input data is invalid. Here, we blindly parse the data even if it's
    // wrong: e.g., if a 3-byte sequence doesn't have two valid continuations.

    var byte1 = bytes[inputIndex++];
    if ((byte1 & 0x80) === 0) {
      // 1-byte or null
      pending[pendingIndex++] = byte1;
    } else if ((byte1 & 0xe0) === 0xc0) {
      // 2-byte
      var byte2 = bytes[inputIndex++] & 0x3f;
      pending[pendingIndex++] = ((byte1 & 0x1f) << 6) | byte2;
    } else if ((byte1 & 0xf0) === 0xe0) {
      // 3-byte
      var byte2 = bytes[inputIndex++] & 0x3f;
      var byte3 = bytes[inputIndex++] & 0x3f;
      pending[pendingIndex++] = ((byte1 & 0x1f) << 12) | (byte2 << 6) | byte3;
    } else if ((byte1 & 0xf8) === 0xf0) {
      // 4-byte
      var byte2 = bytes[inputIndex++] & 0x3f;
      var byte3 = bytes[inputIndex++] & 0x3f;
      var byte4 = bytes[inputIndex++] & 0x3f;

      // this can be > 0xffff, so possibly generate surrogates
      var codepoint =
        ((byte1 & 0x07) << 0x12) | (byte2 << 0x0c) | (byte3 << 0x06) | byte4;
      if (codepoint > 0xffff) {
        // codepoint &= ~0x10000;
        codepoint -= 0x10000;
        pending[pendingIndex++] = ((codepoint >>> 10) & 0x3ff) | 0xd800;
        codepoint = 0xdc00 | (codepoint & 0x3ff);
      }
      pending[pendingIndex++] = codepoint;
    } else {
      // invalid initial byte
    }
  }
}

/// @param {string} string
/// @return {Uint8Array}
////
/// source: https://github.com/samthor/fast-text-encoding
export function encodeFallback(string: string): Uint8Array {
  var pos = 0;
  var len = string.length;

  var at = 0; // output position
  var tlen = Math.max(32, len + (len >>> 1) + 7); // 1.5x size
  var target = new Uint8Array((tlen >>> 3) << 3); // ... but at 8 byte offset

  while (pos < len) {
    var value = string.charCodeAt(pos++);
    if (value >= 0xd800 && value <= 0xdbff) {
      // high surrogate
      if (pos < len) {
        var extra = string.charCodeAt(pos);
        if ((extra & 0xfc00) === 0xdc00) {
          ++pos;
          value = ((value & 0x3ff) << 10) + (extra & 0x3ff) + 0x10000;
        }
      }
      if (value >= 0xd800 && value <= 0xdbff) {
        continue; // drop lone surrogate
      }
    }

    // expand the buffer if we couldn't write 4 bytes
    if (at + 4 > target.length) {
      tlen += 8; // minimum extra
      tlen *= 1.0 + (pos / string.length) * 2; // take 2x the remaining
      tlen = (tlen >>> 3) << 3; // 8 byte offset

      var update = new Uint8Array(tlen);
      update.set(target);
      target = update;
    }

    if ((value & 0xffffff80) === 0) {
      // 1-byte
      target[at++] = value; // ASCII
      continue;
    } else if ((value & 0xfffff800) === 0) {
      // 2-byte
      target[at++] = ((value >>> 6) & 0x1f) | 0xc0;
    } else if ((value & 0xffff0000) === 0) {
      // 3-byte
      target[at++] = ((value >>> 12) & 0x0f) | 0xe0;
      target[at++] = ((value >>> 6) & 0x3f) | 0x80;
    } else if ((value & 0xffe00000) === 0) {
      // 4-byte
      target[at++] = ((value >>> 18) & 0x07) | 0xf0;
      target[at++] = ((value >>> 12) & 0x3f) | 0x80;
      target[at++] = ((value >>> 6) & 0x3f) | 0x80;
    } else {
      continue; // out of range
    }

    target[at++] = (value & 0x3f) | 0x80;
  }

  // Use subarray if slice isn't supported (IE11). This will use more memory
  // because the original array still exists.
  return target.slice ? target.slice(0, at) : target.subarray(0, at);
}
