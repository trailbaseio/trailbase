import { Fields, OutgoingBody, OutgoingResponse } from "wasi:http/types@0.2.3";

import { StatusCode } from "./status";
import { Request, type Method } from "./request";
import { encodeBytes } from "./incoming";

// Override setInterval/setTimeout.
import "../timer";

export { OutgoingResponse } from "wasi:http/types@0.2.3";
export { StatusCode } from "./status";
export type { Method, Request, Scheme, User } from "./request";

export type ResponseType = string | Uint8Array | OutgoingResponse | void;
export type HttpHandlerCallback = (
  req: Request,
) => ResponseType | Promise<ResponseType>;

export interface HttpHandlerInterface {
  path: string;
  method: Method;
  handler: HttpHandlerCallback;
}

export class HttpHandler implements HttpHandlerInterface {
  constructor(
    public readonly path: string,
    public readonly method: Method,
    public readonly handler: HttpHandlerCallback,
  ) {}

  static get(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "get", handler);
  }

  static post(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "post", handler);
  }

  static head(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "head", handler);
  }

  static options(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "options", handler);
  }

  static patch(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "patch", handler);
  }

  static delete(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "delete", handler);
  }

  static put(path: string, handler: HttpHandlerCallback): HttpHandler {
    return new HttpHandler(path, "put", handler);
  }
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
}

export type ResponseOptions = {
  status?: StatusCode;
  headers?: [string, Uint8Array][];
};

export function buildJsonResponse(
  body: object,
  opts?: ResponseOptions,
): OutgoingResponse {
  return buildResponse(encodeBytes(JSON.stringify(body)), {
    ...opts,

    headers: [
      ["Content-Type", encodeBytes("application/json")],
      ...(opts?.headers ?? []),
    ],
  });
}

export function buildResponse(
  body: Uint8Array,
  opts?: ResponseOptions,
): OutgoingResponse {
  // NOTE: `outputStream.blockingWriteAndFlush` only writes up to 4kB, see documentation.
  if (body.length <= 4096) {
    return buildSmallResponse(body, opts);
  }
  return buildLargeResponse(body, opts);
}

function buildSmallResponse(
  body: Uint8Array,
  opts?: ResponseOptions,
): OutgoingResponse {
  const outgoingResponse = new OutgoingResponse(
    Fields.fromList(opts?.headers ?? []),
  );

  const outgoingBody = outgoingResponse.body();
  {
    // Create a stream for the response body
    const outputStream = outgoingBody.write();
    outputStream.blockingWriteAndFlush(body);

    outputStream[Symbol.dispose]?.();
  }

  outgoingResponse.setStatusCode(opts?.status ?? StatusCode.OK);

  OutgoingBody.finish(outgoingBody, undefined);

  return outgoingResponse;
}

function buildLargeResponse(
  body: Uint8Array,
  opts?: ResponseOptions,
): OutgoingResponse {
  const outgoingResponse = new OutgoingResponse(
    Fields.fromList(opts?.headers ?? []),
  );

  const outgoingBody = outgoingResponse.body();
  {
    const outputStream = outgoingBody.write();

    // Retrieve a Preview 2 I/O pollable to coordinate writing to the output stream
    const pollable = outputStream.subscribe();

    let written = 0n;
    let remaining = BigInt(body.length);
    while (remaining > 0) {
      // Wait for the stream to become writable
      pollable.block();

      // Get the amount of bytes that we're allowed to write
      let writableByteCount = outputStream.checkWrite();
      if (remaining <= writableByteCount) {
        writableByteCount = BigInt(remaining);
      }

      // If we are not allowed to write any more, but there are still bytes
      // remaining then flush and try again
      if (writableByteCount === 0n && remaining !== 0n) {
        outputStream.flush();
        continue;
      }

      outputStream.write(
        new Uint8Array(body.buffer, Number(written), Number(writableByteCount)),
      );
      written += writableByteCount;
      remaining -= written;

      // While we can track *when* to flush separately and implement our own logic,
      // the simplest way is to flush the written chunk immediately
      outputStream.flush();
    }

    pollable[Symbol.dispose]?.();
    outputStream[Symbol.dispose]?.();
  }

  outgoingResponse.setStatusCode(opts?.status ?? StatusCode.OK);

  OutgoingBody.finish(outgoingBody, undefined);

  return outgoingResponse;
}

// function writeResponseOriginal(
//   responseOutparam: ResponseOutparam,
//   status: number,
//   body: Uint8Array,
// ) {
//   /* eslint-disable prefer-const */
//   const outgoingResponse = new OutgoingResponse(new Fields());
//
//   let outgoingBody = outgoingResponse.body();
//   {
//     // Create a stream for the response body
//     let outputStream = outgoingBody.write();
//     outputStream.blockingWriteAndFlush(body);
//
//     // eslint-disable-next-line @typescript-eslint/ban-ts-comment
//     // @ts-ignore: This is required in order to dispose the stream before we return
//     outputStream[Symbol.dispose]();
//     //outputStream[Symbol.dispose]?.();
//   }
//
//   outgoingResponse.setStatusCode(status);
//   OutgoingBody.finish(outgoingBody, undefined);
//
//   ResponseOutparam.set(responseOutparam, { tag: "ok", val: outgoingResponse });
// }
