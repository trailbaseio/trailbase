// https://github.com/bytecodealliance/jco/blob/b703b2850d3170d786812a56f40456870c780311/packages/preview2-shim/types/interfaces/wasi-http-types.d.ts
declare module "wasi:http/types@0.2.0" {
  /**
   * Attempts to extract a http-related `error` from the wasi:io `error`
   * provided.
   *
   * Stream operations which return
   * `wasi:io/stream/stream-error::last-operation-failed` have a payload of
   * type `wasi:io/error/error` with more information about the operation
   * that failed. This payload can be passed through to this function to see
   * if there's http-related information about the error to return.
   *
   * Note that this function is fallible because not all io-errors are
   * http-related errors.
   */
  export function httpErrorCode(err: IoError): ErrorCode | undefined;
  /**
   * Construct an empty HTTP Fields.
   *
   * The resulting `fields` is mutable.
   */
  export { Fields };
  /**
   * Construct an HTTP Fields.
   *
   * The resulting `fields` is mutable.
   *
   * The list represents each key-value pair in the Fields. Keys
   * which have multiple values are represented by multiple entries in this
   * list with the same key.
   *
   * The tuple is a pair of the field key, represented as a string, and
   * Value, represented as a list of bytes. In a valid Fields, all keys
   * and values are valid UTF-8 strings. However, values are not always
   * well-formed, so they are represented as a raw list of bytes.
   *
   * An error result will be returned if any header or value was
   * syntactically invalid, or if a header was forbidden.
   */
  /**
   * Get all of the values corresponding to a key. If the key is not present
   * in this `fields`, an empty list is returned. However, if the key is
   * present but empty, this is represented by a list with one or more
   * empty field-values present.
   */
  /**
   * Returns `true` when the key is present in this `fields`. If the key is
   * syntactically invalid, `false` is returned.
   */
  /**
   * Set all of the values for a key. Clears any existing values for that
   * key, if they have been set.
   *
   * Fails with `header-error.immutable` if the `fields` are immutable.
   */
  /**
   * Delete all values for a key. Does nothing if no values for the key
   * exist.
   *
   * Fails with `header-error.immutable` if the `fields` are immutable.
   */
  /**
   * Append a value for a key. Does not change or delete any existing
   * values for that key.
   *
   * Fails with `header-error.immutable` if the `fields` are immutable.
   */
  /**
   * Retrieve the full set of keys and values in the Fields. Like the
   * constructor, the list represents each key-value pair.
   *
   * The outer list represents each key-value pair in the Fields. Keys
   * which have multiple values are represented by multiple entries in this
   * list with the same key.
   */
  /**
   * Make a deep copy of the Fields. Equivelant in behavior to calling the
   * `fields` constructor on the return value of `entries`. The resulting
   * `fields` is mutable.
   */
  /**
   * Returns the method of the incoming request.
   */
  export { IncomingRequest };
  /**
   * Returns the path with query parameters from the request, as a string.
   */
  /**
   * Returns the protocol scheme from the request.
   */
  /**
   * Returns the authority from the request, if it was present.
   */
  /**
   * Get the `headers` associated with the request.
   *
   * The returned `headers` resource is immutable: `set`, `append`, and
   * `delete` operations will fail with `header-error.immutable`.
   *
   * The `headers` returned are a child resource: it must be dropped before
   * the parent `incoming-request` is dropped. Dropping this
   * `incoming-request` before all children are dropped will trap.
   */
  /**
   * Gives the `incoming-body` associated with this request. Will only
   * return success at most once, and subsequent calls will return error.
   */
  /**
   * Construct a new `outgoing-request` with a default `method` of `GET`, and
   * `none` values for `path-with-query`, `scheme`, and `authority`.
   *
   * * `headers` is the HTTP Headers for the Request.
   *
   * It is possible to construct, or manipulate with the accessor functions
   * below, an `outgoing-request` with an invalid combination of `scheme`
   * and `authority`, or `headers` which are not permitted to be sent.
   * It is the obligation of the `outgoing-handler.handle` implementation
   * to reject invalid constructions of `outgoing-request`.
   */
  export { OutgoingRequest };
  /**
   * Returns the resource corresponding to the outgoing Body for this
   * Request.
   *
   * Returns success on the first call: the `outgoing-body` resource for
   * this `outgoing-request` can be retrieved at most once. Subsequent
   * calls will return error.
   */
  /**
   * Get the Method for the Request.
   */
  /**
   * Set the Method for the Request. Fails if the string present in a
   * `method.other` argument is not a syntactically valid method.
   */
  /**
   * Get the combination of the HTTP Path and Query for the Request.
   * When `none`, this represents an empty Path and empty Query.
   */
  /**
   * Set the combination of the HTTP Path and Query for the Request.
   * When `none`, this represents an empty Path and empty Query. Fails is the
   * string given is not a syntactically valid path and query uri component.
   */
  /**
   * Get the HTTP Related Scheme for the Request. When `none`, the
   * implementation may choose an appropriate default scheme.
   */
  /**
   * Set the HTTP Related Scheme for the Request. When `none`, the
   * implementation may choose an appropriate default scheme. Fails if the
   * string given is not a syntactically valid uri scheme.
   */
  /**
   * Get the HTTP Authority for the Request. A value of `none` may be used
   * with Related Schemes which do not require an Authority. The HTTP and
   * HTTPS schemes always require an authority.
   */
  /**
   * Set the HTTP Authority for the Request. A value of `none` may be used
   * with Related Schemes which do not require an Authority. The HTTP and
   * HTTPS schemes always require an authority. Fails if the string given is
   * not a syntactically valid uri authority.
   */
  /**
   * Get the headers associated with the Request.
   *
   * The returned `headers` resource is immutable: `set`, `append`, and
   * `delete` operations will fail with `header-error.immutable`.
   *
   * This headers resource is a child: it must be dropped before the parent
   * `outgoing-request` is dropped, or its ownership is transfered to
   * another component by e.g. `outgoing-handler.handle`.
   */
  /**
   * Construct a default `request-options` value.
   */
  export { RequestOptions };
  /**
   * The timeout for the initial connect to the HTTP Server.
   */
  /**
   * Set the timeout for the initial connect to the HTTP Server. An error
   * return value indicates that this timeout is not supported.
   */
  /**
   * The timeout for receiving the first byte of the Response body.
   */
  /**
   * Set the timeout for receiving the first byte of the Response body. An
   * error return value indicates that this timeout is not supported.
   */
  /**
   * The timeout for receiving subsequent chunks of bytes in the Response
   * body stream.
   */
  /**
   * Set the timeout for receiving subsequent chunks of bytes in the Response
   * body stream. An error return value indicates that this timeout is not
   * supported.
   */
  /**
   * Set the value of the `response-outparam` to either send a response,
   * or indicate an error.
   *
   * This method consumes the `response-outparam` to ensure that it is
   * called at most once. If it is never called, the implementation
   * will respond with an error.
   *
   * The user may provide an `error` to `response` to allow the
   * implementation determine how to respond with an HTTP error response.
   */
  export { ResponseOutparam };
  /**
   * Returns the status code from the incoming response.
   */
  export { IncomingResponse };
  /**
   * Returns the headers from the incoming response.
   *
   * The returned `headers` resource is immutable: `set`, `append`, and
   * `delete` operations will fail with `header-error.immutable`.
   *
   * This headers resource is a child: it must be dropped before the parent
   * `incoming-response` is dropped.
   */
  /**
   * Returns the incoming body. May be called at most once. Returns error
   * if called additional times.
   */
  /**
   * Returns the contents of the body, as a stream of bytes.
   *
   * Returns success on first call: the stream representing the contents
   * can be retrieved at most once. Subsequent calls will return error.
   *
   * The returned `input-stream` resource is a child: it must be dropped
   * before the parent `incoming-body` is dropped, or consumed by
   * `incoming-body.finish`.
   *
   * This invariant ensures that the implementation can determine whether
   * the user is consuming the contents of the body, waiting on the
   * `future-trailers` to be ready, or neither. This allows for network
   * backpressure is to be applied when the user is consuming the body,
   * and for that backpressure to not inhibit delivery of the trailers if
   * the user does not read the entire body.
   */
  export { IncomingBody };
  /**
   * Takes ownership of `incoming-body`, and returns a `future-trailers`.
   * This function will trap if the `input-stream` child is still alive.
   */
  /**
   * Returns a pollable which becomes ready when either the trailers have
   * been received, or an error has occured. When this pollable is ready,
   * the `get` method will return `some`.
   */
  export { FutureTrailers };
  /**
   * Returns the contents of the trailers, or an error which occured,
   * once the future is ready.
   *
   * The outer `option` represents future readiness. Users can wait on this
   * `option` to become `some` using the `subscribe` method.
   *
   * The outer `result` is used to retrieve the trailers or error at most
   * once. It will be success on the first call in which the outer option
   * is `some`, and error on subsequent calls.
   *
   * The inner `result` represents that either the HTTP Request or Response
   * body, as well as any trailers, were received successfully, or that an
   * error occured receiving them. The optional `trailers` indicates whether
   * or not trailers were present in the body.
   *
   * When some `trailers` are returned by this method, the `trailers`
   * resource is immutable, and a child. Use of the `set`, `append`, or
   * `delete` methods will return an error, and the resource must be
   * dropped before the parent `future-trailers` is dropped.
   */
  /**
   * Construct an `outgoing-response`, with a default `status-code` of `200`.
   * If a different `status-code` is needed, it must be set via the
   * `set-status-code` method.
   *
   * * `headers` is the HTTP Headers for the Response.
   */
  export { OutgoingResponse };
  /**
   * Get the HTTP Status Code for the Response.
   */
  /**
   * Set the HTTP Status Code for the Response. Fails if the status-code
   * given is not a valid http status code.
   */
  /**
   * Get the headers associated with the Request.
   *
   * The returned `headers` resource is immutable: `set`, `append`, and
   * `delete` operations will fail with `header-error.immutable`.
   *
   * This headers resource is a child: it must be dropped before the parent
   * `outgoing-request` is dropped, or its ownership is transfered to
   * another component by e.g. `outgoing-handler.handle`.
   */
  /**
   * Returns the resource corresponding to the outgoing Body for this Response.
   *
   * Returns success on the first call: the `outgoing-body` resource for
   * this `outgoing-response` can be retrieved at most once. Subsequent
   * calls will return error.
   */
  /**
   * Returns a stream for writing the body contents.
   *
   * The returned `output-stream` is a child resource: it must be dropped
   * before the parent `outgoing-body` resource is dropped (or finished),
   * otherwise the `outgoing-body` drop or `finish` will trap.
   *
   * Returns success on the first call: the `output-stream` resource for
   * this `outgoing-body` may be retrieved at most once. Subsequent calls
   * will return error.
   */
  export { OutgoingBody };
  /**
   * Finalize an outgoing body, optionally providing trailers. This must be
   * called to signal that the response is complete. If the `outgoing-body`
   * is dropped without calling `outgoing-body.finalize`, the implementation
   * should treat the body as corrupted.
   *
   * Fails if the body's `outgoing-request` or `outgoing-response` was
   * constructed with a Content-Length header, and the contents written
   * to the body (via `write`) does not match the value given in the
   * Content-Length.
   */
  /**
   * Returns a pollable which becomes ready when either the Response has
   * been received, or an error has occured. When this pollable is ready,
   * the `get` method will return `some`.
   */
  export { FutureIncomingResponse };
  /**
   * Returns the incoming HTTP Response, or an error, once one is ready.
   *
   * The outer `option` represents future readiness. Users can wait on this
   * `option` to become `some` using the `subscribe` method.
   *
   * The outer `result` is used to retrieve the response or error at most
   * once. It will be success on the first call in which the outer option
   * is `some`, and error on subsequent calls.
   *
   * The inner `result` represents that either the incoming HTTP Response
   * status and headers have recieved successfully, or that an error
   * occured. Errors may also occur while consuming the response body,
   * but those will be reported by the `incoming-body` and its
   * `output-stream` child.
   */
}

import type { Duration } from "./wasi-clocks-monotonic-clock.js";
export { Duration };
import type { InputStream } from "./wasi-io-streams.js";
export { InputStream };
import type { OutputStream } from "./wasi-io-streams.js";
export { OutputStream };
import type { Error as IoError } from "./wasi-io-error.js";
export { IoError };
import type { Pollable } from "./wasi-io-poll.js";
export { Pollable };

/**
 * This type corresponds to HTTP standard Methods.
 */
export type Method =
  | MethodGet
  | MethodHead
  | MethodPost
  | MethodPut
  | MethodDelete
  | MethodConnect
  | MethodOptions
  | MethodTrace
  | MethodPatch
  | MethodOther;
export interface MethodGet {
  tag: "get";
}
export interface MethodHead {
  tag: "head";
}
export interface MethodPost {
  tag: "post";
}
export interface MethodPut {
  tag: "put";
}
export interface MethodDelete {
  tag: "delete";
}
export interface MethodConnect {
  tag: "connect";
}
export interface MethodOptions {
  tag: "options";
}
export interface MethodTrace {
  tag: "trace";
}
export interface MethodPatch {
  tag: "patch";
}
export interface MethodOther {
  tag: "other";
  val: string;
}
/**
 * This type corresponds to HTTP standard Related Schemes.
 */
export type Scheme = SchemeHttp | SchemeHttps | SchemeOther;
export interface SchemeHttp {
  tag: "HTTP";
}
export interface SchemeHttps {
  tag: "HTTPS";
}
export interface SchemeOther {
  tag: "other";
  val: string;
}
/**
 * Defines the case payload type for `DNS-error` above:
 */
export interface DnsErrorPayload {
  rcode?: string;
  infoCode?: number;
}
/**
 * Defines the case payload type for `TLS-alert-received` above:
 */
export interface TlsAlertReceivedPayload {
  alertId?: number;
  alertMessage?: string;
}
/**
 * Defines the case payload type for `HTTP-response-{header,trailer}-size` above:
 */
export interface FieldSizePayload {
  fieldName?: string;
  fieldSize?: number;
}
/**
 * These cases are inspired by the IANA HTTP Proxy Error Types:
 * https://www.iana.org/assignments/http-proxy-status/http-proxy-status.xhtml#table-http-proxy-error-types
 */
export type ErrorCode =
  | ErrorCodeDnsTimeout
  | ErrorCodeDnsError
  | ErrorCodeDestinationNotFound
  | ErrorCodeDestinationUnavailable
  | ErrorCodeDestinationIpProhibited
  | ErrorCodeDestinationIpUnroutable
  | ErrorCodeConnectionRefused
  | ErrorCodeConnectionTerminated
  | ErrorCodeConnectionTimeout
  | ErrorCodeConnectionReadTimeout
  | ErrorCodeConnectionWriteTimeout
  | ErrorCodeConnectionLimitReached
  | ErrorCodeTlsProtocolError
  | ErrorCodeTlsCertificateError
  | ErrorCodeTlsAlertReceived
  | ErrorCodeHttpRequestDenied
  | ErrorCodeHttpRequestLengthRequired
  | ErrorCodeHttpRequestBodySize
  | ErrorCodeHttpRequestMethodInvalid
  | ErrorCodeHttpRequestUriInvalid
  | ErrorCodeHttpRequestUriTooLong
  | ErrorCodeHttpRequestHeaderSectionSize
  | ErrorCodeHttpRequestHeaderSize
  | ErrorCodeHttpRequestTrailerSectionSize
  | ErrorCodeHttpRequestTrailerSize
  | ErrorCodeHttpResponseIncomplete
  | ErrorCodeHttpResponseHeaderSectionSize
  | ErrorCodeHttpResponseHeaderSize
  | ErrorCodeHttpResponseBodySize
  | ErrorCodeHttpResponseTrailerSectionSize
  | ErrorCodeHttpResponseTrailerSize
  | ErrorCodeHttpResponseTransferCoding
  | ErrorCodeHttpResponseContentCoding
  | ErrorCodeHttpResponseTimeout
  | ErrorCodeHttpUpgradeFailed
  | ErrorCodeHttpProtocolError
  | ErrorCodeLoopDetected
  | ErrorCodeConfigurationError
  | ErrorCodeInternalError;
export interface ErrorCodeDnsTimeout {
  tag: "DNS-timeout";
}
export interface ErrorCodeDnsError {
  tag: "DNS-error";
  val: DnsErrorPayload;
}
export interface ErrorCodeDestinationNotFound {
  tag: "destination-not-found";
}
export interface ErrorCodeDestinationUnavailable {
  tag: "destination-unavailable";
}
export interface ErrorCodeDestinationIpProhibited {
  tag: "destination-IP-prohibited";
}
export interface ErrorCodeDestinationIpUnroutable {
  tag: "destination-IP-unroutable";
}
export interface ErrorCodeConnectionRefused {
  tag: "connection-refused";
}
export interface ErrorCodeConnectionTerminated {
  tag: "connection-terminated";
}
export interface ErrorCodeConnectionTimeout {
  tag: "connection-timeout";
}
export interface ErrorCodeConnectionReadTimeout {
  tag: "connection-read-timeout";
}
export interface ErrorCodeConnectionWriteTimeout {
  tag: "connection-write-timeout";
}
export interface ErrorCodeConnectionLimitReached {
  tag: "connection-limit-reached";
}
export interface ErrorCodeTlsProtocolError {
  tag: "TLS-protocol-error";
}
export interface ErrorCodeTlsCertificateError {
  tag: "TLS-certificate-error";
}
export interface ErrorCodeTlsAlertReceived {
  tag: "TLS-alert-received";
  val: TlsAlertReceivedPayload;
}
export interface ErrorCodeHttpRequestDenied {
  tag: "HTTP-request-denied";
}
export interface ErrorCodeHttpRequestLengthRequired {
  tag: "HTTP-request-length-required";
}
export interface ErrorCodeHttpRequestBodySize {
  tag: "HTTP-request-body-size";
  val: bigint | undefined;
}
export interface ErrorCodeHttpRequestMethodInvalid {
  tag: "HTTP-request-method-invalid";
}
export interface ErrorCodeHttpRequestUriInvalid {
  tag: "HTTP-request-URI-invalid";
}
export interface ErrorCodeHttpRequestUriTooLong {
  tag: "HTTP-request-URI-too-long";
}
export interface ErrorCodeHttpRequestHeaderSectionSize {
  tag: "HTTP-request-header-section-size";
  val: number | undefined;
}
export interface ErrorCodeHttpRequestHeaderSize {
  tag: "HTTP-request-header-size";
  val: FieldSizePayload | undefined;
}
export interface ErrorCodeHttpRequestTrailerSectionSize {
  tag: "HTTP-request-trailer-section-size";
  val: number | undefined;
}
export interface ErrorCodeHttpRequestTrailerSize {
  tag: "HTTP-request-trailer-size";
  val: FieldSizePayload;
}
export interface ErrorCodeHttpResponseIncomplete {
  tag: "HTTP-response-incomplete";
}
export interface ErrorCodeHttpResponseHeaderSectionSize {
  tag: "HTTP-response-header-section-size";
  val: number | undefined;
}
export interface ErrorCodeHttpResponseHeaderSize {
  tag: "HTTP-response-header-size";
  val: FieldSizePayload;
}
export interface ErrorCodeHttpResponseBodySize {
  tag: "HTTP-response-body-size";
  val: bigint | undefined;
}
export interface ErrorCodeHttpResponseTrailerSectionSize {
  tag: "HTTP-response-trailer-section-size";
  val: number | undefined;
}
export interface ErrorCodeHttpResponseTrailerSize {
  tag: "HTTP-response-trailer-size";
  val: FieldSizePayload;
}
export interface ErrorCodeHttpResponseTransferCoding {
  tag: "HTTP-response-transfer-coding";
  val: string | undefined;
}
export interface ErrorCodeHttpResponseContentCoding {
  tag: "HTTP-response-content-coding";
  val: string | undefined;
}
export interface ErrorCodeHttpResponseTimeout {
  tag: "HTTP-response-timeout";
}
export interface ErrorCodeHttpUpgradeFailed {
  tag: "HTTP-upgrade-failed";
}
export interface ErrorCodeHttpProtocolError {
  tag: "HTTP-protocol-error";
}
export interface ErrorCodeLoopDetected {
  tag: "loop-detected";
}
export interface ErrorCodeConfigurationError {
  tag: "configuration-error";
}
/**
 * This is a catch-all error for anything that doesn't fit cleanly into a
 * more specific case. It also includes an optional string for an
 * unstructured description of the error. Users should not depend on the
 * string for diagnosing errors, as it's not required to be consistent
 * between implementations.
 */
export interface ErrorCodeInternalError {
  tag: "internal-error";
  val: string | undefined;
}
/**
 * This type enumerates the different kinds of errors that may occur when
 * setting or appending to a `fields` resource.
 */
export type HeaderError =
  | HeaderErrorInvalidSyntax
  | HeaderErrorForbidden
  | HeaderErrorImmutable;
/**
 * This error indicates that a `field-key` or `field-value` was
 * syntactically invalid when used with an operation that sets headers in a
 * `fields`.
 */
export interface HeaderErrorInvalidSyntax {
  tag: "invalid-syntax";
}
/**
 * This error indicates that a forbidden `field-key` was used when trying
 * to set a header in a `fields`.
 */
export interface HeaderErrorForbidden {
  tag: "forbidden";
}
/**
 * This error indicates that the operation on the `fields` was not
 * permitted because the fields are immutable.
 */
export interface HeaderErrorImmutable {
  tag: "immutable";
}
/**
 * Field keys are always strings.
 */
export type FieldKey = string;
/**
 * Field values should always be ASCII strings. However, in
 * reality, HTTP implementations often have to interpret malformed values,
 * so they are provided as a list of bytes.
 */
export type FieldValue = Uint8Array;
/**
 * Headers is an alias for Fields.
 */
export type Headers = Fields;
/**
 * Trailers is an alias for Fields.
 */
export type Trailers = Fields;
/**
 * This type corresponds to the HTTP standard Status Code.
 */
export type StatusCode = number;
export type Result<T, E> = { tag: "ok"; val: T } | { tag: "err"; val: E };

export class OutgoingBody {
  write(): OutputStream;
  static finish(this_: OutgoingBody, trailers: Trailers | undefined): void;
}

export class Fields {
  constructor();
  static fromList(entries: [FieldKey, FieldValue][]): Fields;
  get(name: FieldKey): FieldValue[];
  has(name: FieldKey): boolean;
  set(name: FieldKey, value: FieldValue[]): void;
  delete(name: FieldKey): void;
  append(name: FieldKey, value: FieldValue): void;
  entries(): [FieldKey, FieldValue][];
  clone(): Fields;
}

export class FutureIncomingResponse {
  subscribe(): Pollable;
  get(): Result<Result<IncomingResponse, ErrorCode>, void> | undefined;
}

export class IncomingRequest {
  method(): Method;
  pathWithQuery(): string | undefined;
  scheme(): Scheme | undefined;
  authority(): string | undefined;
  headers(): Headers;
  consume(): IncomingBody;
}

export class IncomingBody {
  stream(): InputStream;
  static finish(this_: IncomingBody): FutureTrailers;
}

export class FutureTrailers {
  subscribe(): Pollable;
  get(): Result<Result<Trailers | undefined, ErrorCode>, void> | undefined;
}

export class IncomingResponse {
  status(): StatusCode;
  headers(): Headers;
  consume(): IncomingBody;
}

export class OutgoingResponse {
  constructor(headers: Headers);
  statusCode(): StatusCode;
  setStatusCode(statusCode: StatusCode): void;
  headers(): Headers;
  body(): OutgoingBody;
}

export class OutgoingRequest {
  constructor(headers: Headers);
  body(): OutgoingBody;
  method(): Method;
  setMethod(method: Method): void;
  pathWithQuery(): string | undefined;
  setPathWithQuery(pathWithQuery: string | undefined): void;
  scheme(): Scheme | undefined;
  setScheme(scheme: Scheme | undefined): void;
  authority(): string | undefined;
  setAuthority(authority: string | undefined): void;
  headers(): Headers;
}

export class RequestOptions {
  constructor();
  connectTimeout(): Duration | undefined;
  setConnectTimeout(duration: Duration | undefined): void;
  firstByteTimeout(): Duration | undefined;
  setFirstByteTimeout(duration: Duration | undefined): void;
  betweenBytesTimeout(): Duration | undefined;
  setBetweenBytesTimeout(duration: Duration | undefined): void;
}

export class ResponseOutparam {
  static set(
    param: ResponseOutparam,
    response: Result<OutgoingResponse, ErrorCode>
  ): void;
}
