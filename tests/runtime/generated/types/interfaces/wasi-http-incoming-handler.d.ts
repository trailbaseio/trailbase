/** @module Interface wasi:http/incoming-handler@0.2.3 **/
/**
 * This function is invoked with an incoming HTTP Request, and a resource
 * `response-outparam` which provides the capability to reply with an HTTP
 * Response. The response is sent by calling the `response-outparam.set`
 * method, which allows execution to continue after the response has been
 * sent. This enables both streaming to the response body, and performing other
 * work.
 * 
 * The implementor of this function must write a response to the
 * `response-outparam` before returning, or else the caller will respond
 * with an error on its behalf.
 */
export function handle(request: IncomingRequest, responseOut: ResponseOutparam): void;
export type IncomingRequest = import('./wasi-http-types.js').IncomingRequest;
export type ResponseOutparam = import('./wasi-http-types.js').ResponseOutparam;
