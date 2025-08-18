/** @module Interface wasi:http/outgoing-handler@0.2.3 **/
/**
 * This function is invoked with an outgoing HTTP Request, and it returns
 * a resource `future-incoming-response` which represents an HTTP Response
 * which may arrive in the future.
 * 
 * The `options` argument accepts optional parameters for the HTTP
 * protocol's transport layer.
 * 
 * This function may return an error if the `outgoing-request` is invalid
 * or not allowed to be made. Otherwise, protocol errors are reported
 * through the `future-incoming-response`.
 */
export function handle(request: OutgoingRequest, options: RequestOptions | undefined): FutureIncomingResponse;
export type OutgoingRequest = import('./wasi-http-types.js').OutgoingRequest;
export type RequestOptions = import('./wasi-http-types.js').RequestOptions;
export type FutureIncomingResponse = import('./wasi-http-types.js').FutureIncomingResponse;
export type ErrorCode = import('./wasi-http-types.js').ErrorCode;
