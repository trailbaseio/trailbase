/** @module Interface wasi:http/types@0.2.3 **/
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
export type Duration = import('./wasi-clocks-monotonic-clock.js').Duration;
export type InputStream = import('./wasi-io-streams.js').InputStream;
export type OutputStream = import('./wasi-io-streams.js').OutputStream;
export type IoError = import('./wasi-io-error.js').Error;
export type Pollable = import('./wasi-io-poll.js').Pollable;
/**
 * This type corresponds to HTTP standard Methods.
 */
export type Method = MethodGet | MethodHead | MethodPost | MethodPut | MethodDelete | MethodConnect | MethodOptions | MethodTrace | MethodPatch | MethodOther;
export interface MethodGet {
  tag: 'get',
}
export interface MethodHead {
  tag: 'head',
}
export interface MethodPost {
  tag: 'post',
}
export interface MethodPut {
  tag: 'put',
}
export interface MethodDelete {
  tag: 'delete',
}
export interface MethodConnect {
  tag: 'connect',
}
export interface MethodOptions {
  tag: 'options',
}
export interface MethodTrace {
  tag: 'trace',
}
export interface MethodPatch {
  tag: 'patch',
}
export interface MethodOther {
  tag: 'other',
  val: string,
}
/**
 * This type corresponds to HTTP standard Related Schemes.
 */
export type Scheme = SchemeHttp | SchemeHttps | SchemeOther;
export interface SchemeHttp {
  tag: 'HTTP',
}
export interface SchemeHttps {
  tag: 'HTTPS',
}
export interface SchemeOther {
  tag: 'other',
  val: string,
}
/**
 * Defines the case payload type for `DNS-error` above:
 */
export interface DnsErrorPayload {
  rcode?: string,
  infoCode?: number,
}
/**
 * Defines the case payload type for `TLS-alert-received` above:
 */
export interface TlsAlertReceivedPayload {
  alertId?: number,
  alertMessage?: string,
}
/**
 * Defines the case payload type for `HTTP-response-{header,trailer}-size` above:
 */
export interface FieldSizePayload {
  fieldName?: string,
  fieldSize?: number,
}
/**
 * These cases are inspired by the IANA HTTP Proxy Error Types:
 *   <https://www.iana.org/assignments/http-proxy-status/http-proxy-status.xhtml#table-http-proxy-error-types>
 */
export type ErrorCode = ErrorCodeDnsTimeout | ErrorCodeDnsError | ErrorCodeDestinationNotFound | ErrorCodeDestinationUnavailable | ErrorCodeDestinationIpProhibited | ErrorCodeDestinationIpUnroutable | ErrorCodeConnectionRefused | ErrorCodeConnectionTerminated | ErrorCodeConnectionTimeout | ErrorCodeConnectionReadTimeout | ErrorCodeConnectionWriteTimeout | ErrorCodeConnectionLimitReached | ErrorCodeTlsProtocolError | ErrorCodeTlsCertificateError | ErrorCodeTlsAlertReceived | ErrorCodeHttpRequestDenied | ErrorCodeHttpRequestLengthRequired | ErrorCodeHttpRequestBodySize | ErrorCodeHttpRequestMethodInvalid | ErrorCodeHttpRequestUriInvalid | ErrorCodeHttpRequestUriTooLong | ErrorCodeHttpRequestHeaderSectionSize | ErrorCodeHttpRequestHeaderSize | ErrorCodeHttpRequestTrailerSectionSize | ErrorCodeHttpRequestTrailerSize | ErrorCodeHttpResponseIncomplete | ErrorCodeHttpResponseHeaderSectionSize | ErrorCodeHttpResponseHeaderSize | ErrorCodeHttpResponseBodySize | ErrorCodeHttpResponseTrailerSectionSize | ErrorCodeHttpResponseTrailerSize | ErrorCodeHttpResponseTransferCoding | ErrorCodeHttpResponseContentCoding | ErrorCodeHttpResponseTimeout | ErrorCodeHttpUpgradeFailed | ErrorCodeHttpProtocolError | ErrorCodeLoopDetected | ErrorCodeConfigurationError | ErrorCodeInternalError;
export interface ErrorCodeDnsTimeout {
  tag: 'DNS-timeout',
}
export interface ErrorCodeDnsError {
  tag: 'DNS-error',
  val: DnsErrorPayload,
}
export interface ErrorCodeDestinationNotFound {
  tag: 'destination-not-found',
}
export interface ErrorCodeDestinationUnavailable {
  tag: 'destination-unavailable',
}
export interface ErrorCodeDestinationIpProhibited {
  tag: 'destination-IP-prohibited',
}
export interface ErrorCodeDestinationIpUnroutable {
  tag: 'destination-IP-unroutable',
}
export interface ErrorCodeConnectionRefused {
  tag: 'connection-refused',
}
export interface ErrorCodeConnectionTerminated {
  tag: 'connection-terminated',
}
export interface ErrorCodeConnectionTimeout {
  tag: 'connection-timeout',
}
export interface ErrorCodeConnectionReadTimeout {
  tag: 'connection-read-timeout',
}
export interface ErrorCodeConnectionWriteTimeout {
  tag: 'connection-write-timeout',
}
export interface ErrorCodeConnectionLimitReached {
  tag: 'connection-limit-reached',
}
export interface ErrorCodeTlsProtocolError {
  tag: 'TLS-protocol-error',
}
export interface ErrorCodeTlsCertificateError {
  tag: 'TLS-certificate-error',
}
export interface ErrorCodeTlsAlertReceived {
  tag: 'TLS-alert-received',
  val: TlsAlertReceivedPayload,
}
export interface ErrorCodeHttpRequestDenied {
  tag: 'HTTP-request-denied',
}
export interface ErrorCodeHttpRequestLengthRequired {
  tag: 'HTTP-request-length-required',
}
export interface ErrorCodeHttpRequestBodySize {
  tag: 'HTTP-request-body-size',
  val: bigint | undefined,
}
export interface ErrorCodeHttpRequestMethodInvalid {
  tag: 'HTTP-request-method-invalid',
}
export interface ErrorCodeHttpRequestUriInvalid {
  tag: 'HTTP-request-URI-invalid',
}
export interface ErrorCodeHttpRequestUriTooLong {
  tag: 'HTTP-request-URI-too-long',
}
export interface ErrorCodeHttpRequestHeaderSectionSize {
  tag: 'HTTP-request-header-section-size',
  val: number | undefined,
}
export interface ErrorCodeHttpRequestHeaderSize {
  tag: 'HTTP-request-header-size',
  val: FieldSizePayload | undefined,
}
export interface ErrorCodeHttpRequestTrailerSectionSize {
  tag: 'HTTP-request-trailer-section-size',
  val: number | undefined,
}
export interface ErrorCodeHttpRequestTrailerSize {
  tag: 'HTTP-request-trailer-size',
  val: FieldSizePayload,
}
export interface ErrorCodeHttpResponseIncomplete {
  tag: 'HTTP-response-incomplete',
}
export interface ErrorCodeHttpResponseHeaderSectionSize {
  tag: 'HTTP-response-header-section-size',
  val: number | undefined,
}
export interface ErrorCodeHttpResponseHeaderSize {
  tag: 'HTTP-response-header-size',
  val: FieldSizePayload,
}
export interface ErrorCodeHttpResponseBodySize {
  tag: 'HTTP-response-body-size',
  val: bigint | undefined,
}
export interface ErrorCodeHttpResponseTrailerSectionSize {
  tag: 'HTTP-response-trailer-section-size',
  val: number | undefined,
}
export interface ErrorCodeHttpResponseTrailerSize {
  tag: 'HTTP-response-trailer-size',
  val: FieldSizePayload,
}
export interface ErrorCodeHttpResponseTransferCoding {
  tag: 'HTTP-response-transfer-coding',
  val: string | undefined,
}
export interface ErrorCodeHttpResponseContentCoding {
  tag: 'HTTP-response-content-coding',
  val: string | undefined,
}
export interface ErrorCodeHttpResponseTimeout {
  tag: 'HTTP-response-timeout',
}
export interface ErrorCodeHttpUpgradeFailed {
  tag: 'HTTP-upgrade-failed',
}
export interface ErrorCodeHttpProtocolError {
  tag: 'HTTP-protocol-error',
}
export interface ErrorCodeLoopDetected {
  tag: 'loop-detected',
}
export interface ErrorCodeConfigurationError {
  tag: 'configuration-error',
}
/**
 * This is a catch-all error for anything that doesn't fit cleanly into a
 * more specific case. It also includes an optional string for an
 * unstructured description of the error. Users should not depend on the
 * string for diagnosing errors, as it's not required to be consistent
 * between implementations.
 */
export interface ErrorCodeInternalError {
  tag: 'internal-error',
  val: string | undefined,
}
/**
 * This type enumerates the different kinds of errors that may occur when
 * setting or appending to a `fields` resource.
 */
export type HeaderError = HeaderErrorInvalidSyntax | HeaderErrorForbidden | HeaderErrorImmutable;
/**
 * This error indicates that a `field-name` or `field-value` was
 * syntactically invalid when used with an operation that sets headers in a
 * `fields`.
 */
export interface HeaderErrorInvalidSyntax {
  tag: 'invalid-syntax',
}
/**
 * This error indicates that a forbidden `field-name` was used when trying
 * to set a header in a `fields`.
 */
export interface HeaderErrorForbidden {
  tag: 'forbidden',
}
/**
 * This error indicates that the operation on the `fields` was not
 * permitted because the fields are immutable.
 */
export interface HeaderErrorImmutable {
  tag: 'immutable',
}
/**
 * Field keys are always strings.
 *
 * Field keys should always be treated as case insensitive by the `fields`
 * resource for the purposes of equality checking.
 *
 * # Deprecation
 *
 * This type has been deprecated in favor of the `field-name` type.
 */
export type FieldKey = string;
/**
 * Field names are always strings.
 *
 * Field names should always be treated as case insensitive by the `fields`
 * resource for the purposes of equality checking.
 */
export type FieldName = FieldKey;
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
export type Result<T, E> = { tag: 'ok', val: T } | { tag: 'err', val: E };

export class Fields implements Partial<Disposable> {
  /**
  * Construct an empty HTTP Fields.
  *
  * The resulting `fields` is mutable.
  */
  constructor()
  /**
  * Construct an HTTP Fields.
  *
  * The resulting `fields` is mutable.
  *
  * The list represents each name-value pair in the Fields. Names
  * which have multiple values are represented by multiple entries in this
  * list with the same name.
  *
  * The tuple is a pair of the field name, represented as a string, and
  * Value, represented as a list of bytes.
  *
  * An error result will be returned if any `field-name` or `field-value` is
  * syntactically invalid, or if a field is forbidden.
  */
  static fromList(entries: Array<[FieldName, FieldValue]>): Fields;
  /**
  * Get all of the values corresponding to a name. If the name is not present
  * in this `fields` or is syntactically invalid, an empty list is returned.
  * However, if the name is present but empty, this is represented by a list
  * with one or more empty field-values present.
  */
  get(name: FieldName): Array<FieldValue>;
  /**
  * Returns `true` when the name is present in this `fields`. If the name is
  * syntactically invalid, `false` is returned.
  */
  has(name: FieldName): boolean;
  /**
  * Set all of the values for a name. Clears any existing values for that
  * name, if they have been set.
  *
  * Fails with `header-error.immutable` if the `fields` are immutable.
  *
  * Fails with `header-error.invalid-syntax` if the `field-name` or any of
  * the `field-value`s are syntactically invalid.
  */
  set(name: FieldName, value: Array<FieldValue>): void;
  /**
  * Delete all values for a name. Does nothing if no values for the name
  * exist.
  *
  * Fails with `header-error.immutable` if the `fields` are immutable.
  *
  * Fails with `header-error.invalid-syntax` if the `field-name` is
  * syntactically invalid.
  */
  'delete'(name: FieldName): void;
  /**
  * Append a value for a name. Does not change or delete any existing
  * values for that name.
  *
  * Fails with `header-error.immutable` if the `fields` are immutable.
  *
  * Fails with `header-error.invalid-syntax` if the `field-name` or
  * `field-value` are syntactically invalid.
  */
  append(name: FieldName, value: FieldValue): void;
  /**
  * Retrieve the full set of names and values in the Fields. Like the
  * constructor, the list represents each name-value pair.
  *
  * The outer list represents each name-value pair in the Fields. Names
  * which have multiple values are represented by multiple entries in this
  * list with the same name.
  *
  * The names and values are always returned in the original casing and in
  * the order in which they will be serialized for transport.
  */
  entries(): Array<[FieldName, FieldValue]>;
  /**
  * Make a deep copy of the Fields. Equivalent in behavior to calling the
  * `fields` constructor on the return value of `entries`. The resulting
  * `fields` is mutable.
  */
  clone(): Fields;
    [Symbol.dispose]?: () => void;
}

export class FutureIncomingResponse implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
  /**
  * Returns a pollable which becomes ready when either the Response has
  * been received, or an error has occurred. When this pollable is ready,
  * the `get` method will return `some`.
  */
  subscribe(): Pollable;
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
  * status and headers have received successfully, or that an error
  * occurred. Errors may also occur while consuming the response body,
  * but those will be reported by the `incoming-body` and its
  * `output-stream` child.
  */
  get(): Result<Result<IncomingResponse, ErrorCode>, void> | undefined;
    [Symbol.dispose]?: () => void;
}

export class FutureTrailers implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
  /**
  * Returns a pollable which becomes ready when either the trailers have
  * been received, or an error has occurred. When this pollable is ready,
  * the `get` method will return `some`.
  */
  subscribe(): Pollable;
  /**
  * Returns the contents of the trailers, or an error which occurred,
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
  * error occurred receiving them. The optional `trailers` indicates whether
  * or not trailers were present in the body.
  *
  * When some `trailers` are returned by this method, the `trailers`
  * resource is immutable, and a child. Use of the `set`, `append`, or
  * `delete` methods will return an error, and the resource must be
  * dropped before the parent `future-trailers` is dropped.
  */
  get(): Result<Result<Trailers | undefined, ErrorCode>, void> | undefined;
    [Symbol.dispose]?: () => void;
}

export class IncomingBody implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
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
  stream(): InputStream;
  /**
  * Takes ownership of `incoming-body`, and returns a `future-trailers`.
  * This function will trap if the `input-stream` child is still alive.
  */
  static finish(this_: IncomingBody): FutureTrailers;
    [Symbol.dispose]?: () => void;
}

export class IncomingRequest implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
  /**
  * Returns the method of the incoming request.
  */
  method(): Method;
  /**
  * Returns the path with query parameters from the request, as a string.
  */
  pathWithQuery(): string | undefined;
  /**
  * Returns the protocol scheme from the request.
  */
  scheme(): Scheme | undefined;
  /**
  * Returns the authority of the Request's target URI, if present.
  */
  authority(): string | undefined;
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
  headers(): Headers;
  /**
  * Gives the `incoming-body` associated with this request. Will only
  * return success at most once, and subsequent calls will return error.
  */
  consume(): IncomingBody;
    [Symbol.dispose]?: () => void;
}

export class IncomingResponse implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
  /**
  * Returns the status code from the incoming response.
  */
  status(): StatusCode;
  /**
  * Returns the headers from the incoming response.
  *
  * The returned `headers` resource is immutable: `set`, `append`, and
  * `delete` operations will fail with `header-error.immutable`.
  *
  * This headers resource is a child: it must be dropped before the parent
  * `incoming-response` is dropped.
  */
  headers(): Headers;
  /**
  * Returns the incoming body. May be called at most once. Returns error
  * if called additional times.
  */
  consume(): IncomingBody;
    [Symbol.dispose]?: () => void;
}

export class OutgoingBody implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
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
  write(): OutputStream;
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
  static finish(this_: OutgoingBody, trailers: Trailers | undefined): void;
    [Symbol.dispose]?: () => void;
}

export class OutgoingRequest implements Partial<Disposable> {
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
  constructor(headers: Headers)
  /**
  * Returns the resource corresponding to the outgoing Body for this
  * Request.
  *
  * Returns success on the first call: the `outgoing-body` resource for
  * this `outgoing-request` can be retrieved at most once. Subsequent
  * calls will return error.
  */
  body(): OutgoingBody;
  /**
  * Get the Method for the Request.
  */
  method(): Method;
  /**
  * Set the Method for the Request. Fails if the string present in a
  * `method.other` argument is not a syntactically valid method.
  */
  setMethod(method: Method): void;
  /**
  * Get the combination of the HTTP Path and Query for the Request.
  * When `none`, this represents an empty Path and empty Query.
  */
  pathWithQuery(): string | undefined;
  /**
  * Set the combination of the HTTP Path and Query for the Request.
  * When `none`, this represents an empty Path and empty Query. Fails is the
  * string given is not a syntactically valid path and query uri component.
  */
  setPathWithQuery(pathWithQuery: string | undefined): void;
  /**
  * Get the HTTP Related Scheme for the Request. When `none`, the
  * implementation may choose an appropriate default scheme.
  */
  scheme(): Scheme | undefined;
  /**
  * Set the HTTP Related Scheme for the Request. When `none`, the
  * implementation may choose an appropriate default scheme. Fails if the
  * string given is not a syntactically valid uri scheme.
  */
  setScheme(scheme: Scheme | undefined): void;
  /**
  * Get the authority of the Request's target URI. A value of `none` may be used
  * with Related Schemes which do not require an authority. The HTTP and
  * HTTPS schemes always require an authority.
  */
  authority(): string | undefined;
  /**
  * Set the authority of the Request's target URI. A value of `none` may be used
  * with Related Schemes which do not require an authority. The HTTP and
  * HTTPS schemes always require an authority. Fails if the string given is
  * not a syntactically valid URI authority.
  */
  setAuthority(authority: string | undefined): void;
  /**
  * Get the headers associated with the Request.
  *
  * The returned `headers` resource is immutable: `set`, `append`, and
  * `delete` operations will fail with `header-error.immutable`.
  *
  * This headers resource is a child: it must be dropped before the parent
  * `outgoing-request` is dropped, or its ownership is transferred to
  * another component by e.g. `outgoing-handler.handle`.
  */
  headers(): Headers;
    [Symbol.dispose]?: () => void;
}

export class OutgoingResponse implements Partial<Disposable> {
  /**
  * Construct an `outgoing-response`, with a default `status-code` of `200`.
  * If a different `status-code` is needed, it must be set via the
  * `set-status-code` method.
  *
  * * `headers` is the HTTP Headers for the Response.
  */
  constructor(headers: Headers)
  /**
  * Get the HTTP Status Code for the Response.
  */
  statusCode(): StatusCode;
  /**
  * Set the HTTP Status Code for the Response. Fails if the status-code
  * given is not a valid http status code.
  */
  setStatusCode(statusCode: StatusCode): void;
  /**
  * Get the headers associated with the Request.
  *
  * The returned `headers` resource is immutable: `set`, `append`, and
  * `delete` operations will fail with `header-error.immutable`.
  *
  * This headers resource is a child: it must be dropped before the parent
  * `outgoing-request` is dropped, or its ownership is transferred to
  * another component by e.g. `outgoing-handler.handle`.
  */
  headers(): Headers;
  /**
  * Returns the resource corresponding to the outgoing Body for this Response.
  *
  * Returns success on the first call: the `outgoing-body` resource for
  * this `outgoing-response` can be retrieved at most once. Subsequent
  * calls will return error.
  */
  body(): OutgoingBody;
    [Symbol.dispose]?: () => void;
}

export class RequestOptions implements Partial<Disposable> {
  /**
  * Construct a default `request-options` value.
  */
  constructor()
  /**
  * The timeout for the initial connect to the HTTP Server.
  */
  connectTimeout(): Duration | undefined;
  /**
  * Set the timeout for the initial connect to the HTTP Server. An error
  * return value indicates that this timeout is not supported.
  */
  setConnectTimeout(duration: Duration | undefined): void;
  /**
  * The timeout for receiving the first byte of the Response body.
  */
  firstByteTimeout(): Duration | undefined;
  /**
  * Set the timeout for receiving the first byte of the Response body. An
  * error return value indicates that this timeout is not supported.
  */
  setFirstByteTimeout(duration: Duration | undefined): void;
  /**
  * The timeout for receiving subsequent chunks of bytes in the Response
  * body stream.
  */
  betweenBytesTimeout(): Duration | undefined;
  /**
  * Set the timeout for receiving subsequent chunks of bytes in the Response
  * body stream. An error return value indicates that this timeout is not
  * supported.
  */
  setBetweenBytesTimeout(duration: Duration | undefined): void;
    [Symbol.dispose]?: () => void;
}

export class ResponseOutparam implements Partial<Disposable> {
  /**
   * This type does not have a public constructor.
   */
  private constructor();
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
  static set(param: ResponseOutparam, response: Result<OutgoingResponse, ErrorCode>): void;
    [Symbol.dispose]?: () => void;
}
