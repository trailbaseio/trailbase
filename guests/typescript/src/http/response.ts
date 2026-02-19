import { Fields, OutgoingBody, OutgoingResponse } from "wasi:http/types@0.2.3";
import { StatusCode } from "./status";
import { encodeBytes } from "./incoming";

export class HttpResponse {
  protected constructor(
    public readonly status: StatusCode,
    public body?: Uint8Array,
    public headers: [string, Uint8Array][] = [],
  ) {}

  public static status(
    status: StatusCode | number,
    body?: string | Uint8Array,
  ): HttpResponse {
    return new HttpResponse(
      status,
      typeof body === "string" ? encodeBytes(body) : body,
    );
  }

  public static ok(body?: string | Uint8Array): HttpResponse {
    return new HttpResponse(
      StatusCode.OK,
      typeof body === "string" ? encodeBytes(body) : body,
    );
  }

  public static text(body: string | Uint8Array): HttpResponse {
    return new HttpResponse(
      StatusCode.OK,
      typeof body === "string" ? encodeBytes(body) : body,
      [["Content-Type", encodeBytes("text/plain; charset=utf-8")]],
    );
  }

  public static json(value: object): HttpResponse {
    return new HttpResponse(StatusCode.OK, encodeBytes(JSON.stringify(value)), [
      ["Content-Type", encodeBytes("application/json")],
    ]);
  }

  public setBody(body: string | Uint8Array): HttpResponse {
    this.body = typeof body === "string" ? encodeBytes(body) : body;
    return this;
  }

  public setHeader(key: string, value: string | Uint8Array): HttpResponse {
    this.headers.push([
      key,
      typeof value === "string" ? encodeBytes(value) : value,
    ]);
    return this;
  }
}

export class HttpError extends Error {
  public constructor(
    public readonly status: StatusCode,
    message?: string,
  ) {
    super(message);
  }

  public static from(status: StatusCode | number, message?: string): HttpError {
    return new HttpError(status, message);
  }

  public override toString(): string {
    return `HttpError(${this.status}, ${this.message})`;
  }
}

type ResponseOptions = {
  status: StatusCode;
  headers: [string, Uint8Array][];
  body: Uint8Array;
};

export function buildResponse(opts: ResponseOptions): OutgoingResponse {
  // NOTE: `outputStream.blockingWriteAndFlush` only writes up to 4kB, see documentation.
  if (opts.body.length <= 4096) {
    return buildSmallResponse(opts);
  }
  return buildLargeResponse(opts);
}

function buildSmallResponse({
  status,
  headers,
  body,
}: ResponseOptions): OutgoingResponse {
  const outgoingResponse = new OutgoingResponse(Fields.fromList(headers));

  const outgoingBody = outgoingResponse.body();
  {
    // Create a stream for the response body
    const outputStream = outgoingBody.write();
    outputStream.blockingWriteAndFlush(body);

    outputStream[Symbol.dispose]?.();
  }

  outgoingResponse.setStatusCode(status);

  OutgoingBody.finish(outgoingBody, undefined);

  return outgoingResponse;
}

function buildLargeResponse({
  status,
  headers,
  body,
}: ResponseOptions): OutgoingResponse {
  const outgoingResponse = new OutgoingResponse(Fields.fromList(headers));

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

  outgoingResponse.setStatusCode(status);

  OutgoingBody.finish(outgoingBody, undefined);

  return outgoingResponse;
}
